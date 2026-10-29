use std::collections::BTreeSet;

use serde_json::{Value, json};
use tsrs_compiler::{self as compiler, Context};
use tsrs_core::{P, Tristate, arena::Region};

use crate::{
    Result,
    case::typescript::{self, TypeScriptCase},
    runner::Output,
};

const REVISION: &str = "677622d08ef3c12f370b9342623a41d156687c0c";

pub fn run() -> Result<Output> {
    let cases = TypeScriptCase::read()?;
    let runner = Tsrs {
        codes: typescript::syntax_codes()?,
    };
    let mut output = Vec::new();
    for case in cases {
        let value = runner
            .parse(&case)
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
    fn parse(&self, case: &TypeScriptCase) -> Result<Value> {
        let region = Region::new(1024 * 1024);
        let _scope = region.enter();
        let mut options = case.compiler_input()?;
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
