use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::Result;

pub fn files(root: &Path) -> Result<Vec<PathBuf>> {
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
