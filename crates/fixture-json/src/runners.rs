use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, de::DeserializeOwned};
use serde_json::json;

use crate::{Output, Result, files, workspace_root};

const STRICT: &str = "\"use strict\";\n";

#[derive(Deserialize)]
struct Test262 {
    path: PathBuf,
    metadata: Option<Metadata>,
    modes: Vec<String>,
}

#[derive(Default, Deserialize)]
struct Metadata {
    flags: Vec<String>,
    negative: Option<Negative>,
}

#[derive(Deserialize)]
struct Negative {
    phase: String,
    #[serde(rename = "type")]
    kind: Option<String>,
}

#[derive(Deserialize)]
struct Minifier {
    path: PathBuf,
    module: bool,
    jsx: bool,
}

#[derive(Deserialize)]
struct TypeScript {
    variants: Vec<Variant>,
}

#[derive(Deserialize)]
struct Variant {
    name: String,
    config: String,
    options: BTreeMap<String, String>,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Deserialize)]
struct Diagnostic {
    codes: Vec<i32>,
}

pub fn read() -> Result<Output> {
    let root = workspace_root().join("fixtures-json");
    let mut swc = Vec::new();
    let mut v8 = Vec::new();
    for suite in ["test262", "test262-legacy"] {
        for path in files(&root.join(suite))? {
            let input: Test262 = read_json(&path)?;
            let meta = input.metadata.unwrap_or_default();
            let has = |name: &str| meta.flags.iter().any(|flag| flag == name);
            let module = has("module");
            let prefix = if has("onlyStrict") && !module {
                STRICT
            } else {
                ""
            };
            let parse_error = meta
                .negative
                .as_ref()
                .is_some_and(|negative| matches!(negative.phase.as_str(), "parse" | "early"));
            let language =
                input.path.starts_with("language") || input.path.starts_with("annexB/language");
            let source = PathBuf::from(suite).join("test").join(&input.path);
            if language && !parse_error {
                swc.push(json!({
                    "path": source,
                    "snapshot": source.with_added_extension("json"),
                    "module": module,
                    "jsx": false,
                    "prefix": prefix,
                }));
            }
            let syntax_error = meta.negative.as_ref().is_some_and(|negative| {
                parse_error && negative.kind.as_deref() == Some("SyntaxError")
            });
            let name = input.path.file_name().unwrap().to_string_lossy();
            if syntax_error && !name.contains("_FIXTURE") {
                for mode in input.modes {
                    let mut snapshot =
                        PathBuf::from(format!("{}.{}.json", input.path.display(), mode));
                    if suite == "test262-legacy" {
                        snapshot = PathBuf::from("legacy").join(snapshot);
                    }
                    v8.push(json!({
                        "path": source,
                        "snapshot": snapshot,
                        "module": mode == "module",
                        "jsx": false,
                        "prefix": if mode == "strict" { STRICT } else { "" },
                    }));
                }
            }
        }
    }
    for path in files(&root.join("swc-minifier"))? {
        let input: Minifier = read_json(&path)?;
        swc.push(json!({
            "path": input.path,
            "snapshot": input.path.with_added_extension("json"),
            "module": input.module,
            "jsx": input.jsx,
            "prefix": "",
        }));
    }
    let typescript = root.join("typescript");
    let codes: BTreeSet<i32> = read_json(&typescript.join("syntax-codes.json"))?;
    let mut tsrs = Vec::new();
    for suite in ["compiler", "conformance"] {
        for path in files(&typescript.join(suite))? {
            let input: TypeScript = read_json(&path)?;
            for variant in input.variants {
                if syntax_error(&variant.diagnostics, &codes) {
                    tsrs.push(json!({
                        "fixture": path.strip_prefix(&typescript)?,
                        "snapshot": PathBuf::from(suite).join(format!("{}.json", variant.name)),
                        "config": variant.config,
                        "options": variant.options,
                    }));
                }
            }
        }
    }
    Ok(vec![
        ("swc.json".into(), json!(swc)),
        ("v8.json".into(), json!(v8)),
        ("tsrs.json".into(), json!(tsrs)),
    ])
}

fn syntax_error(diagnostics: &[Diagnostic], codes: &BTreeSet<i32>) -> bool {
    for diagnostic in diagnostics {
        for code in &diagnostic.codes {
            if codes.contains(code) {
                return true;
            }
        }
    }
    false
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = fs::read(path)?;
    let value =
        serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(value)
}
