use serde_json::json;

use crate::{Output, Result, files, workspace_root};

pub fn read() -> Result<Output> {
    let fixtures = workspace_root().join("fixtures");
    let root = fixtures.join("swc/crates/swc_ecma_minifier/tests/fixture/issues");
    let mut output = Vec::new();
    for path in files(&root)? {
        if path.file_name().unwrap() != "input.js" {
            continue;
        }
        let value = json!({
            "path": path.strip_prefix(&fixtures)?,
            "module": false,
            "jsx": true,
        });
        output.push((
            path.strip_prefix(&root)?.with_added_extension("json"),
            value,
        ));
    }
    Ok(output)
}
