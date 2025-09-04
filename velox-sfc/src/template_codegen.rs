use crate::template_ast::{AttrKind, Node, TemplateAttr};

/// Public API: compile `<template>` string to a Rust module body with `render()`.
pub fn compile_template_to_rs(template_src: &str, _component_name: &str) -> Result<String, String> {
    let nodes = crate::template_parse::parse_template_to_ast(template_src)?;
    if nodes.is_empty() {
        return Ok(format!(
            r#"pub fn render() -> velox_dom::VNode {{
    use velox_dom::*;
    text("")
}}"#
        ));
    }

    // For MVP, assume a single root node.
    let root = &nodes[0];
    let body_with = emit_node_with(root);

    Ok(format!(
        r#"pub fn render() -> velox_dom::VNode {{
    render_with(|_| String::new())
}}

pub fn render_with<F>(mut resolve: F) -> velox_dom::VNode where F: FnMut(&str) -> String {{
    use velox_dom::*;
    {body_with}
}}"#,
        body_with = body_with
    ))
}

fn emit_node(n: &Node) -> String {
    match n {
        Node::Text(t) => format!(r#"text({})"#, string_lit(t)),
        Node::Interpolation(expr) => {
            format!(r#"text(&format!("{{}}", {}))"#, expr.trim())
        }
        Node::Element {
            tag,
            attrs,
            children,
            ..
        } => {
            let props = emit_props(attrs);
            let kids = emit_children(children);
            format!(r#"h("{}", {props}, {kids})"#, tag)
        }
    }
}

fn emit_props(attrs: &[TemplateAttr]) -> String {
    if attrs.is_empty() {
        return "Props::new()".to_string();
    }
    let mut parts = vec!["Props::new()".to_string()];
    for a in attrs {
        match a.kind {
            AttrKind::Static => {
                let v = a.value.clone().unwrap_or_default();
                parts.push(format!(r#".set("{}", {})"#, a.name, string_lit(&v)));
            }
            AttrKind::Bind => {
                let expr = a.value.clone().unwrap_or_else(|| a.name.clone());
                parts.push(format!(
                    r#".set("{}", &format!("{{}}", {}))"#,
                    a.name,
                    expr.trim()
                ));
            }
            AttrKind::On => {
                // Store as a string for now; renderer will wire this later
                let handler = a.value.clone().unwrap_or_default();
                parts.push(format!(
                    r#".set("on:{}", {})"#,
                    a.name,
                    string_lit(&handler)
                ));
            }
        }
    }
    parts.join("")
}

fn emit_children(children: &[Node]) -> String {
    if children.is_empty() {
        return "vec![]".to_string();
    }
    let items: Vec<String> = children.iter().map(emit_node).collect();
    format!("vec![{}]", items.join(", "))
}

fn emit_node_with(n: &Node) -> String {
    match n {
        Node::Text(t) => format!(r#"text({})"#, string_lit(t)),
        Node::Interpolation(expr) => {
            let key = string_lit(expr.trim());
            format!(r#"text(&resolve({}))"#, key)
        }
        Node::Element { tag, attrs, children, .. } => {
            let props = emit_props_with(attrs);
            let kids = emit_children_with(children);
            format!(r#"h("{}", {props}, {kids})"#, tag)
        }
    }
}

fn emit_props_with(attrs: &[TemplateAttr]) -> String {
    if attrs.is_empty() { return "Props::new()".to_string(); }
    let mut parts = vec!["Props::new()".to_string()];
    for a in attrs {
        match a.kind {
            AttrKind::Static => {
                let v = a.value.clone().unwrap_or_default();
                parts.push(format!(r#".set("{}", {})"#, a.name, string_lit(&v)));
            }
            AttrKind::Bind => {
                let expr = a.value.clone().unwrap_or_else(|| a.name.clone());
                let key = string_lit(expr.trim());
                parts.push(format!(r#".set("{}", &resolve({}))"#, a.name, key));
            }
            AttrKind::On => {
                let handler = a.value.clone().unwrap_or_default();
                parts.push(format!(r#".set("on:{}", {})"#, a.name, string_lit(&handler)));
            }
        }
    }
    parts.join("")
}

fn emit_children_with(children: &[Node]) -> String {
    if children.is_empty() { return "vec![]".to_string(); }
    let items: Vec<String> = children.iter().map(emit_node_with).collect();
    format!("vec![{}]", items.join(", "))
}

fn string_lit(s: &str) -> String {
    // Basic escape for quotes and backslashes; good enough for tests
    let mut out = String::with_capacity(s.len() + 8);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}
