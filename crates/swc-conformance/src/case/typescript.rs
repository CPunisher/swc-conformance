use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
    sync::Arc,
};

use serde::Deserialize;
use tsrs_testrunner::compiler_runner;

use crate::{Result, workspace_root};

pub struct TypeScriptCase {
    pub input: Arc<Input>,
    suite: &'static str,
    name: String,
    config: String,
    pub settings: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Input {
    pub path: PathBuf,
    #[serde(skip)]
    pub source: String,
    pub files: Vec<File>,
    pub tsconfig: Option<File>,
    pub current_directory: String,
    pub options: BTreeMap<String, String>,
    pub symlinks: BTreeMap<String, String>,
    variants: Vec<Variant>,
}

#[derive(Deserialize)]
pub struct File {
    pub name: String,
    pub lines: Vec<usize>,
}

#[derive(Deserialize)]
struct Variant {
    name: String,
    config: String,
    options: BTreeMap<String, String>,
    diagnostics: Vec<ExpectedDiagnostic>,
}

#[derive(Deserialize)]
struct ExpectedDiagnostic {
    codes: Vec<i32>,
}

impl TypeScriptCase {
    pub fn read() -> Result<Vec<Self>> {
        let codes = syntax_codes()?;
        let mut cases = Vec::new();
        for suite in ["compiler", "conformance"] {
            let directory = format!("typescript/{suite}");
            for mut input in super::read::<Input>(&directory)? {
                let path = workspace_root()
                    .join("fixtures/typescript/tsc/testdata/tests/cases")
                    .join(&input.path);
                let source = compiler_runner::read_test_file(path.to_str().unwrap());
                input.source = source.replace("\r\n", "\n");
                let variants = std::mem::take(&mut input.variants);
                let input = Arc::new(input);
                for variant in variants {
                    let mut negative = false;
                    for diagnostic in &variant.diagnostics {
                        if diagnostic.codes.iter().any(|code| codes.contains(code)) {
                            negative = true;
                            break;
                        }
                    }
                    if negative {
                        cases.push(Self {
                            input: Arc::clone(&input),
                            suite,
                            name: variant.name,
                            config: variant.config,
                            settings: variant.options,
                        });
                    }
                }
            }
        }
        Ok(cases)
    }

    pub fn path(&self) -> PathBuf {
        PathBuf::from(self.suite).join(format!("{}.json", self.name))
    }

    pub fn relative_path(&self) -> &std::path::Path {
        &self.input.path
    }

    pub fn configuration(&self) -> &str {
        &self.config
    }
}

pub fn syntax_codes() -> Result<BTreeSet<i32>> {
    super::json("typescript/syntax-codes.json")
}

pub fn libraries() -> Result<BTreeMap<String, String>> {
    let root = workspace_root().join("fixtures/typescript/tsc/testdata/tests/lib");
    let names: Vec<PathBuf> = super::json("typescript/libraries.json")?;
    let mut libraries = BTreeMap::new();
    for name in names {
        let content = fs::read_to_string(root.join(&name))?;
        libraries.insert(name.to_string_lossy().into_owned(), content);
    }
    Ok(libraries)
}
