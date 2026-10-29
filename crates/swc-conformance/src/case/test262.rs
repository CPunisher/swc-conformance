use std::{fs, path::PathBuf};

use serde::Deserialize;

use crate::{Result, workspace_root};

pub struct Test262Case {
    pub path: PathBuf,
    pub source: String,
    pub mode: String,
}

#[derive(Deserialize)]
struct Input {
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

impl Metadata {
    fn flag(&self, name: &str) -> bool {
        self.flags.iter().any(|flag| flag == name)
    }

    fn parse_error(&self) -> bool {
        self.negative
            .as_ref()
            .is_some_and(|negative| matches!(negative.phase.as_str(), "parse" | "early"))
    }
}

impl Test262Case {
    pub fn read() -> Result<Vec<Self>> {
        let mut cases = Vec::new();
        for input in super::read::<Input>("test262")? {
            let name = input.path.file_name().unwrap().to_string_lossy();
            if name.contains("_FIXTURE") {
                continue;
            }
            let meta = input.metadata.unwrap_or_default();
            if !meta.parse_error() {
                continue;
            }
            let negative = meta.negative.unwrap();
            if negative.kind.as_deref() != Some("SyntaxError") {
                continue;
            }
            let path = workspace_root()
                .join("fixtures/test262/test")
                .join(&input.path);
            let source = fs::read_to_string(path)?;
            for mode in input.modes {
                let source = if mode == "strict" {
                    format!("\"use strict\";\n{source}")
                } else {
                    source.clone()
                };
                cases.push(Self {
                    path: input.path.clone(),
                    source,
                    mode,
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
    let mut cases = Vec::new();
    for input in super::read::<Input>("test262")? {
        if !input.path.starts_with("language") && !input.path.starts_with("annexB/language") {
            continue;
        }
        let meta = input.metadata.unwrap_or_default();
        if meta.parse_error() {
            continue;
        }
        let path = workspace_root()
            .join("fixtures/test262/test")
            .join(&input.path);
        let source = fs::read_to_string(path)?;
        let module = meta.flag("module");
        let source = if meta.flag("onlyStrict") && !module {
            format!("\"use strict\";\n{source}")
        } else {
            source
        };
        cases.push(super::Case {
            path: PathBuf::from("test262/test").join(input.path),
            source,
            module,
            jsx: false,
        });
    }
    Ok(cases)
}
