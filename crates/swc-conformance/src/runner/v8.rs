use serde_json::{Value, json};

use crate::{
    Result,
    case::{self, Case},
    runner::Output,
};

pub fn run() -> Result<Output> {
    let cases = case::read("v8")?;
    let mut runner = V8::new();
    let mut output = Vec::new();
    for case in cases {
        let value = runner.parse(&case);
        output.push((case.snapshot, value));
    }
    let version = json!({ "v8": v8::V8::get_version(), "offsetEncoding": "utf8" });
    output.push(("version.json".into(), version));
    Ok(output)
}

struct V8 {
    isolate: v8::OwnedIsolate,
}

impl V8 {
    fn new() -> Self {
        let platform = v8::new_default_platform(0, false).make_shared();
        v8::V8::initialize_platform(platform);
        v8::V8::initialize();
        Self {
            isolate: v8::Isolate::new(Default::default()),
        }
    }

    fn parse(&mut self, case: &Case) -> Value {
        v8::scope!(let scope, &mut self.isolate);
        let context = v8::Context::new(scope, Default::default());
        let scope = &mut v8::ContextScope::new(scope, context);
        v8::tc_scope!(let scope, scope);
        let code = v8::String::new(scope, &case.source).unwrap();
        if case.module {
            let name = v8::String::new(scope, "case.mjs").unwrap();
            let origin = v8::ScriptOrigin::new(
                scope,
                name.into(),
                0,
                0,
                false,
                0,
                None,
                false,
                false,
                true,
                None,
            );
            let mut source = v8::script_compiler::Source::new(code, Some(&origin));
            v8::script_compiler::compile_module(scope, &mut source);
        } else {
            v8::Script::compile(scope, code, None);
        }
        let mut diagnostics = Vec::new();
        if let Some(message) = scope.message() {
            diagnostics.push(json!({
                "start": byte_offset(&case.source, message.get_start_position()),
                "end": byte_offset(&case.source, message.get_end_position()),
            }));
        }
        json!(diagnostics)
    }
}

fn byte_offset(source: &str, position: i32) -> Option<u32> {
    let position = usize::try_from(position).ok()?;
    let mut offset = 0;
    for (byte, character) in source.char_indices() {
        if offset == position {
            return byte.try_into().ok();
        }
        offset += character.len_utf16();
    }
    (offset == position).then(|| source.len().try_into().unwrap())
}
