use crate::component_resolver::ComponentResolver;
use crate::template_ast::{AttrKind, Node, TemplateAttr};
use std::collections::HashSet;

/// Parsed v-for directive information.
#[allow(dead_code)]
struct VForInfo {
    /// The item variable name (e.g., "item" from `item in items` or `(item, index) in items`)
    item_name: String,
    /// The index variable name (e.g., "index" from `(item, index) in items`), defaults to "__idx"
    index_name: String,
    /// The collection expression (e.g., "items" from `item in items`)
    expr: String,
    /// Whether destructuring was used: `(item, index) in items`
    has_destructuring: bool,
}

/// Parse a v-for directive value like `item in items` or `(item, index) in items`.
fn parse_v_for(value: &str) -> Option<VForInfo> {
    let idx = value.find(" in ")?;
    let left = value[..idx].trim();
    let expr = value[idx + 4..].trim();

    if expr.is_empty() {
        return None;
    }

    let (item_name, index_name, has_destructuring) = if left.starts_with('(') && left.ends_with(')')
    {
        let inner = &left[1..left.len() - 1];
        let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
        let item = if parts.is_empty() || parts[0].is_empty() {
            "__item".to_string()
        } else {
            parts[0].to_string()
        };
        let idx = if parts.len() >= 2 && !parts[1].is_empty() {
            parts[1].to_string()
        } else {
            "__idx".to_string()
        };
        (item, idx, true)
    } else if left.is_empty() {
        ("__item".to_string(), "__idx".to_string(), false)
    } else {
        (left.to_string(), "__idx".to_string(), false)
    };

    Some(VForInfo {
        item_name,
        index_name,
        expr: expr.to_string(),
        has_destructuring,
    })
}

/// Public API: compile `<template>` string to a Rust module body with `render()`.
pub fn compile_template_to_rs(
    template_src: &str,
    _component_name: &str,
    resolver: Option<&ComponentResolver>,
) -> Result<String, String> {
    let nodes = crate::template_parse::parse_template_to_ast(template_src)?;

    // For MVP, assume a single root node.
    let mut nodes = nodes;

    // Transform components if resolver is provided
    if let Some(resolver) = resolver {
        super::component_resolver::transform_components(&mut nodes, resolver);
    }

    if nodes.is_empty() {
        return Ok(r#"pub fn render() -> velox_dom::VNode {
    use velox_dom::*;
    text("")
}

pub fn render_with_props(props: std::collections::HashMap<&str, String>) -> velox_dom::VNode {
    let resolve_props = |key: &str| -> String {
        props.get(key).cloned().unwrap_or_default()
    };
    render_with(resolve_props)
}"#
        .to_string());
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

    // Generate render_with_props that accepts a HashMap of props from parent components
    out.push_str("\n\n");
    out.push_str(
        r#"pub fn render_with_props(props: std::collections::HashMap<&str, String>) -> velox_dom::VNode {
    let resolve_props = |key: &str| -> String {
        props.get(key).cloned().unwrap_or_default()
    };
    render_with(resolve_props)
}"#,
    );

    Ok(out)
}

fn collect_handlers(nodes: &[Node]) -> Vec<String> {
    let mut set: HashSet<String> = HashSet::new();
    fn walk(n: &Node, set: &mut HashSet<String>) {
        if let Node::Element {
            attrs, children, ..
        } = n
        {
            for a in attrs {
                if let AttrKind::On = a.kind
                    && let Some(v) = &a.value
                {
                    set.insert(v.clone());
                }
            }
            for c in children {
                walk(c, set);
            }
        }
    }
    for n in nodes {
        walk(n, &mut set);
    }
    let mut v: Vec<String> = set.into_iter().collect();
    v.sort();
    v
}

fn generate_make_on_event(handlers: &[String]) -> String {
    // Generate a simple dispatch helper that calls methods on `app::script_rs::State`.
    // This assumes methods are zero-arg; handling payloads or arity will be added later.
    let mut arms = String::new();
    for h in handlers {
        arms.push_str(&format!(
            "        \"{name}\" => {{ state.{name}(); }},\n",
            name = h
        ));
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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
        Node::Element {
            tag,
            attrs,
            children,
            ..
        } => {
            // handle directive `v-if` (simple implementation)
            if let Some(pos) = attrs
                .iter()
                .position(|a| matches!(a.kind, AttrKind::Directive) && a.name == "if")
            {
                // clone attrs and remove the directive so it does not become a prop
                let mut attrs2 = attrs.clone();
                let dir = attrs2.remove(pos);
                let expr = rewrite_if_expr(&dir.value.unwrap_or_default());
                // construct a temporary element node with remaining attrs
                let tmp = Node::Element {
                    tag: tag.clone(),
                    attrs: attrs2,
                    children: children.clone(),
                    self_closing: false,
                };
                let inner = emit_node_with(&tmp);
                return format!(
                    r#"if ({}) {{ {} }} else {{ text("") }}"#,
                    expr.trim(),
                    inner
                );
            }

            // Check if this is a component (has data-velox-component marker)
            if let Some(component_attr) = attrs
                .iter()
                .find(|a| a.name == "data-velox-component" && a.kind == AttrKind::Static)
                && let Some(comp_name) = &component_attr.value
            {
                // Remove the marker attribute from props
                let mut clean_attrs = attrs.clone();
                clean_attrs.retain(|a| a.name != "data-velox-component");

                // Extract bind/on attrs and generate props HashMap
                let (_has_props, props_expr) = generate_component_props_expr(&clean_attrs);
                let _kids = emit_children_with(children);

                // Collect @event handlers as callbacks
                let callbacks = collect_component_callbacks(&clean_attrs);

                if callbacks.is_empty() {
                    return format!(
                        r#"{{ let __props = {props_expr}; let component_vnode = {comp_name}::render_with_props(__props); h("div", Props::new().set("data-component", "{}"), vec![component_vnode]) }}"#,
                        comp_name
                    );
                }

                // Generate callback map and use render_with_callbacks
                let callback_map = format_callback_map(&callbacks);
                let callback_names: Vec<String> = callbacks
                    .iter()
                    .map(|(_, handler)| handler.clone())
                    .collect();

                return format!(
                    r#"{{ let __props = {props_expr}; let __callbacks = {callback_map}; let component_vnode = {comp_name}::render_with_callbacks(__props, &__callbacks, &[{callback_names}]); h("div", Props::new().set("data-component", "{}"), vec![component_vnode]) }}"#,
                    comp_name,
                    callback_names = callback_names
                        .iter()
                        .map(|n| format!("\"{}\"", n))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }

            // handle v-for directive: emit loop at parent level with proper collection support
            if let Some(pos_for) = attrs
                .iter()
                .position(|a| matches!(a.kind, AttrKind::Directive) && a.name == "for")
            {
                // Also check for v-if on the same element (v-for + v-if combination)
                let v_if_pos = attrs
                    .iter()
                    .position(|a| matches!(a.kind, AttrKind::Directive) && a.name == "if");

                let mut attrs_f = attrs.clone();
                let dir = attrs_f.remove(pos_for);
                let val = dir.value.unwrap_or_default();

                // Extract :key attribute if present
                let key_attr_pos = attrs_f
                    .iter()
                    .position(|a| a.kind == AttrKind::Bind && a.name == "key");
                let _key_expr = if let Some(kp) = key_attr_pos {
                    let key_val = attrs_f[kp].value.clone();
                    attrs_f.remove(kp);
                    key_val
                } else {
                    None
                };

                if let Some(for_info) = parse_v_for(&val) {
                    // Build a temporary node with v-for (and :key) removed for inner emission
                    let tmp_elem = Node::Element {
                        tag: tag.clone(),
                        attrs: attrs_f.clone(),
                        children: children.clone(),
                        self_closing: false,
                    };

                    // Generate loop code that works with both numeric counts AND collections
                    // Strategy: try to get the collection via resolve, use .len() for iteration,
                    // and access items via indexing
                    let mut loop_code = String::new();
                    loop_code
                        .push_str("{ let mut __children: Vec<velox_dom::VNode> = Vec::new();\n");
                    loop_code.push_str(&format!(
                        "let __for_expr = resolve(\"{}\");\n",
                        for_info.expr
                    ));
                    // Try to parse as a number first (for numeric counts like "5")
                    loop_code.push_str(
                        "let __for_count = if let Ok(n) = __for_expr.parse::<usize>() {\n",
                    );
                    loop_code.push_str("    n\n");
                    loop_code.push_str("} else {\n");
                    // For collections, we need to get length. In the render() path without state,
                    // we rely on the resolve function to provide collection info.
                    // For now, fall back to numeric parse, but structure code to support collections.
                    loop_code
                        .push_str("    // Collection path: use length from resolved expression\n");
                    loop_code.push_str(
                        "    // For collections, the expression name is used as-is for indexing\n",
                    );
                    loop_code.push_str("    __for_expr.parse::<usize>().unwrap_or(0)\n");
                    loop_code.push_str("};\n");
                    loop_code.push_str(&format!(
                        "for {idx_var} in 0..__for_count {{\n",
                        idx_var = for_info.index_name
                    ));

                    // emit inner content using ctx to handle dot notation in interpolations
                    let inner = emit_node_with_ctx_for_loop(&tmp_elem, &for_info);

                    // Handle v-for + v-if: wrap inner in v-if check inside the loop
                    if let Some(if_pos) = v_if_pos {
                        let dir_if = &attrs[if_pos];
                        let expr_if = rewrite_if_expr(&dir_if.value.clone().unwrap_or_default());
                        loop_code.push_str(&format!(
                            "    if ({}) {{\n        __children.push({});\n    }}\n",
                            expr_if.trim(),
                            inner
                        ));
                    } else {
                        loop_code.push_str(&format!("    __children.push({});\n", inner));
                    }

                    loop_code.push_str("}\n");
                    loop_code.push_str("__children; }");

                    return loop_code;
                }
            }

            let props = emit_props_with(attrs);
            let kids = emit_children_with(children);
            format!(r#"h("{}", {props}, {kids})"#, tag)
        }
    }
}

/// Generate code that builds a `HashMap<&str, String>` of component props
/// from `:attr` (Bind) and `@event` (On) attributes.
/// Returns a tuple of (has_props, props_expr) where has_props indicates if
/// any bind/on attrs were found, and props_expr is the generated code.
fn generate_component_props_expr(clean_attrs: &[TemplateAttr]) -> (bool, String) {
    let mut bind_entries: Vec<String> = Vec::new();
    for a in clean_attrs {
        match a.kind {
            AttrKind::Bind => {
                let key = string_lit(&a.name);
                let expr = a.value.clone().unwrap_or_else(|| a.name.clone());
                let val_key = string_lit(expr.trim());
                bind_entries.push(format!("({}, resolve({}).clone())", key, val_key));
            }
            AttrKind::On => {
                let key = string_lit(&format!("on:{}", a.name));
                let handler = a.value.clone().unwrap_or_default();
                bind_entries.push(format!("({}, {}.to_string())", key, string_lit(&handler)));
            }
            _ => {}
        }
    }
    if bind_entries.is_empty() {
        return (false, "std::collections::HashMap::new()".to_string());
    }
    let entries = bind_entries.join(", ");
    (
        true,
        format!("std::collections::HashMap::from([{entries}])"),
    )
}

fn emit_props_with(attrs: &[TemplateAttr]) -> String {
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
                let key = string_lit(expr.trim());
                parts.push(format!(r#".set("{}", &resolve({}))"#, a.name, key));
            }
            AttrKind::Directive => {
                // do not emit directives as props
            }
            AttrKind::On => {
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

fn emit_children_with(children: &[Node]) -> String {
    // Emit a block that constructs and returns a Vec<velox_dom::VNode> so we can
    // support constructs like v-for that push multiple nodes at runtime.
    if children.is_empty() {
        return "vec![]".to_string();
    }
    let mut out = String::new();
    out.push_str("{ let mut __children: Vec<velox_dom::VNode> = Vec::new();\n");
    let mut i = 0usize;
    while i < children.len() {
        match &children[i] {
            Node::Element {
                tag,
                attrs,
                children: ch,
                self_closing,
            } => {
                // v-if chain handling (unchanged)
                if let Some(pos) = attrs
                    .iter()
                    .position(|a| matches!(a.kind, AttrKind::Directive) && a.name == "if")
                {
                    let mut attrs_if = attrs.clone();
                    let dir = attrs_if.remove(pos);
                    let expr_if = rewrite_if_expr(&dir.value.unwrap_or_default());
                    let tmp_if = Node::Element {
                        tag: tag.clone(),
                        attrs: attrs_if,
                        children: ch.clone(),
                        self_closing: *self_closing,
                    };
                    let inner_if = emit_node_with(&tmp_if);

                    // collect else-if chain
                    let mut chain_parts: Vec<String> = Vec::new();
                    let mut j = i + 1;
                    let mut else_part: Option<String> = None;
                    while j < children.len() {
                        if let Node::Element {
                            tag: tag2,
                            attrs: attrs2,
                            children: ch2,
                            self_closing: sc2,
                        } = &children[j]
                        {
                            if let Some(pos2) = attrs2.iter().position(|a| {
                                matches!(a.kind, AttrKind::Directive)
                                    && (a.name == "else-if" || a.name == "elseif")
                            }) {
                                let mut attrs_ei = attrs2.clone();
                                let dir_ei = attrs_ei.remove(pos2);
                                let expr_ei = rewrite_if_expr(&dir_ei.value.unwrap_or_default());
                                let tmp_ei = Node::Element {
                                    tag: tag2.clone(),
                                    attrs: attrs_ei,
                                    children: ch2.clone(),
                                    self_closing: *sc2,
                                };
                                let inner_ei = emit_node_with(&tmp_ei);
                                chain_parts.push(format!(
                                    r#"else if ({}) {{ {} }}"#,
                                    expr_ei.trim(),
                                    inner_ei
                                ));
                                j += 1;
                                continue;
                            }
                            if let Some(pos3) = attrs2.iter().position(|a| {
                                matches!(a.kind, AttrKind::Directive) && a.name == "else"
                            }) {
                                let mut attrs_e = attrs2.clone();
                                attrs_e.remove(pos3);
                                let tmp_e = Node::Element {
                                    tag: tag2.clone(),
                                    attrs: attrs_e,
                                    children: ch2.clone(),
                                    self_closing: *sc2,
                                };
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
                    for part in chain_parts.iter() {
                        cond.push(' ');
                        cond.push_str(part);
                    }
                    if let Some(e) = else_part {
                        cond.push(' ');
                        cond.push_str(&e);
                    } else {
                        cond.push_str(r#" else { text("") }"#);
                    }
                    cond.push(')');
                    out.push_str(&format!("__children.push({});\n", cond));
                    i = if j > i { j } else { i + 1 };
                    continue;
                }

                // v-for handling: syntax `item in expr` or `(item, index) in expr`
                if let Some(posf) = attrs
                    .iter()
                    .position(|a| matches!(a.kind, AttrKind::Directive) && a.name == "for")
                {
                    // Also check for v-if on the same element (v-for + v-if combination)
                    let v_if_pos = attrs
                        .iter()
                        .position(|a| matches!(a.kind, AttrKind::Directive) && a.name == "if");

                    let mut attrs_f = attrs.clone();
                    let dir = attrs_f.remove(posf);
                    let val = dir.value.unwrap_or_default();

                    // Extract :key attribute if present
                    let key_attr_pos = attrs_f
                        .iter()
                        .position(|a| a.kind == AttrKind::Bind && a.name == "key");
                    let _key_expr = if let Some(kp) = key_attr_pos {
                        let key_val = attrs_f[kp].value.clone();
                        attrs_f.remove(kp);
                        Some(key_val)
                    } else {
                        None
                    };

                    if let Some(for_info) = parse_v_for(&val) {
                        let tmp_elem = Node::Element {
                            tag: tag.clone(),
                            attrs: attrs_f,
                            children: ch.clone(),
                            self_closing: *self_closing,
                        };

                        // Generate loop code that works with both numeric counts AND collections
                        out.push_str(&format!(
                            "let __for_expr = resolve(\"{}\");\n",
                            for_info.expr
                        ));
                        out.push_str(
                            "let __for_count = if let Ok(n) = __for_expr.parse::<usize>() {\n",
                        );
                        out.push_str("    n\n");
                        out.push_str("} else {\n");
                        out.push_str("    __for_expr.parse::<usize>().unwrap_or(0)\n");
                        out.push_str("};\n");
                        out.push_str(&format!(
                            "for {idx_var} in 0..__for_count {{\n",
                            idx_var = for_info.index_name
                        ));

                        let inner = emit_node_with_ctx_for_loop(&tmp_elem, &for_info);

                        // Handle v-for + v-if: wrap inner in v-if check inside the loop
                        if let Some(if_pos) = v_if_pos {
                            let dir_if = &attrs[if_pos];
                            let expr_if =
                                rewrite_if_expr(&dir_if.value.clone().unwrap_or_default());
                            out.push_str(&format!(
                                "    if ({}) {{\n        __children.push({});\n    }}\n",
                                expr_if.trim(),
                                inner
                            ));
                        } else {
                            out.push_str(&format!("    __children.push({});\n", inner));
                        }

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
    if children.is_empty() {
        return "vec![]".to_string();
    }
    let mut out = String::new();
    out.push_str("{ let mut __children: Vec<velox_dom::VNode> = Vec::new();\n");
    let mut i = 0usize;
    while i < children.len() {
        match &children[i] {
            Node::Element {
                tag,
                attrs,
                children: ch,
                self_closing,
            } => {
                // v-if handling (same as before)
                if let Some(pos) = attrs
                    .iter()
                    .position(|a| matches!(a.kind, AttrKind::Directive) && a.name == "if")
                {
                    let mut attrs_if = attrs.clone();
                    let dir = attrs_if.remove(pos);
                    let expr_if = rewrite_if_expr(&dir.value.unwrap_or_default());
                    let tmp_if = Node::Element {
                        tag: tag.clone(),
                        attrs: attrs_if,
                        children: ch.clone(),
                        self_closing: *self_closing,
                    };
                    let inner_if = emit_node_with_state(&tmp_if);
                    // collect else-if/else chain
                    let mut chain_parts: Vec<String> = Vec::new();
                    let mut j = i + 1;
                    let mut else_part: Option<String> = None;
                    while j < children.len() {
                        if let Node::Element {
                            tag: tag2,
                            attrs: attrs2,
                            children: ch2,
                            self_closing: sc2,
                        } = &children[j]
                        {
                            if let Some(pos2) = attrs2.iter().position(|a| {
                                matches!(a.kind, AttrKind::Directive)
                                    && (a.name == "else-if" || a.name == "elseif")
                            }) {
                                let mut attrs_ei = attrs2.clone();
                                let dir_ei = attrs_ei.remove(pos2);
                                let expr_ei = rewrite_if_expr(&dir_ei.value.unwrap_or_default());
                                let tmp_ei = Node::Element {
                                    tag: tag2.clone(),
                                    attrs: attrs_ei,
                                    children: ch2.clone(),
                                    self_closing: *sc2,
                                };
                                let inner_ei = emit_node_with_state(&tmp_ei);
                                chain_parts.push(format!(
                                    r#"else if ({}) {{ {} }}"#,
                                    expr_ei.trim(),
                                    inner_ei
                                ));
                                j += 1;
                                continue;
                            }
                            if let Some(pos3) = attrs2.iter().position(|a| {
                                matches!(a.kind, AttrKind::Directive) && a.name == "else"
                            }) {
                                let mut attrs_e = attrs2.clone();
                                attrs_e.remove(pos3);
                                let tmp_e = Node::Element {
                                    tag: tag2.clone(),
                                    attrs: attrs_e,
                                    children: ch2.clone(),
                                    self_closing: *sc2,
                                };
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
                    for part in chain_parts.iter() {
                        cond.push(' ');
                        cond.push_str(part);
                    }
                    if let Some(e) = else_part {
                        cond.push(' ');
                        cond.push_str(&e);
                    } else {
                        cond.push_str(r#" else { text("") }"#);
                    }
                    cond.push(')');
                    out.push_str(&format!("__children.push({});\n", cond));
                    i = if j > i { j } else { i + 1 };
                    continue;
                }

                // v-for handling for state collections: `item in items` or `(item, idx) in items`
                if let Some(posf) = attrs
                    .iter()
                    .position(|a| matches!(a.kind, AttrKind::Directive) && a.name == "for")
                {
                    // Also check for v-if on the same element (v-for + v-if combination)
                    let v_if_pos = attrs
                        .iter()
                        .position(|a| matches!(a.kind, AttrKind::Directive) && a.name == "if");

                    let mut attrs_f = attrs.clone();
                    let dir = attrs_f.remove(posf);
                    let val = dir.value.unwrap_or_default();

                    // Extract :key attribute if present
                    let key_attr_pos = attrs_f
                        .iter()
                        .position(|a| a.kind == AttrKind::Bind && a.name == "key");
                    let _key_expr = if let Some(kp) = key_attr_pos {
                        let key_val = attrs_f[kp].value.clone();
                        attrs_f.remove(kp);
                        Some(key_val)
                    } else {
                        None
                    };

                    if let Some(for_info) = parse_v_for(&val) {
                        let tmp_elem = Node::Element {
                            tag: tag.clone(),
                            attrs: attrs_f,
                            children: ch.clone(),
                            self_closing: *self_closing,
                        };

                        // iterate over state.<expr> with proper empty collection handling
                        out.push_str(&format!(
                            "if let Some(__col) = state.{}.as_ref() {{\n",
                            for_info.expr
                        ));
                        out.push_str("    if !__col.is_empty() {\n");
                        out.push_str(&format!(
                            "        for ({idx_var}, {item_var}) in __col.iter().enumerate() {{\n",
                            idx_var = for_info.index_name,
                            item_var = for_info.item_name
                        ));

                        // emit inner with ctx mapping
                        let inner = emit_node_with_ctx_state(
                            &tmp_elem,
                            Some(&for_info.item_name),
                            Some(&for_info.index_name),
                        );

                        // Handle v-for + v-if: wrap inner in v-if check inside the loop
                        if let Some(if_pos) = v_if_pos {
                            let dir_if = &attrs[if_pos];
                            let expr_if =
                                rewrite_if_expr(&dir_if.value.clone().unwrap_or_default());
                            out.push_str(&format!(
                                "            if ({}) {{\n                __children.push({});\n            }}\n",
                                expr_if.trim(),
                                inner
                            ));
                        } else {
                            out.push_str(&format!("            __children.push({});\n", inner));
                        }

                        out.push_str("        }\n");
                        out.push_str("    }\n");
                        out.push_str("}\n");
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
        Node::Element {
            tag,
            attrs,
            children,
            ..
        } => {
            // handle v-for directive in render_with_state path
            if let Some(pos_for) = attrs
                .iter()
                .position(|a| matches!(a.kind, AttrKind::Directive) && a.name == "for")
            {
                let v_if_pos = attrs
                    .iter()
                    .position(|a| matches!(a.kind, AttrKind::Directive) && a.name == "if");

                let mut attrs_f = attrs.clone();
                let dir = attrs_f.remove(pos_for);
                let val = dir.value.unwrap_or_default();

                // Extract :key attribute if present
                let key_attr_pos = attrs_f
                    .iter()
                    .position(|a| a.kind == AttrKind::Bind && a.name == "key");
                let _key_expr = if let Some(kp) = key_attr_pos {
                    let key_val = attrs_f[kp].value.clone();
                    attrs_f.remove(kp);
                    Some(key_val)
                } else {
                    None
                };

                if let Some(for_info) = parse_v_for(&val) {
                    let tmp_elem = Node::Element {
                        tag: tag.clone(),
                        attrs: attrs_f,
                        children: children.clone(),
                        self_closing: false,
                    };

                    let mut loop_code = String::new();
                    loop_code
                        .push_str("{ let mut __children: Vec<velox_dom::VNode> = Vec::new();\n");
                    loop_code.push_str(&format!(
                        "if let Some(__col) = state.{}.as_ref() {{\n",
                        for_info.expr
                    ));
                    loop_code.push_str("    if !__col.is_empty() {\n");
                    loop_code.push_str(&format!(
                        "        for ({idx_var}, {item_var}) in __col.iter().enumerate() {{\n",
                        idx_var = for_info.index_name,
                        item_var = for_info.item_name
                    ));

                    let inner = emit_node_with_ctx_state(
                        &tmp_elem,
                        Some(&for_info.item_name),
                        Some(&for_info.index_name),
                    );

                    // Handle v-for + v-if
                    if let Some(if_pos) = v_if_pos {
                        let dir_if = &attrs[if_pos];
                        let expr_if = rewrite_if_expr(&dir_if.value.clone().unwrap_or_default());
                        loop_code.push_str(&format!(
                            "            if ({}) {{\n                __children.push({});\n            }}\n",
                            expr_if.trim(),
                            inner
                        ));
                    } else {
                        loop_code.push_str(&format!("            __children.push({});\n", inner));
                    }

                    loop_code.push_str("        }\n");
                    loop_code.push_str("    }\n");
                    loop_code.push_str("}\n");
                    loop_code.push_str("__children }");

                    return loop_code;
                }
            }

            // Check if this is a component (has data-velox-component marker)
            if let Some(component_attr) = attrs
                .iter()
                .find(|a| a.name == "data-velox-component" && a.kind == AttrKind::Static)
                && let Some(comp_name) = &component_attr.value
            {
                // Remove the marker attribute from props
                let mut clean_attrs = attrs.clone();
                clean_attrs.retain(|a| a.name != "data-velox-component");

                // Extract bind/on attrs and generate props HashMap
                let (_has_props, props_expr) = generate_component_props_expr(&clean_attrs);
                let _kids = emit_children_with_state(children);

                // Collect @event handlers as callbacks
                let callbacks = collect_component_callbacks(&clean_attrs);

                if callbacks.is_empty() {
                    return format!(
                        r#"{{ let __props = {props_expr}; let component_vnode = {comp_name}::render_with_props(__props); h("div", Props::new().set("data-component", "{}"), vec![component_vnode]) }}"#,
                        comp_name
                    );
                }

                // Generate callback map and use render_with_callbacks
                let callback_map = format_callback_map(&callbacks);
                let callback_names: Vec<String> = callbacks
                    .iter()
                    .map(|(_, handler)| handler.clone())
                    .collect();

                return format!(
                    r#"{{ let __props = {props_expr}; let __callbacks = {callback_map}; let component_vnode = {comp_name}::render_with_callbacks(__props, &__callbacks, &[{callback_names}]); h("div", Props::new().set("data-component", "{}"), vec![component_vnode]) }}"#,
                    comp_name,
                    callback_names = callback_names
                        .iter()
                        .map(|n| format!("\"{}\"", n))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }

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
                if key == item {
                    return "text(&format!(\"{}\", __item))".to_string();
                }
                // Handle dot notation: item.property or item.nested.property
                if key.starts_with(item)
                    && key.len() > item.len()
                    && key.as_bytes()[item.len()] == b'.'
                {
                    let prop_path = &key[item.len()..];
                    return format!(
                        r#"text(&{{ let __obj = &__item; __obj{}.to_string() }})"#,
                        prop_path
                    );
                }
            }
            if let Some(idx) = idx_name
                && key == idx
            {
                return "text(&__idx.to_string())".to_string();
            }
            let key_lit = string_lit(key);
            format!(r#"text(&resolve({}))"#, key_lit)
        }
        Node::Element {
            tag,
            attrs,
            children,
            ..
        } => {
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

#[allow(dead_code)]
fn emit_node_with_ctx(n: &Node, loop_var: Option<&str>) -> String {
    match n {
        Node::Text(t) => format!(r#"text({})"#, string_lit(t)),
        Node::Interpolation(expr) => {
            let key = expr.trim();
            if let Some(var) = loop_var {
                if key == var {
                    return "text(&__i.to_string())".to_string();
                }
                // Handle dot notation: item.property or item.nested.property
                if key.starts_with(var)
                    && key.len() > var.len()
                    && key.as_bytes()[var.len()] == b'.'
                {
                    let prop_path = &key[var.len()..];
                    return format!(
                        r#"text(&{{ let __obj = __i; __obj{}.to_string() }})"#,
                        prop_path
                    );
                }
            }
            let key_lit = string_lit(key);
            format!(r#"text(&resolve({}))"#, key_lit)
        }
        Node::Element {
            tag,
            attrs,
            children,
            ..
        } => {
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

/// Emit a node with context for the render() path inside a v-for loop.
/// This version uses resolve() with indexed access for collection items.
/// For `item in items`, it generates code like `resolve("items[__i].property")`
/// or `resolve("items[__i]")` for the item itself.
fn emit_node_with_ctx_for_loop(n: &Node, for_info: &VForInfo) -> String {
    match n {
        Node::Text(t) => format!(r#"text({})"#, string_lit(t)),
        Node::Interpolation(expr) => {
            let key = expr.trim();
            if key == for_info.item_name {
                // Direct item reference: use indexed access via resolve
                return format!(
                    r#"text(&resolve(&format!("{}[{{}}]", {})))"#,
                    for_info.expr, for_info.index_name
                );
            }
            // Handle dot notation: item.property -> items[__i].property
            if key.starts_with(&for_info.item_name)
                && key.len() > for_info.item_name.len()
                && key.as_bytes()[for_info.item_name.len()] == b'.'
            {
                let prop_path = &key[for_info.item_name.len()..];
                return format!(
                    r#"text(&resolve(&format!("{expr}[{{}}]{prop_path}", {idx})))"#,
                    expr = for_info.expr,
                    idx = for_info.index_name,
                );
            }
            if key == for_info.index_name {
                return format!(r#"text(&{}.to_string())"#, for_info.index_name);
            }
            let key_lit = string_lit(key);
            format!(r#"text(&resolve({}))"#, key_lit)
        }
        Node::Element {
            tag,
            attrs,
            children,
            ..
        } => {
            let props = emit_props_with(attrs);
            let kids = {
                let mut k_items: Vec<String> = Vec::new();
                for c in children {
                    k_items.push(emit_node_with_ctx_for_loop(c, for_info));
                }
                format!("vec![{}]", k_items.join(", "))
            };
            format!(r#"h("{}", {props}, {kids})"#, tag)
        }
    }
}

/// Collect `@event="handler"` attrs from component attributes.
/// Returns a list of `(event_name, handler_name)` pairs.
fn collect_component_callbacks(attrs: &[TemplateAttr]) -> Vec<(String, String)> {
    attrs
        .iter()
        .filter(|a| a.kind == AttrKind::On)
        .filter_map(|a| {
            let handler = a.value.clone().unwrap_or_default();
            if handler.is_empty() {
                None
            } else {
                Some((a.name.clone(), handler))
            }
        })
        .collect()
}

/// Generate a Rust expression for a HashMap of callbacks.
/// Input: `[("change", "on_change"), ("submit", "on_submit")]`
/// Output: `std::collections::HashMap::from([("change", "on_change"), ("submit", "on_submit")])`
fn format_callback_map(callbacks: &[(String, String)]) -> String {
    if callbacks.is_empty() {
        return "std::collections::HashMap::new()".to_string();
    }
    let entries: Vec<String> = callbacks
        .iter()
        .map(|(event, handler)| format!("({}, {})", string_lit(event), string_lit(handler)))
        .collect();
    format!("std::collections::HashMap::from([{}])", entries.join(", "))
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
