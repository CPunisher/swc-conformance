use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, de::DeserializeOwned};
use walkdir::WalkDir;

use crate::{Result, workspace_root};

pub mod swc_minifier;
pub mod test262;
pub mod typescript;

#[derive(Deserialize)]
pub struct Case {
    pub path: PathBuf,
    #[serde(skip)]
    pub source: String,
    pub module: bool,
    pub jsx: bool,
}

pub fn read<T: DeserializeOwned>(directory: &str) -> Result<Vec<T>> {
    let root = workspace_root().join("fixtures-json");
    let mut cases = Vec::new();
    for path in files(&root.join(directory))? {
        if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            cases.push(json(path.strip_prefix(&root)?)?);
        }
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

fn files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in WalkDir::new(root) {
        let entry = entry?;
        if entry.file_type().is_file() {
            paths.push(entry.into_path());
        }
    }
    paths.sort();
    Ok(paths)
}
