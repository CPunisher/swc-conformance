use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use regex::Regex;
use tsrs_compiler::{self as compiler, ProgramOptions};
use tsrs_core::{
    P,
    tspath::{self, ComparePathsOptions},
};
use tsrs_testrunner::{
    compiler_runner::{self, TestItem},
    harnessutil::TestConfiguration,
    options::{
        parse_test_ts_config, test_compiler_options, tsoptions_option_table, unsupported_reason,
    },
    test_case_parser,
};
use tsrs_vfs::{FS, bundled::embed as bundled, vfstest};

use crate::{Result, files::files, workspace_root};

pub struct TypeScriptCase {
    item: TestItem,
    settings: TestConfiguration,
}

impl TypeScriptCase {
    pub fn read() -> Result<Vec<Self>> {
        let root = workspace_root().join("fixtures/typescript/tsc/testdata");
        let table = tsoptions_option_table();
        let codes = syntax_codes()?;
        let diagnostic = Regex::new(r"(?m)^!!! error TS(\d+):")?;
        let mut cases = Vec::new();
        for suite in ["compiler", "conformance"] {
            let input = root.join("tests/cases").join(suite);
            for path in files(&input)? {
                let extension = path.extension().and_then(|extension| extension.to_str());
                if !matches!(extension, Some("ts" | "tsx")) {
                    continue;
                }
                let variants =
                    compiler_runner::expand_test_file(suite, path.to_str().unwrap(), &table)?;
                for item in variants {
                    let reference = root.join("baselines/reference").join(suite);
                    let baseline = reference.join(format!("{}.errors.txt", item.name));
                    if !baseline.exists() {
                        continue;
                    }
                    let baseline = fs::read_to_string(baseline)?;
                    let mut negative = false;
                    for capture in diagnostic.captures_iter(&baseline) {
                        let code = capture[1].parse()?;
                        if codes.contains(&code) {
                            negative = true;
                            break;
                        }
                    }
                    if !negative {
                        continue;
                    }
                    let source = compiler_runner::read_test_file(&item.path);
                    let configuration =
                        compiler_runner::find_configuration(&source, &table, &item.config)?;
                    let settings = match configuration {
                        Some(configuration) => configuration.config,
                        None => TestConfiguration::default(),
                    };
                    cases.push(Self { item, settings });
                }
            }
        }
        Ok(cases)
    }

    pub fn path(&self) -> PathBuf {
        PathBuf::from(&self.item.suite).join(format!("{}.json", self.item.name))
    }

    pub fn relative_path(&self) -> PathBuf {
        let root = workspace_root().join("fixtures/typescript/tsc/testdata/tests/cases");
        let path = Path::new(&self.item.path).strip_prefix(root).unwrap();
        path.to_path_buf()
    }

    pub fn configuration(&self) -> &str {
        &self.item.config
    }

    pub fn compiler_input(&self) -> Result<ProgramOptions> {
        let source = compiler_runner::read_test_file(&self.item.path);
        let payload = test_case_parser::make_units_from_test(&source, &self.item.path);
        let external = payload.global_options.get("runexternalcode");
        if external.is_some_and(|value| value == "true") {
            return Err("external content mappers are unsupported".into());
        }
        let tsconfig = parse_test_ts_config(&payload);
        let mut settings = self.settings.clone();
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
        let library = workspace_root().join("fixtures/typescript/tsc/testdata/tests/lib");
        for path in files(&library)? {
            let name = format!("/.lib/{}", path.strip_prefix(&library)?.display());
            content.insert(name, vfstest::MapFile::from(fs::read(path)?));
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
}

// Use the pinned compiler's syntax checks rather than guessing diagnostic-code ranges.
pub fn syntax_codes() -> Result<BTreeSet<i32>> {
    let root = workspace_root().join("fixtures/typescript/tsc/internal");
    let reference = Regex::new(r"diagnostics\.([A-Za-z0-9_]+)")?;
    let mut names = BTreeSet::new();
    let mut paths = vec![root.join("checker/grammarchecks.go")];
    for directory in ["parser", "scanner", "binder"] {
        for path in files(&root.join(directory))? {
            if path.extension().is_some_and(|extension| extension == "go")
                && !path.to_string_lossy().ends_with("_test.go")
            {
                paths.push(path);
            }
        }
    }
    for path in paths {
        let source = fs::read_to_string(path)?;
        for capture in reference.captures_iter(&source) {
            names.insert(capture[1].to_owned());
        }
    }
    let definitions = fs::read_to_string(root.join("diagnostics/diagnostics_generated.go"))?;
    let definition = Regex::new(r"var (\w+) = &Message\{code: (\d+),")?;
    let mut codes = BTreeSet::new();
    for capture in definition.captures_iter(&definitions) {
        if names.contains(&capture[1]) {
            codes.insert(capture[2].parse()?);
        }
    }
    Ok(codes)
}
