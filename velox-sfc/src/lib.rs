pub mod codegen;
pub mod sfc;

pub mod template_ast;
pub mod template_codegen;
pub mod template_parse;

pub use sfc::{Attr, ScriptBlock, Sfc, StyleBlock, TemplateBlock, parse_sfc};

pub use template_ast::{AttrKind, Node, TemplateAttr};
pub use template_codegen::compile_template_to_rs;
pub use template_parse::parse_template_to_ast;

// NEW: re-export so velox_sfc::to_stub_rs works in the CLI
pub use codegen::to_stub_rs;
