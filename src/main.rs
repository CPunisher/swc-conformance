use std::{
    error::Error,
    ffi::OsString,
    fmt::Write,
    fs,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand};
use swc_experimental_allocator::Allocator;
use swc_experimental_ecma_ast::{EsVersion, Ident, Visit};
use swc_experimental_ecma_parser::{EsSyntax, Syntax, parse_file_as_program};
use swc_experimental_ecma_semantic::resolver::{Semantic, resolver};
use walkdir::WalkDir;

#[derive(Parser)]
#[command(about = "Generate SWC conformance snapshots")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate identifier resolution snapshots with the experimental resolver.
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

const RESOLVER_FIXTURE_SETS: &[FixtureSet] = &[
    FixtureSet {
        input: "fixtures/test262/test/annexB/language",
        output: "tests/resolver/test262/test/annexB/language",
        extensions: &["js"],
        file_name: None,
        jsx: false,
    },
    FixtureSet {
        input: "fixtures/test262/test/language",
        output: "tests/resolver/test262/test/language",
        extensions: &["js"],
        file_name: None,
        jsx: false,
    },
    FixtureSet {
        input: "fixtures/swc/crates/swc_ecma_minifier/tests/fixture/issues",
        output: "tests/resolver/swc/crates/swc_ecma_minifier/tests/fixture/issues",
        extensions: &["js"],
        file_name: Some("input.js"),
        jsx: true,
    },
];

struct ResolverDisplayVisitor<'a, 'b> {
    semantic: &'a Semantic,
    output: &'b mut String,
}

impl<'a> Visit<'a> for ResolverDisplayVisitor<'_, '_> {
    fn visit_ident(&mut self, node: &Ident) {
        let scope = if node.symbol_id.get().is_some() {
            self.semantic.node_scope(node)
        } else {
            self.semantic.unresolved_scope_id()
        };
        let _ = writeln!(self.output, "{} ({:?}) -> {:?}", node.sym, node.sym, scope,);
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    match Cli::parse().command {
        Command::Resolver => generate_resolver_snapshots(),
    }
}

fn generate_resolver_snapshots() -> Result<(), Box<dyn Error>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut total_generated = 0;
    let mut total_skipped = 0;

    for fixture_set in RESOLVER_FIXTURE_SETS {
        let input_root = manifest_dir.join(fixture_set.input);
        if !input_root.is_dir() {
            return Err(format!(
                "fixture directory does not exist: {} (run scripts/clone_fixtures.sh first)",
                input_root.display()
            )
            .into());
        }
    }

    clean_output_root(&manifest_dir.join("tests/resolver"))?;

    for fixture_set in RESOLVER_FIXTURE_SETS {
        let input_root = manifest_dir.join(fixture_set.input);
        let output_root = manifest_dir.join(fixture_set.output);

        let mut generated = 0;
        let mut skipped = 0;
        for path in fixture_files(&input_root, fixture_set.extensions, fixture_set.file_name) {
            let Some(snapshot) = resolver_snapshot(&path, fixture_set.jsx) else {
                skipped += 1;
                continue;
            };

            let relative_path = path.strip_prefix(&input_root)?;
            let snapshot_path = snapshot_path(&output_root, relative_path);

            if let Some(parent) = snapshot_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(snapshot_path, snapshot)?;
            generated += 1;
        }

        println!(
            "{}: generated {generated}, skipped {skipped} files that could not be processed",
            fixture_set.input
        );
        total_generated += generated;
        total_skipped += skipped;
    }

    println!("resolver: generated {total_generated}, skipped {total_skipped} files in total");
    Ok(())
}

fn clean_output_root(output_root: &Path) -> Result<(), Box<dyn Error>> {
    if output_root.exists() {
        fs::remove_dir_all(output_root)?;
    }
    Ok(())
}

fn fixture_files(root: &Path, extensions: &[&str], file_name: Option<&str>) -> Vec<PathBuf> {
    let mut files = WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| {
            file_name.is_none_or(|file_name| path.file_name().is_some_and(|name| name == file_name))
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extensions.contains(&extension))
        })
        .collect::<Vec<_>>();
    files.sort_unstable();
    files
}

fn syntax_for(jsx: bool) -> Syntax {
    Syntax::Es(EsSyntax {
        jsx,
        decorators: true,
        decorators_before_export: true,
        auto_accessors: true,
        explicit_resource_management: true,
        ..Default::default()
    })
}

fn resolver_snapshot(path: &Path, jsx: bool) -> Option<String> {
    let source_text = fs::read_to_string(path).ok()?;
    let allocator = Allocator::new();

    // Experimental parser and resolver panics are isolated to the current
    // fixture so one unsupported AST does not stop the entire generation run.
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let snapshot = catch_unwind(AssertUnwindSafe(|| {
        let program = parse_file_as_program(
            &allocator,
            &source_text,
            syntax_for(jsx),
            EsVersion::EsNext,
            None,
        )
        .ok()?;
        let semantic = resolver(&program);
        let mut output = String::new();
        let _ = writeln!(output, "Top level: {:?}", semantic.top_level_scope_id());
        let _ = writeln!(output, "Unresolved: {:?}", semantic.unresolved_scope_id());
        let mut visitor = ResolverDisplayVisitor {
            semantic: &semantic,
            output: &mut output,
        };
        visitor.visit_program(&program);
        Some(output)
    }));
    std::panic::set_hook(previous_hook);

    snapshot.ok().flatten()
}

fn snapshot_path(output_root: &Path, relative_path: &Path) -> PathBuf {
    let mut path = OsString::from(relative_path.as_os_str());
    path.push(".snap");
    output_root.join(path)
}
