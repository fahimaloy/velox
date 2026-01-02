use crate::template_ast::{AttrKind, Node, TemplateAttr};
use std::collections::HashSet;

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
    let body_with_state = emit_node_with_state(root);

    let mut out = format!(
        r#"pub fn render() -> velox_dom::VNode {{
    render_with(|_| String::new())
}}

pub fn render_with<F>(mut resolve: F) -> velox_dom::VNode where F: FnMut(&str) -> String {{
    use velox_dom::*;
    {body_with}
}}"#,
        body_with = body_with
    );

    // Also emit render_with_state that accepts a `state: Arc<script_rs::State>`
    out.push_str("\n\n");
    out.push_str(&format!(
        r#"pub fn render_with_state<F>(state: std::sync::Arc<script_rs::State>, mut resolve: F) -> velox_dom::VNode where F: FnMut(&str) -> String {{
    use velox_dom::*;
    {body_with_state}
}}"#,
        body_with_state = body_with_state
    ));

    // Collect event handler names from the template and generate a helper
    let handlers = collect_handlers(&nodes);
    if !handlers.is_empty() {
        out.push_str("\n\n");
        out.push_str(&generate_make_on_event(&handlers));
    }

    Ok(out)
}

fn collect_handlers(nodes: &[Node]) -> Vec<String> {
    let mut set: HashSet<String> = HashSet::new();
    fn walk(n: &Node, set: &mut HashSet<String>) {
        match n {
            Node::Element { attrs, children, .. } => {
                for a in attrs {
                    if let AttrKind::On = a.kind {
                        if let Some(v) = &a.value {
                            set.insert(v.clone());
                        }
                    }
                }
                for c in children {
                    walk(c, set);
                }
            }
            _ => {}
        }
    }
    for n in nodes { walk(n, &mut set); }
    let mut v: Vec<String> = set.into_iter().collect();
    v.sort();
    v
}

fn generate_make_on_event(handlers: &[String]) -> String {
    // Generate a simple dispatch helper that calls methods on `app::script_rs::State`.
    // This assumes methods are zero-arg; handling payloads or arity will be added later.
    let mut arms = String::new();
    for h in handlers {
        arms.push_str(&format!("        \"{name}\" => {{ state.{name}(); }},\n", name = h));
    }

    format!(
        r#"pub fn make_on_event(state: std::sync::Arc<script_rs::State>) -> impl FnMut(&str, Option<&str>) + 'static {{
    move |name: &str, _payload: Option<&str>| {{
        match name {{
{arms}            _ => {{}}
        }}
    }}
}}"#,
        arms = arms
    )
}

pub(crate) fn emit_node(n: &Node) -> String {
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

pub(crate) fn emit_props(attrs: &[TemplateAttr]) -> String {
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

pub(crate) fn emit_children(children: &[Node]) -> String {
    if children.is_empty() {
        return "vec![]".to_string();
    }
    let items: Vec<String> = children.iter().map(emit_node).collect();
    format!("vec![{}]", items.join(", "))
}

fn rewrite_if_expr(expr: &str) -> String {
    let has_cmp = expr.contains("==")
        || expr.contains("!=")
        || expr.contains(">=")
        || expr.contains("<=")
        || expr.contains('>')
        || expr.contains('<');
    let mut out = String::new();
    let mut ident = String::new();
    let mut chars = expr.chars().peekable();

    fn flush_ident(out: &mut String, ident: &mut String, has_cmp: bool) {
        if ident.is_empty() {
            return;
        }
        let token = ident.as_str();
        let keep = token == "true"
            || token == "false"
            || token == "resolve"
            || token == "state"
            || token.contains('.');
        if keep {
            out.push_str(token);
        } else if has_cmp {
            out.push_str(&format!(
                "resolve({}).parse::<f64>().unwrap_or(0.0)",
                string_lit(token)
            ));
        } else {
            out.push_str(&format!("!resolve({}).is_empty()", string_lit(token)));
        }
        ident.clear();
    }

    while let Some(ch) = chars.next() {
        if ch.is_ascii_alphabetic() || ch == '_' {
            ident.push(ch);
            while let Some(&next) = chars.peek() {
                if next.is_ascii_alphanumeric() || next == '_' {
                    ident.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
            flush_ident(&mut out, &mut ident, has_cmp);
        } else if has_cmp && ch.is_ascii_digit() {
            let mut num = String::new();
            num.push(ch);
            while let Some(&next) = chars.peek() {
                if next.is_ascii_digit() || next == '.' {
                    num.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
            if !num.contains('.') {
                num.push_str(".0");
            }
            out.push_str(&num);
        } else {
            out.push(ch);
        }
    }
    flush_ident(&mut out, &mut ident, has_cmp);
    out
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
                let expr = rewrite_if_expr(&dir.value.unwrap_or_default());
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
    // Emit a block that constructs and returns a Vec<velox_dom::VNode> so we can
    // support constructs like v-for that push multiple nodes at runtime.
    if children.is_empty() { return "vec![]".to_string(); }
    let mut out = String::new();
    out.push_str("{ let mut __children: Vec<velox_dom::VNode> = Vec::new();\n");
    let mut i = 0usize;
    while i < children.len() {
        match &children[i] {
            Node::Element { tag, attrs, children: ch, self_closing } => {
                // v-if chain handling (unchanged)
                if let Some(pos) = attrs.iter().position(|a| matches!(a.kind, AttrKind::Directive) && a.name == "if") {
                    let mut attrs_if = attrs.clone();
                    let dir = attrs_if.remove(pos);
                    let expr_if = rewrite_if_expr(&dir.value.unwrap_or_default());
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
                                let expr_ei = rewrite_if_expr(&dir_ei.value.unwrap_or_default());
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

                    // build the conditional expression string and push into __children
                    let mut cond = String::new();
                    cond.push_str(&format!(r#"(if ({}) {{ {} }}"#, expr_if.trim(), inner_if));
                    for part in chain_parts.iter() { cond.push(' '); cond.push_str(part); }
                    if let Some(e) = else_part { cond.push(' '); cond.push_str(&e); } else { cond.push_str(r#" else { text("") }"#); }
                    cond.push(')');
                    out.push_str(&format!("__children.push({});\n", cond));
                    i = if j > i { j } else { i + 1 };
                    continue;
                }

                // v-for handling: syntax `item in expr`
                if let Some(posf) = attrs.iter().position(|a| matches!(a.kind, AttrKind::Directive) && a.name == "for") {
                    let mut attrs_f = attrs.clone();
                    let dir = attrs_f.remove(posf);
                    let val = dir.value.unwrap_or_default();
                    // parse `item in expr` pattern
                    if let Some(idx) = val.find(" in ") {
                        let var = val[..idx].trim();
                        let expr = val[idx + 4..].trim();
                        // generate loop: parse count from resolve(expr)
                        let tmp_elem = Node::Element { tag: tag.clone(), attrs: attrs_f, children: ch.clone(), self_closing: *self_closing };
                        // emit loop that pushes nodes; use __i as index variable
                        out.push_str(&format!("let __for_count = {{ let s = resolve(\"{}\"); s.parse::<usize>().unwrap_or(0) }};\n", expr));
                        out.push_str("for __i in 0..__for_count {\n");
                        // when emitting the element inside loop, substitute interp of var -> __i
                        let inner = emit_node_with_ctx(&tmp_elem, Some(var));
                        out.push_str(&format!("    __children.push({});\n", inner));
                        out.push_str("}\n");
                        i += 1;
                        continue;
                    }
                }

                // not an if-directive or for-directive element
                let expr = emit_node_with(&children[i]);
                out.push_str(&format!("__children.push({});\n", expr));
                i += 1;
            }
            _ => {
                let expr = emit_node_with(&children[i]);
                out.push_str(&format!("__children.push({});\n", expr));
                i += 1;
            }
        }
    }
    out.push_str("__children\n}");
    out
}

// Variant of children emitter that generates code targeting a `state` variable
fn emit_children_with_state(children: &[Node]) -> String {
    if children.is_empty() { return "vec![]".to_string(); }
    let mut out = String::new();
    out.push_str("{ let mut __children: Vec<velox_dom::VNode> = Vec::new();\n");
    let mut i = 0usize;
    while i < children.len() {
        match &children[i] {
            Node::Element { tag, attrs, children: ch, self_closing } => {
                // v-if handling (same as before)
                if let Some(pos) = attrs.iter().position(|a| matches!(a.kind, AttrKind::Directive) && a.name == "if") {
                    let mut attrs_if = attrs.clone();
                    let dir = attrs_if.remove(pos);
                    let expr_if = rewrite_if_expr(&dir.value.unwrap_or_default());
                    let tmp_if = Node::Element { tag: tag.clone(), attrs: attrs_if, children: ch.clone(), self_closing: *self_closing };
                    let inner_if = emit_node_with_state(&tmp_if);
                    // collect else-if/else chain
                    let mut chain_parts: Vec<String> = Vec::new();
                    let mut j = i + 1;
                    let mut else_part: Option<String> = None;
                    while j < children.len() {
                        if let Node::Element { tag: tag2, attrs: attrs2, children: ch2, self_closing: sc2 } = &children[j] {
                            if let Some(pos2) = attrs2.iter().position(|a| matches!(a.kind, AttrKind::Directive) && (a.name == "else-if" || a.name == "elseif")) {
                                let mut attrs_ei = attrs2.clone();
                                let dir_ei = attrs_ei.remove(pos2);
                                let expr_ei = rewrite_if_expr(&dir_ei.value.unwrap_or_default());
                                let tmp_ei = Node::Element { tag: tag2.clone(), attrs: attrs_ei, children: ch2.clone(), self_closing: *sc2 };
                                let inner_ei = emit_node_with_state(&tmp_ei);
                                chain_parts.push(format!(r#"else if ({}) {{ {} }}"#, expr_ei.trim(), inner_ei));
                                j += 1;
                                continue;
                            }
                            if let Some(pos3) = attrs2.iter().position(|a| matches!(a.kind, AttrKind::Directive) && a.name == "else") {
                                let mut attrs_e = attrs2.clone();
                                attrs_e.remove(pos3);
                                let tmp_e = Node::Element { tag: tag2.clone(), attrs: attrs_e, children: ch2.clone(), self_closing: *sc2 };
                                let inner_e = emit_node_with_state(&tmp_e);
                                else_part = Some(format!(r#"else {{ {} }}"#, inner_e));
                                j += 1;
                                break;
                            }
                        }
                        break;
                    }
                    let mut cond = String::new();
                    cond.push_str(&format!(r#"(if ({}) {{ {} }}"#, expr_if.trim(), inner_if));
                    for part in chain_parts.iter() { cond.push(' '); cond.push_str(part); }
                    if let Some(e) = else_part { cond.push(' '); cond.push_str(&e); } else { cond.push_str(r#" else { text("") }"#); }
                    cond.push(')');
                    out.push_str(&format!("__children.push({});\n", cond));
                    i = if j > i { j } else { i + 1 };
                    continue;
                }

                // v-for handling for state collections: `item in items` or `(item, idx) in items`
                if let Some(posf) = attrs.iter().position(|a| matches!(a.kind, AttrKind::Directive) && a.name == "for") {
                    let mut attrs_f = attrs.clone();
                    let dir = attrs_f.remove(posf);
                    let val = dir.value.unwrap_or_default();
                    if let Some(idx) = val.find(" in ") {
                        let left = val[..idx].trim();
                        let expr = val[idx + 4..].trim();
                        // parse destructuring
                        let mut item_name = "__item".to_string();
                        let mut idx_name = "__idx".to_string();
                        if left.starts_with('(') && left.ends_with(')') {
                            let inner = &left[1..left.len()-1];
                            let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
                            if parts.len() >= 1 && !parts[0].is_empty() { item_name = parts[0].to_string(); }
                            if parts.len() >= 2 && !parts[1].is_empty() { idx_name = parts[1].to_string(); }
                        } else {
                            item_name = left.to_string();
                        }
                        let tmp_elem = Node::Element { tag: tag.clone(), attrs: attrs_f, children: ch.clone(), self_closing: *self_closing };
                        // iterate over state.<expr>
                        out.push_str(&format!("if let Some(__col) = std::option::Option::Some(&state.{}) {{\n", expr));
                        out.push_str(&format!("    for ({idx_var}, {item_var}) in __col.iter().enumerate() {{\n", idx_var = "__idx", item_var = "__item"));
                        // emit inner with ctx mapping
                        let inner = emit_node_with_ctx_state(&tmp_elem, Some(&item_name), Some(&idx_name));
                        out.push_str(&format!("        __children.push({});\n", inner));
                        out.push_str("    }\n}\n");
                        i += 1;
                        continue;
                    }
                }

                // default
                let expr = emit_node_with_state(&children[i]);
                out.push_str(&format!("__children.push({});\n", expr));
                i += 1;
            }
            _ => {
                let expr = emit_node_with_state(&children[i]);
                out.push_str(&format!("__children.push({});\n", expr));
                i += 1;
            }
        }
    }
    out.push_str("__children\n}");
    out
}

fn emit_node_with_state(n: &Node) -> String {
    match n {
        Node::Text(t) => format!(r#"text({})"#, string_lit(t)),
        Node::Interpolation(expr) => {
            let key = string_lit(expr.trim());
            format!(r#"text(&resolve({}))"#, key)
        }
        Node::Element { tag, attrs, children, .. } => {
            let props = emit_props_with(attrs);
            let kids = emit_children_with_state(children);
            format!(r#"h("{}", {props}, {kids})"#, tag)
        }
    }
}

fn emit_node_with_ctx_state(n: &Node, item_name: Option<&str>, idx_name: Option<&str>) -> String {
    match n {
        Node::Text(t) => format!(r#"text({})"#, string_lit(t)),
        Node::Interpolation(expr) => {
            let key = expr.trim();
            if let Some(item) = item_name {
                if key == item { return "text(&format!(\"{}\", __item))".to_string(); }
            }
            if let Some(idx) = idx_name {
                if key == idx { return "text(&__idx.to_string())".to_string(); }
            }
            let key_lit = string_lit(key);
            format!(r#"text(&resolve({}))"#, key_lit)
        }
        Node::Element { tag, attrs, children, .. } => {
            let props = emit_props_with(attrs);
            let mut k_items: Vec<String> = Vec::new();
            for c in children {
                k_items.push(emit_node_with_ctx_state(c, item_name, idx_name));
            }
            let kids = format!("vec![{}]", k_items.join(", "));
            format!(r#"h("{}", {props}, {kids})"#, tag)
        }
    }
}

fn emit_node_with_ctx(n: &Node, loop_var: Option<&str>) -> String {
    match n {
        Node::Text(t) => format!(r#"text({})"#, string_lit(t)),
        Node::Interpolation(expr) => {
            let key = expr.trim();
            if let Some(var) = loop_var {
                if key == var {
                    return "text(&__i.to_string())".to_string();
                }
            }
            let key_lit = string_lit(key);
            format!(r#"text(&resolve({}))"#, key_lit)
        }
        Node::Element { tag, attrs, children, .. } => {
            let props = emit_props_with(attrs);
            let kids = {
                let mut k_items: Vec<String> = Vec::new();
                for c in children {
                    k_items.push(emit_node_with_ctx(c, loop_var));
                }
                format!("vec![{}]", k_items.join(", "))
            };
            format!(r#"h("{}", {props}, {kids})"#, tag)
        }
    }
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
