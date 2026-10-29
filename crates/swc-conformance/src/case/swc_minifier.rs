use std::fs;

use crate::{Result, case::Case, workspace_root};

pub fn read() -> Result<Vec<Case>> {
    let root = workspace_root().join("fixtures");
    let mut cases: Vec<Case> = super::read("swc-minifier")?;
    for case in &mut cases {
        case.source = fs::read_to_string(root.join(&case.path))?;
    }
    Ok(cases)
}
