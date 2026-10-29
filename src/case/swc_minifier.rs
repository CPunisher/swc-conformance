use std::fs;

use crate::{Result, case::Case, files::files, workspace_root};

pub fn read() -> Result<Vec<Case>> {
    let fixtures = workspace_root().join("fixtures");
    let mut cases = Vec::new();
    let root = fixtures.join("swc/crates/swc_ecma_minifier/tests/fixture/issues");
    for path in files(&root)? {
        if path.file_name().unwrap() != "input.js" {
            continue;
        }
        cases.push(Case {
            path: path.strip_prefix(&fixtures)?.to_path_buf(),
            source: fs::read_to_string(path)?,
            module: false,
            jsx: true,
        });
    }
    Ok(cases)
}
