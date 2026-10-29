use std::{fs, path::PathBuf};

use saphyr::{LoadableYamlNode, Yaml};

use crate::{Result, files::files, workspace_root};

pub struct Test262Case {
    pub path: PathBuf,
    pub source: String,
    pub mode: &'static str,
}

#[derive(Default)]
pub struct Meta {
    pub module: bool,
    pub only_strict: bool,
    pub no_strict: bool,
    pub raw: bool,
    pub negative: bool,
    pub syntax_error: bool,
}

impl Test262Case {
    pub fn read() -> Result<Vec<Self>> {
        let root = workspace_root().join("fixtures/test262/test");
        let mut cases = Vec::new();
        for path in files(&root)? {
            let name = path.file_name().unwrap().to_string_lossy();
            if path.extension().is_none_or(|extension| extension != "js")
                || name.contains("_FIXTURE")
            {
                continue;
            }
            let source = fs::read_to_string(&path)?;
            let meta = metadata(&source)?;
            if !meta.negative || !meta.syntax_error {
                continue;
            }
            let path = path.strip_prefix(&root)?.to_path_buf();
            if meta.module {
                cases.push(Self {
                    path,
                    source,
                    mode: "module",
                });
                continue;
            }
            if !meta.only_strict {
                cases.push(Self {
                    path: path.clone(),
                    source: source.clone(),
                    mode: "sloppy",
                });
            }
            if !meta.no_strict && !meta.raw {
                cases.push(Self {
                    path,
                    source: format!("\"use strict\";\n{source}"),
                    mode: "strict",
                });
            }
        }
        Ok(cases)
    }

    pub fn path(&self) -> PathBuf {
        format!("{}.{}.json", self.path.display(), self.mode).into()
    }
}

pub fn resolver_cases() -> Result<Vec<super::Case>> {
    let fixtures = workspace_root().join("fixtures");
    let mut cases = Vec::new();
    for directory in ["test262/test/annexB/language", "test262/test/language"] {
        let input = fixtures.join(directory);
        if !input.exists() {
            continue;
        }
        for path in files(&input)? {
            if path.extension().is_none_or(|extension| extension != "js") {
                continue;
            }
            let mut source = fs::read_to_string(&path)?;
            let meta = metadata(&source)?;
            if meta.negative {
                continue;
            }
            if meta.only_strict && !meta.module {
                source = format!("\"use strict\";\n{source}");
            }
            cases.push(super::Case {
                path: path.strip_prefix(&fixtures)?.to_path_buf(),
                source,
                module: meta.module,
                jsx: false,
            });
        }
    }
    Ok(cases)
}

pub fn metadata(source: &str) -> Result<Meta> {
    let Some((_, source)) = source.split_once("/*---") else {
        return Ok(Meta::default());
    };
    let (source, _) = source
        .split_once("---*/")
        .ok_or("unterminated Test262 metadata")?;
    let yaml = Yaml::load_from_str(source)?;
    let value = yaml.first().ok_or("missing Test262 metadata")?;
    let mut meta = Meta::default();
    if let Some(negative) = value.as_mapping_get("negative")
        && !negative.is_null()
        && !negative.is_badvalue()
    {
        let phase = negative.as_mapping_get("phase").and_then(Yaml::as_str);
        let kind = negative.as_mapping_get("type").and_then(Yaml::as_str);
        meta.negative = match phase {
            Some("resolution" | "runtime") => false,
            Some("parse" | "early") | None => true,
            Some(phase) => return Err(format!("invalid Test262 negative phase: {phase}").into()),
        };
        meta.syntax_error = kind == Some("SyntaxError");
        if negative.as_str() == Some("SyntaxError") {
            meta.negative = true;
            meta.syntax_error = true;
        }
    }
    if let Some(flags) = value.as_mapping_get("flags").and_then(Yaml::as_sequence) {
        for flag in flags {
            match flag.as_str() {
                Some("module") => meta.module = true,
                Some("onlyStrict") => meta.only_strict = true,
                Some("noStrict") => meta.no_strict = true,
                Some("raw") => meta.raw = true,
                _ => {}
            }
        }
    }
    Ok(meta)
}
