use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use serde_json::{Value, json};
use tsrs_compiler::{self as compiler, Context, ProgramOptions};
use tsrs_core::{
    P, Tristate,
    arena::Region,
    tspath::{self, ComparePathsOptions},
};
use tsrs_testrunner::{
    compiler_runner,
    options::{parse_test_ts_config, test_compiler_options, unsupported_reason},
    test_case_parser::{TestCaseContent, TestUnit},
};
use tsrs_vfs::{FS, bundled::embed as bundled, vfstest};

use crate::{
    Result,
    case::typescript::{self, File, TypeScriptCase},
    runner::Output,
};

const REVISION: &str = "677622d08ef3c12f370b9342623a41d156687c0c";

pub fn run() -> Result<Output> {
    let cases = TypeScriptCase::read()?;
    let runner = Tsrs {
        codes: crate::case::json("typescript/syntax-codes.json")?,
    };
    let libraries = typescript::libraries()?;
    let mut output = Vec::new();
    for case in cases {
        let value = runner
            .parse(&case, &libraries)
            .map_err(|error| format!("{}: {error}", case.path().display()))?;
        output.push((case.path(), value));
    }
    let version =
        json!({ "tsrs": REVISION, "typescript": tsrs_core::version(), "offsetEncoding": "utf8" });
    output.push(("version.json".into(), version));
    Ok(output)
}

struct Tsrs {
    codes: BTreeSet<i32>,
}

impl Tsrs {
    fn parse(&self, case: &TypeScriptCase, libraries: &BTreeMap<String, String>) -> Result<Value> {
        let region = Region::new(1024 * 1024);
        let _scope = region.enter();
        let mut options = compiler_input(case, libraries)?;
        options.single_threaded = Tristate::True;
        let program = compiler::new_program(options);
        let context = Context::default();
        let mut diagnostics = Vec::new();
        for diagnostic in program.get_config_file_parsing_diagnostics() {
            if self.codes.contains(&diagnostic.code()) {
                diagnostics.push(diagnostic);
            }
        }
        diagnostics.extend(program.get_syntactic_diagnostics(&context, None));
        diagnostics.extend(program.get_bind_diagnostics(&context, None));
        for diagnostic in program.get_semantic_diagnostics(&context, None) {
            if self.codes.contains(&diagnostic.code()) {
                diagnostics.push(diagnostic);
            }
        }
        let mut output = Vec::new();
        for diagnostic in compiler::sort_and_deduplicate_diagnostics(&diagnostics) {
            if diagnostic.category() != tsrs_diagnostics::Category::Error {
                continue;
            }
            let Some(file) = diagnostic.file() else {
                continue;
            };
            if !file.file_name().starts_with("bundled:///") {
                output.push(snapshot(diagnostic));
            }
        }
        // The snapshot owns its data and no other program shares this program's data.
        unsafe { compiler::free_unshared_program(program) };
        let value = json!({
            "path": case.relative_path(),
            "config": case.configuration(),
            "diagnostics": output,
        });
        Ok(value)
    }
}

fn compiler_input(
    case: &TypeScriptCase,
    libraries: &BTreeMap<String, String>,
) -> Result<ProgramOptions> {
    let lines: Vec<_> = case.input.source.split('\n').collect();
    let mut files = Vec::new();
    for file in &case.input.files {
        files.push(test_unit(file, &lines)?);
    }
    let tsconfig = match &case.input.tsconfig {
        Some(file) => Some(test_unit(file, &lines)?),
        None => None,
    };
    let payload = TestCaseContent {
        test_unit_data: files,
        ts_config_file_unit_data: tsconfig,
        current_directory: case.input.current_directory.clone(),
        global_options: case.input.options.clone().into_iter().collect(),
        symlinks: case.input.symlinks.clone(),
    };
    let external = payload.global_options.get("runexternalcode");
    if external.is_some_and(|value| value == "true") {
        return Err("external content mappers are unsupported".into());
    }
    let tsconfig = parse_test_ts_config(&payload);
    let mut settings = case.settings.clone();
    let config_files = tsconfig.map(|config| config.get().parsed_config.file_names.as_slice());
    let units = compiler_runner::split_units(&payload, &mut settings, config_files);
    let cwd = &units.current_directory;
    let (mut options, _) = test_compiler_options(Some(&settings), tsconfig, cwd)?;
    if let Some(reason) = unsupported_reason(&options) {
        return Err(reason.into());
    }
    for path in [
        &mut options.out_dir,
        &mut options.project,
        &mut options.root_dir,
        &mut options.ts_build_info_file,
        &mut options.base_url,
        &mut options.declaration_dir,
    ] {
        if !path.is_empty() {
            *path = tspath::get_normalized_absolute_path(path, cwd);
        }
    }
    let directories = [&mut options.root_dirs, &mut options.type_roots];
    for paths in directories.into_iter().flatten() {
        for path in paths {
            *path = tspath::get_normalized_absolute_path(path, cwd);
        }
    }
    let sensitivity = settings.get("usecasesensitivefilenames");
    let case_sensitive = sensitivity.is_none_or(|value| value == "true");
    let absolute = |name: &str| tspath::get_normalized_absolute_path(name, cwd);
    let mut content = BTreeMap::new();
    for group in [
        &units.to_be_compiled,
        &units.other_files,
        &units.ts_config_files,
    ] {
        for file in group {
            content.insert(
                absolute(&file.unit_name),
                vfstest::MapFile::from(&file.content),
            );
        }
    }
    for (link, target) in &payload.symlinks {
        content.insert(absolute(link), vfstest::symlink(&absolute(target)));
    }
    for (name, source) in libraries {
        content.insert(format!("/.lib/{name}"), vfstest::MapFile::from(source));
    }
    let mut roots = Vec::new();
    for file in &units.to_be_compiled {
        if !file.unit_name.ends_with(".json") && !file.unit_name.ends_with(".tsbuildinfo") {
            roots.push(absolute(&file.unit_name));
        }
    }
    if let Some(libraries) = settings.get("libfiles") {
        for file in libraries.split(',') {
            let file = file.trim();
            if !file.is_empty() && (file != "lib.d.ts" || options.no_lib.is_true()) {
                roots.push(format!("/.lib/{file}"));
            }
        }
    }
    let mut base = cwd.clone();
    if let Some(config) = tsconfig {
        let directory = config.get_current_directory();
        if !directory.is_empty() {
            base = directory.to_owned();
        }
    }
    let mut config = tsrs_tsoptions::new_parsed_command_line(
        P::new(options),
        roots,
        Vec::new(),
        ComparePathsOptions {
            current_directory: base,
            use_case_sensitive_file_names: case_sensitive,
        },
    );
    if let Some(tsconfig) = tsconfig {
        config.config_file = tsconfig.config_file;
        config.errors = tsconfig.errors.clone();
    }
    let fs = vfstest::from_map(content, case_sensitive);
    let fs: Arc<dyn FS> = Arc::new(bundled::wrap_fs(fs));
    let host = compiler::new_compiler_host(cwd, fs, &bundled::lib_path(), None, None, None);
    Ok(ProgramOptions::new(P::new(config), host))
}

fn test_unit(file: &File, lines: &[&str]) -> Result<TestUnit> {
    let mut content = Vec::new();
    for &line in &file.lines {
        let text = lines
            .get(line)
            .ok_or_else(|| format!("invalid source line {line} for {}", file.name))?;
        content.push(*text);
    }
    Ok(TestUnit {
        name: file.name.clone(),
        content: content.join("\n"),
    })
}

fn snapshot(diagnostic: P<tsrs_ast::Diagnostic>) -> Value {
    let mut related = Vec::new();
    for &diagnostic in diagnostic.related_information() {
        related.push(snapshot(diagnostic));
    }
    let mut chain = Vec::new();
    for &diagnostic in diagnostic.message_chain() {
        chain.push(snapshot(diagnostic));
    }
    let mut value = json!({
        "start": diagnostic.pos(),
        "end": diagnostic.end(),
        "code": diagnostic.code(),
    });
    if let Some(file) = diagnostic.file() {
        value["file"] = json!(file.file_name());
    }
    if !related.is_empty() {
        value["related"] = json!(related);
    }
    if !chain.is_empty() {
        value["chain"] = json!(chain);
    }
    value
}
