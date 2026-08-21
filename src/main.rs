use std::error::Error;

use clap::{Parser, Subcommand};

pub mod resolver;

#[derive(Parser)]
#[command(about = "Generate SWC conformance snapshots")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Resolver,
}

#[derive(Clone, Copy)]
struct FixtureSet {
    input: &'static str,
    output: &'static str,
    extensions: &'static [&'static str],
    file_name: Option<&'static str>,
    jsx: bool,
}

fn main() -> Result<(), Box<dyn Error>> {
    match Cli::parse().command {
        Command::Resolver => resolver::run(),
    }
}

fn workspace_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}
