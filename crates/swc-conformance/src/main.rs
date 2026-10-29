use std::{error::Error, fs, path::PathBuf};

use clap::{Parser, Subcommand};

mod case;
mod runner;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Parser)]
#[command(about = "Generate conformance snapshots")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Swc,
    V8,
    Tsrs,
}

fn main() -> Result<()> {
    let (directory, output) = match Cli::parse().command {
        Command::Swc => ("tests/resolver", runner::swc::run()?),
        Command::V8 => ("tests/parser/test262", runner::v8::run()?),
        Command::Tsrs => ("tests/parser/typescript", runner::tsrs::run()?),
    };
    let root = workspace_root().join(directory);
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
    println!("{directory}: updated {count}");
    Ok(())
}

fn workspace_root() -> PathBuf {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.pop();
    root.pop();
    root
}
