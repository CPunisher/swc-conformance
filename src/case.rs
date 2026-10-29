use std::path::PathBuf;

pub mod swc_minifier;
pub mod test262;
pub mod typescript;

pub struct Case {
    pub path: PathBuf,
    pub source: String,
    pub module: bool,
    pub jsx: bool,
}
