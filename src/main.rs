use std::{
    error::Error,
    ffi::OsString,
    fmt::Write,
    fs,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand};
use swc_core::{
    common::{FileName, GLOBALS, Globals, Mark, SourceMap, SyntaxContext, sync::Lrc},
    ecma::{
        ast::{EsVersion, Ident, Program},
        parser::{EsSyntax, Syntax, TsSyntax, parse_file_as_program},
        transforms::base::resolver,
        visit::{Visit, VisitMutWith, VisitWith},
    },
};
use walkdir::WalkDir;

#[derive(Parser)]
#[command(about = "Generate SWC conformance snapshots")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate identifier resolution snapshots with swc_core's resolver.
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
        input: "fixtures/typescript/tests/cases/compiler",
        output: "tests/resolver/typescript/tests/cases/compiler",
        extensions: &["js", "jsx", "ts", "tsx"],
        file_name: None,
        jsx: false,
    },
    FixtureSet {
        input: "fixtures/typescript/tests/cases/conformance",
        output: "tests/resolver/typescript/tests/cases/conformance",
        extensions: &["js", "jsx", "ts", "tsx"],
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

struct ResolverDisplayVisitor<'a> {
    output: &'a mut String,
}

impl Visit for ResolverDisplayVisitor<'_> {
    fn visit_ident(&mut self, node: &Ident) {
        let _ = writeln!(
            self.output,
            "{} ({:?}) -> {:?}",
            node.sym, node.sym, node.ctxt,
        );
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
        let output_root = manifest_dir.join(fixture_set.output);

        if !input_root.is_dir() {
            return Err(format!(
                "fixture directory does not exist: {} (run scripts/clone_fixtures.sh first)",
                input_root.display()
            )
            .into());
        }

        clean_output_root(&output_root)?;

        let mut generated = 0;
        let mut skipped = 0;
        for path in fixture_files(&input_root, fixture_set.extensions, fixture_set.file_name) {
            let Some(program) = parse(&path, fixture_set.jsx) else {
                skipped += 1;
                continue;
            };

            let relative_path = path.strip_prefix(&input_root)?;
            let snapshot_path = snapshot_path(&output_root, relative_path);
            let is_typescript = matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("ts" | "tsx")
            );

            if let Some(parent) = snapshot_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(snapshot_path, resolver_snapshot(program, is_typescript))?;
            generated += 1;
        }

        println!(
            "{}: generated {generated}, skipped {skipped} files that did not parse",
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

fn syntax_for(path: &Path, jsx: bool) -> Syntax {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("ts" | "tsx") => Syntax::Typescript(TsSyntax {
            tsx: path.extension().is_some_and(|extension| extension == "tsx"),
            decorators: true,
            dts: path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().ends_with(".d.ts")),
            ..Default::default()
        }),
        _ => Syntax::Es(EsSyntax {
            jsx: jsx || path.extension().is_some_and(|extension| extension == "jsx"),
            decorators: true,
            decorators_before_export: true,
            auto_accessors: true,
            explicit_resource_management: true,
            ..Default::default()
        }),
    }
}

fn parse(path: &Path, jsx: bool) -> Option<Program> {
    let source_text = fs::read_to_string(path).ok()?;
    let source_map: Lrc<SourceMap> = Default::default();
    let source_file =
        source_map.new_source_file(FileName::Real(path.to_path_buf()).into(), source_text);
    let mut recovered_errors = Vec::new();

    // Some parser fixtures currently trigger internal parser panics. They are
    // treated like any other input that cannot be parsed; resolver panics are
    // intentionally not caught.
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let parsed = catch_unwind(AssertUnwindSafe(|| {
        parse_file_as_program(
            &source_file,
            syntax_for(path, jsx),
            EsVersion::EsNext,
            None,
            &mut recovered_errors,
        )
    }));
    std::panic::set_hook(previous_hook);

    let program = parsed.ok()?.ok()?;
    recovered_errors.is_empty().then_some(program)
}

fn resolver_snapshot(mut program: Program, is_typescript: bool) -> String {
    GLOBALS.set(&Globals::new(), || {
        let unresolved_mark = Mark::new();
        let top_level_mark = Mark::new();

        program.visit_mut_with(&mut resolver(
            unresolved_mark,
            top_level_mark,
            is_typescript,
        ));

        let mut output = String::new();
        let _ = writeln!(
            output,
            "Top level: {:?}",
            SyntaxContext::empty().apply_mark(top_level_mark)
        );
        let _ = writeln!(
            output,
            "Unresolved: {:?}",
            SyntaxContext::empty().apply_mark(unresolved_mark)
        );
        program.visit_with(&mut ResolverDisplayVisitor {
            output: &mut output,
        });
        output
    })
}

fn snapshot_path(output_root: &Path, relative_path: &Path) -> PathBuf {
    let mut path = OsString::from(relative_path.as_os_str());
    path.push(".snap");
    output_root.join(path)
}
