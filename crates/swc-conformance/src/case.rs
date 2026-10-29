use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, de::DeserializeOwned};

use crate::{Result, workspace_root};

pub mod typescript;

#[derive(Deserialize)]
pub struct Case {
    path: PathBuf,
    prefix: String,
    pub snapshot: PathBuf,
    #[serde(skip)]
    pub source: String,
    pub module: bool,
    pub jsx: bool,
}

pub fn read(runner: &str) -> Result<Vec<Case>> {
    let mut cases: Vec<Case> = json(format!("runners/{runner}.json"))?;
    let root = workspace_root().join("fixtures");
    for case in &mut cases {
        let source = fs::read_to_string(root.join(&case.path))?;
        case.source = format!("{}{source}", case.prefix);
    }
    Ok(cases)
}

pub fn json<T: DeserializeOwned>(relative: impl AsRef<Path>) -> Result<T> {
    let path = workspace_root().join("fixtures-json").join(relative);
    let bytes = fs::read(&path)?;
    let value =
        serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(value)
}
