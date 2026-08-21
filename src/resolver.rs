use std::{
    borrow::Cow,
    error::Error,
    ffi::OsString,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
};

use saphyr::{LoadableYamlNode, Yaml};
use swc_experimental_allocator::Allocator;
use swc_experimental_ecma_ast::{EsVersion, Ident, Program, Visit};
use swc_experimental_ecma_parser::{EsSyntax, Syntax, with_file_parser};
use swc_experimental_ecma_semantic::resolver::{Semantic, resolver};
use walkdir::WalkDir;

use crate::{FixtureSet, workspace_root};

const RESOLVER_FIXTURE_SETS: &[FixtureSet] = &[
    FixtureSet {
        input: "fixtures/test262/test/annexB/language",
        output: "tests/resolver/test262/test/annexB/language",
        extensions: &["js"],
        file_name: None,
        jsx: false,
        test262: true,
    },
    FixtureSet {
        input: "fixtures/test262/test/language",
        output: "tests/resolver/test262/test/language",
        extensions: &["js"],
        file_name: None,
        jsx: false,
        test262: true,
    },
    FixtureSet {
        input: "fixtures/swc/crates/swc_ecma_minifier/tests/fixture/issues",
        output: "tests/resolver/swc/crates/swc_ecma_minifier/tests/fixture/issues",
        extensions: &["js"],
        file_name: Some("input.js"),
        jsx: true,
        test262: false,
    },
];

pub(crate) fn run() -> Result<(), Box<dyn Error>> {
    let workspace_root = workspace_root();
    let mut total_generated = 0;
    let mut total_skipped = 0;

    let output_root = workspace_root.join("tests/resolver");
    if output_root.exists() {
        fs::remove_dir_all(&output_root)?;
    }

    for fixture_set in RESOLVER_FIXTURE_SETS {
        let input_root = workspace_root.join(fixture_set.input);
        let output_root = workspace_root.join(fixture_set.output);

        let mut generated = 0;
        let mut skipped = 0;
        for path in fixture_files(&input_root, fixture_set.extensions, fixture_set.file_name) {
            let Some(snapshot) = resolver_snapshot(&path, fixture_set.jsx, fixture_set.test262)
            else {
                skipped += 1;
                continue;
            };

            let relative_path = path.strip_prefix(&input_root)?;
            let mut path = OsString::from(relative_path.as_os_str());
            path.push(".snap");
            let snapshot_path = output_root.join(path);

            if let Some(parent) = snapshot_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(snapshot_path, snapshot)?;
            generated += 1;
        }

        println!(
            "{}: generated {generated}, skipped {skipped} files that failed parsing, produced diagnostics, or panicked",
            fixture_set.input
        );
        total_generated += generated;
        total_skipped += skipped;
    }

    println!("resolver: generated {total_generated}, skipped {total_skipped} files in total");
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

fn resolver_snapshot(path: &Path, jsx: bool, test262: bool) -> Option<String> {
    let source_text = fs::read_to_string(path).ok()?;
    let allocator = Allocator::new();
    let test262_options = test262
        .then(|| test262_parse_options(&source_text))
        .flatten()
        .unwrap_or_default();
    let source_text = prepare_source(&source_text, test262_options);

    let program = with_file_parser(
        &allocator,
        &source_text,
        Syntax::Es(EsSyntax {
            jsx,
            decorators: true,
            decorators_before_export: true,
            auto_accessors: true,
            explicit_resource_management: true,
            ..Default::default()
        }),
        EsVersion::EsNext,
        None,
        |parser| {
            let program = if test262_options.module {
                Program::Module(allocator.boxed(parser.parse_module().ok()?))
            } else {
                parser.parse_program().ok()?
            };
            // A successful parse can still recover from syntax errors and
            // return a partial AST. Do not pass such programs to resolver.
            parser.take_errors().is_empty().then_some(program)
        },
    )?;
    let semantic = resolver(&program);
    let mut output = String::new();
    let _ = writeln!(output, "Top level: {:?}", semantic.top_level_scope_id());
    let _ = writeln!(output, "Unresolved: {:?}", semantic.unresolved_scope_id());
    let mut visitor = SnapshotWriter {
        semantic: &semantic,
        output: &mut output,
    };
    visitor.visit_program(&program);
    Some(output)
}

#[derive(Clone, Copy, Default)]
struct Test262ParseOptions {
    module: bool,
    only_strict: bool,
}

fn test262_parse_options(source: &str) -> Option<Test262ParseOptions> {
    let meta_start = source.find("/*---")?;
    let meta_end = source.find("---*/")?;
    let meta = &source[meta_start + 5..meta_end];
    let yaml = Yaml::load_from_str(meta).unwrap_or_default();
    let yaml = yaml.first()?;
    let flags = yaml
        .as_mapping_get("flags")
        .and_then(Yaml::as_sequence)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let has_flag = |expected| flags.iter().any(|flag| flag.as_str() == Some(expected));

    Some(Test262ParseOptions {
        module: has_flag("module"),
        only_strict: has_flag("onlyStrict"),
    })
}

fn prepare_source(source: &str, options: Test262ParseOptions) -> Cow<'_, str> {
    if options.only_strict && !options.module {
        Cow::Owned(format!("\"use strict\";\n{source}"))
    } else {
        Cow::Borrowed(source)
    }
}

struct SnapshotWriter<'a, 'b> {
    semantic: &'a Semantic,
    output: &'b mut String,
}

impl<'a> Visit<'a> for SnapshotWriter<'_, '_> {
    fn visit_ident(&mut self, node: &Ident) {
        let scope = if node.symbol_id.get().is_some() {
            self.semantic.node_scope(node)
        } else {
            self.semantic.unresolved_scope_id()
        };
        let _ = writeln!(self.output, "{} ({:?}) -> {:?}", node.sym, node.sym, scope,);
    }
}
