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
            let key = string_lit(expr.trim());
            format!(r#"text(&resolve({}))"#, key)
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
            AttrKind::Directive => {
                // directives are not emitted as props
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
            // handle directive `v-if` (simple implementation)
            if let Some(pos) = attrs.iter().position(|a| matches!(a.kind, AttrKind::Directive) && a.name == "if") {
                // clone attrs and remove the directive so it does not become a prop
                let mut attrs2 = attrs.clone();
                let dir = attrs2.remove(pos);
                let expr = dir.value.unwrap_or_default();
                // construct a temporary element node with remaining attrs
                let tmp = Node::Element { tag: tag.clone(), attrs: attrs2, children: children.clone(), self_closing: false };
                let inner = emit_node_with(&tmp);
                return format!(r#"if ({}) {{ {} }} else {{ text("") }}"#, expr.trim(), inner);
            }

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
            AttrKind::Directive => {
                // do not emit directives as props
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
    let mut items: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < children.len() {
        match &children[i] {
            Node::Element { tag, attrs, children: ch, self_closing } => {
                // if this element has a `v-if`, try to chain following `v-else-if` and `v-else` siblings
                if let Some(pos) = attrs.iter().position(|a| matches!(a.kind, AttrKind::Directive) && a.name == "if") {
                    // prepare first if
                    let mut attrs_if = attrs.clone();
                    let dir = attrs_if.remove(pos);
                    let expr_if = dir.value.unwrap_or_default();
                    let tmp_if = Node::Element { tag: tag.clone(), attrs: attrs_if, children: ch.clone(), self_closing: *self_closing };
                    let inner_if = emit_node_with(&tmp_if);

                    // collect else-if chain
                    let mut chain_parts: Vec<String> = Vec::new();
                    let mut j = i + 1;
                    let mut else_part: Option<String> = None;
                    while j < children.len() {
                        if let Node::Element { tag: tag2, attrs: attrs2, children: ch2, self_closing: sc2 } = &children[j] {
                            if let Some(pos2) = attrs2.iter().position(|a| matches!(a.kind, AttrKind::Directive) && (a.name == "else-if" || a.name == "elseif")) {
                                let mut attrs_ei = attrs2.clone();
                                let dir_ei = attrs_ei.remove(pos2);
                                let expr_ei = dir_ei.value.unwrap_or_default();
                                let tmp_ei = Node::Element { tag: tag2.clone(), attrs: attrs_ei, children: ch2.clone(), self_closing: *sc2 };
                                let inner_ei = emit_node_with(&tmp_ei);
                                chain_parts.push(format!(r#"else if ({}) {{ {} }}"#, expr_ei.trim(), inner_ei));
                                j += 1;
                                continue;
                            }
                            if let Some(pos3) = attrs2.iter().position(|a| matches!(a.kind, AttrKind::Directive) && a.name == "else") {
                                let mut attrs_e = attrs2.clone();
                                attrs_e.remove(pos3);
                                let tmp_e = Node::Element { tag: tag2.clone(), attrs: attrs_e, children: ch2.clone(), self_closing: *sc2 };
                                let inner_e = emit_node_with(&tmp_e);
                                else_part = Some(format!(r#"else {{ {} }}"#, inner_e));
                                j += 1;
                                break;
                            }
                        }
                        break;
                    }

                    // build the conditional expression string
                    let mut cond = String::new();
                    cond.push_str(&format!(r#"(if ({}) {{ {} }}"#, expr_if.trim(), inner_if));
                    for part in chain_parts.iter() { cond.push(' '); cond.push_str(part); }
                    if let Some(e) = else_part { cond.push(' '); cond.push_str(&e); } else { cond.push_str(r#" else { text("") }"#); }
                    cond.push(')');

                    items.push(cond);
                    // advance i past the chain
                    i = if j > i { j } else { i + 1 };
                    continue;
                }

                // not an if-directive element
                items.push(emit_node_with(&children[i]));
                i += 1;
            }
            _ => {
                items.push(emit_node_with(&children[i]));
                i += 1;
            }
        }
    }

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
