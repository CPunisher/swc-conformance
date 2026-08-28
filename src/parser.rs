use std::{
    borrow::Cow,
    error::Error,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
};

use saphyr::{LoadableYamlNode, Yaml};
use swc_experimental_allocator::Allocator;
use swc_experimental_ecma_ast::{Comments, EsVersion};
use swc_experimental_ecma_parser::{
    EsSyntax, Lexer, Parser, StringSource, Syntax, unstable::Capturing,
};
use walkdir::WalkDir;

use crate::workspace_root;

const TEST262_ROOT: &str = "fixtures/test262/test";
const SNAPSHOT_ROOT: &str = "tests/parser/test262";

pub(crate) fn run() -> Result<(), Box<dyn Error>> {
    let workspace_root = workspace_root();
    let input_root = workspace_root.join(TEST262_ROOT);
    let output_root = workspace_root.join(SNAPSHOT_ROOT);
    let mut snapshots = [
        Snapshots::new(ParseMode::Module),
        Snapshots::new(ParseMode::Script),
        Snapshots::new(ParseMode::Program),
    ];

    for path in javascript_files(&input_root) {
        let source = fs::read_to_string(&path)?;
        let options = test262_parse_options(&source);
        let source = prepare_source(&source, options);
        let relative_path = path.strip_prefix(&input_root)?.to_string_lossy();
        for &mode in options.parse_modes() {
            let snapshot = &mut snapshots[mode.snapshot_index()];
            snapshot.record(&relative_path, parses_without_errors(&source, mode))?;
        }
    }

    if output_root.exists() {
        fs::remove_dir_all(&output_root)?;
    }
    fs::create_dir_all(&output_root)?;
    for snapshot in snapshots {
        snapshot.write(&output_root)?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParseMode {
    Module,
    Script,
    Program,
}

impl ParseMode {
    fn name(self) -> &'static str {
        match self {
            Self::Module => "module",
            Self::Script => "script",
            Self::Program => "program",
        }
    }

    fn snapshot_index(self) -> usize {
        match self {
            Self::Module => 0,
            Self::Script => 1,
            Self::Program => 2,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Test262ParseOptions {
    module: bool,
    only_strict: bool,
}

impl Test262ParseOptions {
    fn parse_modes(self) -> &'static [ParseMode] {
        if self.module {
            &[ParseMode::Module, ParseMode::Program]
        } else {
            &[ParseMode::Script, ParseMode::Program]
        }
    }
}

struct Snapshots {
    mode: ParseMode,
    passed: String,
    failed: String,
    passed_count: usize,
    failed_count: usize,
}

impl Snapshots {
    fn new(mode: ParseMode) -> Self {
        Self {
            mode,
            passed: String::new(),
            failed: String::new(),
            passed_count: 0,
            failed_count: 0,
        }
    }

    fn record(&mut self, relative_path: &str, passed: bool) -> Result<(), std::fmt::Error> {
        let output = if passed {
            self.passed_count += 1;
            &mut self.passed
        } else {
            self.failed_count += 1;
            &mut self.failed
        };
        writeln!(output, "{relative_path}")
    }

    fn write(self, output_root: &Path) -> Result<(), Box<dyn Error>> {
        let name = self.mode.name();
        fs::write(output_root.join(format!("{name}_pass.snap")), self.passed)?;
        fs::write(output_root.join(format!("{name}_fail.snap")), self.failed)?;
        println!(
            "parser ({name}): recorded {} passing and {} failing Test262 files",
            self.passed_count, self.failed_count
        );
        Ok(())
    }
}

fn javascript_files(root: &Path) -> Vec<PathBuf> {
    let mut files = WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "js"))
        .collect::<Vec<_>>();
    files.sort_unstable_by(|a, b| a.as_os_str().cmp(b.as_os_str()));
    files
}

fn test262_parse_options(source: &str) -> Test262ParseOptions {
    let Some(meta_start) = source.find("/*---") else {
        return Test262ParseOptions::default();
    };
    let meta = &source[meta_start + 5..];
    let Some(meta_end) = meta.find("---*/") else {
        return Test262ParseOptions::default();
    };
    let yaml = Yaml::load_from_str(&meta[..meta_end]).unwrap_or_default();
    let Some(yaml) = yaml.first() else {
        return Test262ParseOptions::default();
    };
    let flags = yaml
        .as_mapping_get("flags")
        .and_then(Yaml::as_sequence)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let has_flag = |expected| flags.iter().any(|flag| flag.as_str() == Some(expected));

    Test262ParseOptions {
        module: has_flag("module"),
        only_strict: has_flag("onlyStrict"),
    }
}

fn prepare_source(source: &str, options: Test262ParseOptions) -> Cow<'_, str> {
    if options.only_strict && !options.module {
        Cow::Owned(format!("\"use strict\";\n{source}"))
    } else {
        Cow::Borrowed(source)
    }
}

fn parses_without_errors(source: &str, mode: ParseMode) -> bool {
    let allocator = Allocator::new();
    let mut comments = Comments::new_in(&allocator);
    let lexer = Lexer::new(
        &allocator,
        Syntax::Es(EsSyntax {
            allow_return_outside_function: true,
            explicit_resource_management: true,
            import_attributes: true,
            ..Default::default()
        }),
        EsVersion::EsNext,
        StringSource::new(source),
        Some(&mut comments),
    );
    let lexer = Capturing::new(lexer);
    let mut parser = Parser::new_from(&allocator, lexer);
    let parsed = match mode {
        ParseMode::Module => parser.parse_module().is_ok(),
        ParseMode::Script => parser.parse_script().is_ok(),
        ParseMode::Program => parser.parse_program().is_ok(),
    };
    parsed && parser.take_errors().is_empty()
}
