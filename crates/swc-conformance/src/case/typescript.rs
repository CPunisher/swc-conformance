use std::{collections::BTreeMap, fs, path::PathBuf};

use serde::Deserialize;
use tsrs_testrunner::compiler_runner;

use crate::{Result, workspace_root};

pub struct TypeScriptCase {
    pub input: Input,
    snapshot: PathBuf,
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
}

#[derive(Deserialize)]
pub struct File {
    pub name: String,
    pub lines: Vec<usize>,
}

#[derive(Deserialize)]
struct Selection {
    fixture: PathBuf,
    snapshot: PathBuf,
    config: String,
    options: BTreeMap<String, String>,
}

impl TypeScriptCase {
    pub fn read() -> Result<Vec<Self>> {
        let selected: Vec<Selection> = super::json("runners/tsrs.json")?;
        let mut cases = Vec::new();
        let root = workspace_root().join("fixtures/typescript/tsc/testdata/tests/cases");
        for selected in selected {
            let mut input: Input = super::json(PathBuf::from("typescript").join(selected.fixture))?;
            let path = root.join(&input.path);
            let source = compiler_runner::read_test_file(path.to_str().unwrap());
            input.source = source.replace("\r\n", "\n");
            cases.push(Self {
                input,
                snapshot: selected.snapshot,
                config: selected.config,
                settings: selected.options,
            });
        }
        Ok(cases)
    }

    pub fn path(&self) -> PathBuf {
        self.snapshot.clone()
    }

    pub fn relative_path(&self) -> &std::path::Path {
        &self.input.path
    }

    pub fn configuration(&self) -> &str {
        &self.config
    }
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
