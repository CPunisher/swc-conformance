use std::{collections::BTreeSet, fs};

use regex::Regex;
use serde_json::{Value, json};
use tsrs_testrunner::{
    compiler_runner, harnessutil::TestConfiguration, options::tsoptions_option_table,
    test_case_parser,
};

use crate::{Output, Result, files, workspace_root};

pub fn read() -> Result<Output> {
    let root = workspace_root().join("fixtures/typescript/tsc/testdata");
    let tests = root.join("tests/cases");
    let table = tsoptions_option_table();
    let diagnostic = Regex::new(r"^!!! error TS(\d+):")?;
    let mut output = Vec::new();
    for suite in ["compiler", "conformance"] {
        let input = tests.join(suite);
        let references = root.join("baselines/reference").join(suite);
        for path in files(&input)? {
            if path
                .extension()
                .is_none_or(|extension| extension != "ts" && extension != "tsx")
            {
                continue;
            }
            let source = compiler_runner::read_test_file(path.to_str().unwrap());
            let payload = test_case_parser::make_units_from_test(&source, path.to_str().unwrap());
            let normalized = source.replace("\r\n", "\n");
            let lines: Vec<_> = normalized.split('\n').collect();
            let mut variants = Vec::new();
            let items = compiler_runner::expand_test_file(suite, path.to_str().unwrap(), &table)?;
            for item in items {
                let configuration =
                    compiler_runner::find_configuration(&source, &table, &item.config)?;
                let options = match configuration {
                    Some(configuration) => configuration.config,
                    None => TestConfiguration::default(),
                };
                let baseline = references.join(format!("{}.errors.txt", item.name));
                let diagnostics = if baseline.exists() {
                    read_diagnostics(&fs::read_to_string(baseline)?, &diagnostic)?
                } else {
                    Vec::new()
                };
                variants.push(json!({
                    "name": item.name,
                    "config": item.config,
                    "options": options,
                    "diagnostics": diagnostics,
                }));
            }
            let mut units = Vec::new();
            for unit in &payload.test_unit_data {
                units.push(file_json(unit, &lines)?);
            }
            let tsconfig = payload
                .ts_config_file_unit_data
                .as_ref()
                .map(|unit| file_json(unit, &lines))
                .transpose()?;
            let relative = path.strip_prefix(&tests)?;
            let value = json!({
                "path": relative,
                "files": units,
                "tsconfig": tsconfig,
                "currentDirectory": payload.current_directory,
                "options": payload.global_options,
                "symlinks": payload.symlinks,
                "variants": variants,
            });
            output.push((relative.with_added_extension("json"), value));
        }
    }
    let library = root.join("tests/lib");
    let mut libraries = Vec::new();
    for path in files(&library)? {
        libraries.push(path.strip_prefix(&library)?.to_path_buf());
    }
    output.push(("libraries.json".into(), json!(libraries)));
    output.push(("syntax-codes.json".into(), json!(syntax_codes()?)));
    Ok(output)
}

fn file_json(unit: &test_case_parser::TestUnit, source: &[&str]) -> Result<Value> {
    let mut cursor = 0;
    let mut lines = Vec::new();
    if !unit.content.is_empty() {
        for line in unit.content.split('\n') {
            let position = source[cursor..]
                .iter()
                .position(|candidate| *candidate == line);
            let position =
                position.ok_or_else(|| format!("cannot locate source for {}", unit.name))?;
            cursor += position;
            lines.push(cursor);
            cursor += 1;
        }
    }
    Ok(json!({"name": unit.name, "lines": lines}))
}

fn read_diagnostics(source: &str, pattern: &Regex) -> Result<Vec<Value>> {
    let mut output = Vec::new();
    let mut file = None;
    let mut codes = Vec::new();
    for line in source.lines() {
        if let Some(section) = line.strip_prefix("==== ")
            && let Some(section) = section.strip_suffix(" ====")
            && let Some((name, _)) = section.rsplit_once(" (")
        {
            if file.is_some() || !codes.is_empty() {
                output.push(json!({"file": file, "codes": codes}));
            }
            file = Some(name.to_owned());
            codes = Vec::new();
        }
        if let Some(capture) = pattern.captures(line) {
            codes.push(capture[1].parse::<i32>()?);
        }
    }
    if file.is_some() || !codes.is_empty() {
        output.push(json!({"file": file, "codes": codes}));
    }
    Ok(output)
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
