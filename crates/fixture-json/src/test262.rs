use std::fs;

use saphyr::{LoadableYamlNode, Yaml};
use serde_json::{Value, json};

use crate::{Output, Result, files, workspace_root};

pub fn read(suite: &str) -> Result<Output> {
    let root = workspace_root().join("fixtures").join(suite).join("test");
    let mut output = Vec::new();
    for path in files(&root)? {
        if path.extension().is_none_or(|extension| extension != "js") {
            continue;
        }
        let source = fs::read_to_string(&path)?;
        let meta = metadata(&source)?;
        let mut modes = Vec::new();
        let flags = meta["flags"].as_array();
        let has = |flag: &str| flags.is_some_and(|flags| flags.iter().any(|value| value == flag));
        if has("module") {
            modes.push("module");
        } else {
            if !has("onlyStrict") {
                modes.push("sloppy");
            }
            if !has("noStrict") && !has("raw") {
                modes.push("strict");
            }
        }
        let relative = path.strip_prefix(&root)?;
        let value = json!({
            "path": relative,
            "metadata": meta,
            "modes": modes,
        });
        output.push((relative.with_added_extension("json"), value));
    }
    Ok(output)
}

fn metadata(source: &str) -> Result<Value> {
    let Some((_, source)) = source.split_once("/*---") else {
        return Ok(Value::Null);
    };
    let (source, _) = source
        .split_once("---*/")
        .ok_or("unterminated Test262 metadata")?;
    let yaml = Yaml::load_from_str(source)?;
    let value = yaml.first().ok_or("missing Test262 metadata")?;
    let mut meta = json!({});
    for name in ["flags", "features", "includes"] {
        let mut values = Vec::new();
        if let Some(sequence) = value.as_mapping_get(name).and_then(Yaml::as_sequence) {
            for value in sequence {
                values.push(value.as_str().ok_or("invalid Test262 metadata list")?);
            }
        }
        meta[name] = json!(values);
    }
    meta["negative"] = Value::Null;
    if let Some(negative) = value.as_mapping_get("negative")
        && !negative.is_null()
        && !negative.is_badvalue()
    {
        let phase = negative
            .as_mapping_get("phase")
            .and_then(Yaml::as_str)
            .unwrap_or("parse");
        if !matches!(phase, "parse" | "early" | "resolution" | "runtime") {
            return Err(format!("invalid Test262 negative phase: {phase}").into());
        }
        let kind = if negative.as_str() == Some("SyntaxError") {
            Some("SyntaxError")
        } else {
            negative.as_mapping_get("type").and_then(Yaml::as_str)
        };
        meta["negative"] = json!({"phase": phase, "type": kind});
    }
    Ok(meta)
}
