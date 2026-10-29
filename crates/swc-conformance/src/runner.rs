use std::path::PathBuf;

use serde_json::Value;

pub mod swc;
pub mod tsrs;
pub mod v8;

pub type Output = Vec<(PathBuf, Value)>;
