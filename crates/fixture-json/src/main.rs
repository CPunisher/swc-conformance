use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use serde_json::Value;
use walkdir::WalkDir;

mod runners;
mod swc;
mod test262;
mod typescript;

type Result<T> = std::result::Result<T, Box<dyn Error>>;
type Output = Vec<(PathBuf, Value)>;

fn main() -> Result<()> {
    for suite in ["test262", "typescript", "swc-minifier", "runners"] {
        let output = match suite {
            "test262" => test262::read()?,
            "typescript" => typescript::read()?,
            "swc-minifier" => swc::read()?,
            "runners" => runners::read()?,
            _ => unreachable!(),
        };
        let root = workspace_root().join("fixtures-json").join(suite);
        if root.exists() {
            fs::remove_dir_all(&root)?;
        }
        fs::create_dir_all(&root)?;
        let count = output.len();
        for (path, value) in output {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap())?;
            fs::write(path, serde_json::to_vec_pretty(&value)?)?;
        }
        println!("fixtures-json/{suite}: updated {count}");
    }
    Ok(())
}

fn workspace_root() -> PathBuf {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.pop();
    root.pop();
    root
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
