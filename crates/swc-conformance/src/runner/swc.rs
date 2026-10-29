use std::collections::{BTreeMap, HashMap};

use serde_json::Value;
use swc_experimental_allocator::Allocator;
use swc_experimental_ecma_ast::{
    EsVersion, ExportNamedSpecifier, Ident, JSXElementName, JSXObject, ModuleExportName,
    NamedExport, Program, ScopeId, Visit, VisitWith,
};
use swc_experimental_ecma_parser::{EsSyntax, Syntax, with_file_parser};
use swc_experimental_ecma_semantic::resolver::{Semantic, resolver};

use crate::{
    Result,
    case::{Case, swc_minifier, test262},
    runner::Output,
};

pub fn run() -> Result<Output> {
    let mut cases = test262::resolver_cases()?;
    cases.extend(swc_minifier::read()?);
    let mut output = Vec::new();
    for case in cases {
        if let Some(value) = resolve(&case) {
            output.push((case.path.with_added_extension("json"), value));
        }
    }
    Ok(output)
}

fn resolve(case: &Case) -> Option<Value> {
    let allocator = Allocator::new();
    let source_text = &case.source;

    let program = with_file_parser(
        &allocator,
        source_text,
        Syntax::Es(EsSyntax {
            jsx: case.jsx,
            decorators: true,
            decorators_before_export: true,
            auto_accessors: true,
            explicit_resource_management: true,
            ..Default::default()
        }),
        EsVersion::EsNext,
        None,
        |parser| {
            let program = if case.module {
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
    let mut visitor = Bindings {
        semantic: &semantic,
        identifiers: BTreeMap::new(),
    };
    visitor.visit_program(&program);
    // Canonical binding IDs follow identifier end offsets, independent of traversal order.
    let mut bindings = HashMap::new();
    let mut output = BTreeMap::new();
    for (end, binding) in visitor.identifiers {
        let target = binding.map_or(0, |binding| {
            let next = bindings.len() + 1;
            *bindings.entry(binding).or_insert(next)
        });
        output.insert(end, target);
    }
    Some(serde_json::json!(output))
}

struct Bindings<'a> {
    semantic: &'a Semantic,
    identifiers: BTreeMap<u32, Option<(String, ScopeId)>>,
}

impl<'a> Visit<'a> for Bindings<'_> {
    fn visit_ident(&mut self, node: &Ident) {
        let unresolved = self.semantic.unresolved_scope_id();
        let scope = if node.symbol_id.get().is_some() {
            self.semantic.node_scope(node)
        } else {
            unresolved
        };
        // Labels are control-flow names rather than lexical bindings.
        if scope.raw() == u32::MAX {
            return;
        }
        let binding =
            (scope != self.semantic.unresolved_scope_id()).then(|| (node.sym.to_string(), scope));
        // The reference parser reserves position zero for synthetic nodes.
        if !node.span.is_dummy() {
            self.identifiers.insert(node.span.end - 1, binding);
        }
    }

    // Imported and re-exported names, and exported aliases, are external names.
    fn visit_module_export_name(&mut self, _: &ModuleExportName) {}

    fn visit_named_export(&mut self, node: &NamedExport<'a>) {
        if node.src.is_none() {
            node.specifiers.visit_with(self);
        }
    }

    fn visit_export_named_specifier(&mut self, node: &ExportNamedSpecifier<'a>) {
        if let ModuleExportName::Ident(local) = &node.orig {
            self.visit_ident(local);
        }
    }

    fn visit_jsx_element_name(&mut self, node: &JSXElementName<'a>) {
        match node {
            JSXElementName::Ident(identifier) => {
                let name = identifier.sym.as_bytes();
                if name.first().is_some_and(u8::is_ascii_lowercase) || name.contains(&b'-') {
                    return;
                }
                self.visit_ident(identifier);
            }
            JSXElementName::JSXMemberExpr(member) => member.obj.visit_with(self),
            JSXElementName::JSXNamespacedName(_) => {}
        }
    }

    fn visit_jsx_object(&mut self, node: &JSXObject<'a>) {
        match node {
            JSXObject::Ident(identifier) if identifier.sym != "this" => {
                self.visit_ident(identifier)
            }
            JSXObject::JSXMemberExpr(member) => member.obj.visit_with(self),
            JSXObject::Ident(_) => {}
        }
    }
}
