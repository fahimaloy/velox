pub mod codegen;
pub mod component_resolver;
pub mod diagnostic;
pub mod expr;
pub mod script_index;
pub mod sfc;

pub mod template_ast;
pub mod template_codegen;
pub mod template_parse;

#[cfg(test)]
mod codegen_unit_tests;

pub use component_resolver::{ComponentImport, ComponentResolver, transform_components};
pub use sfc::{Attr, ScriptBlock, Sfc, StyleBlock, TemplateBlock, parse_sfc, validate_sfc};

pub use template_ast::{AttrKind, Node, TemplateAttr};
pub use template_codegen::collect_vmodel_expressions;
pub use template_codegen::compile_template_to_rs;
pub use template_codegen::compile_template_to_rs_full;
pub use template_codegen::generate_vmodel_setters;
pub use template_parse::parse_template_to_ast;

pub use codegen::generate_scope_id;
pub use codegen::is_scoped;
pub use codegen::to_stub_rs;
pub use codegen::to_stub_rs_unwrapped;
pub use codegen::to_stub_rs_with_base;
