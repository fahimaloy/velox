use crate::component_resolver::ComponentResolver;
use crate::script_index::{StateMethod, extract_state_methods, method_names, resolve_method_name};
use crate::template_ast::{AttrKind, Node, TemplateAttr};
use crate::template_parse::is_all_ws;
use std::collections::{HashMap, HashSet};

/// Determines how identifiers are resolved in generated code.
#[derive(Debug, Clone, Copy, PartialEq)]
enum TransformMode {
    /// Wrap identifiers in `resolve()` calls — used by `render_with()`.
    Resolve,
    /// Reference `state.field.get()` directly — used by `render_with_state()`.
    State,
}

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

/// Validate the parsed template AST and collect structural errors.
/// Returns a list of error/warning messages (empty if the template is valid).
fn validate_template(nodes: &[Node]) -> Vec<String> {
    let mut errors: Vec<String> = Vec::new();

    // 1. Multiple root template nodes
    // A well-formed SFC template should have exactly one root element.
    // (Whitespace-only text nodes are already trimmed by the parser, so
    //  any remaining root nodes are real elements.)
    let root_elements: Vec<&Node> = nodes
        .iter()
        .filter(|n| matches!(n, Node::Element { .. }))
        .collect();
    if root_elements.len() > 1 {
        let tags: Vec<String> = root_elements
            .iter()
            .filter_map(|n| match n {
                Node::Element { tag, .. } => Some(tag.clone()),
                _ => None,
            })
            .collect();
        errors.push(format!(
            "template has multiple root elements: <{}> — a valid SFC template must have exactly one root element",
            tags.join(", ")
        ));
    }

    // 2. Stray v-else / v-else-if without preceding v-if
    // Walk the tree checking that every v-else or v-else-if is immediately
    // preceded by a sibling with v-if or v-else-if (part of a valid chain).
    fn check_stray_else(nodes: &[Node], errors: &mut Vec<String>) {
        for i in 0..nodes.len() {
            if let Node::Element { attrs, tag, .. } = &nodes[i]
                && let Some(dir) = attrs.iter().find(|a| {
                    matches!(a.kind, AttrKind::Directive)
                        && (a.name == "else" || a.name == "else-if" || a.name == "elseif")
                })
            {
                // Check that the nearest preceding element sibling has
                // v-if or v-else-if (i.e., is part of a conditional chain).
                let has_valid_preceding = if i > 0 {
                    // Walk backwards to find the nearest element sibling
                    // (text nodes between v-if and v-else are valid in Vue)
                    let mut found = false;
                    for j in (0..i).rev() {
                        if let Node::Element {
                            attrs: prev_attrs, ..
                        } = &nodes[j]
                        {
                            found = prev_attrs.iter().any(|a| {
                                matches!(a.kind, AttrKind::Directive)
                                    && (a.name == "if" || a.name == "else-if" || a.name == "elseif")
                            });
                            break;
                        }
                    }
                    found
                } else {
                    false
                };

                if !has_valid_preceding {
                    errors.push(format!(
                            "stray v-{} on <{}> without a preceding v-if — v-else/v-else-if must follow an element with v-if",
                            dir.name, tag
                        ));
                }
            }
            // Recurse into children
            if let Node::Element { children, .. } = &nodes[i] {
                check_stray_else(children, errors);
            }
        }
    }
    check_stray_else(nodes, &mut errors);

    errors
}

/// Compile a `<template>` with no script indexing or cross-component handlers.
/// Convenience wrapper used by tests and single-file compilation.
pub fn compile_template_to_rs(
    template_src: &str,
    component_name: &str,
    resolver: Option<&mut ComponentResolver>,
) -> Result<String, String> {
    compile_template_to_rs_full(template_src, component_name, resolver, None, None)
}

/// Full public API: compile `<template>` string to a Rust module body with `render()`.
///
/// `script_setup` is the raw `<script setup>` block (if any); it is indexed so
/// template keys can be resolved to the actual State method names and component
/// tags can be matched to persistent child-state fields.
///
/// `scope_id` is the optional CSS scope attribute (e.g. `"data-v-abc123"`) that
/// should be added to every rendered element so scoped CSS selectors match.
///
/// When a `resolver` is provided, handlers are collected across the component
/// tree so the root dispatcher can route child-component events to the owning
/// persistent State field.
pub fn compile_template_to_rs_full(
    template_src: &str,
    _component_name: &str,
    mut resolver: Option<&mut ComponentResolver>,
    script_setup: Option<&str>,
    scope_id: Option<&str>,
) -> Result<String, String> {
    let nodes = crate::template_parse::parse_template_to_ast(template_src)?;

    // Validate template structure before codegen.
    let validation_errors = validate_template(&nodes);
    if !validation_errors.is_empty() {
        return Err(validation_errors.join("\n"));
    }

    // For MVP, assume a single root node.
    let mut nodes = nodes;

    // Index the script block so codegen can resolve template keys to the real
    // State method names and match component tags to persistent state fields.
    let methods = script_setup.map(extract_state_methods).unwrap_or_default();
    let names = method_names(&methods);
    let fields = script_setup
        .map(crate::script_index::extract_state_fields)
        .unwrap_or_default();

    // Transform components if resolver is provided.
    if let Some(resolver) = &mut resolver {
        super::component_resolver::transform_components(&mut nodes, resolver);
    }

    // Collect handlers from the whole component tree so the dispatcher can route
    // child-component events to the owning persistent State field.
    let tree_handlers = if let Some(resolver) = &mut resolver {
        collect_component_tree_handlers(&nodes, resolver, &fields)
    } else {
        Vec::new()
    };

    if nodes.is_empty() {
        return Ok(empty_component_module());
    }

    // For MVP, assume a single root node.
    let root = &nodes[0];
    let body_with = emit_node_with(root, &fields, scope_id);
    let body_with_state = emit_node_with_state(root, &fields, scope_id);

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

    // render_with_state accepts a `state: Arc<script_rs::State>`; the param is
    // named `state` so v-for/component codegen can read `state.{field}`.
    out.push_str("\n\n");
    out.push_str(&format!(
        r#"#[allow(clippy::arc_with_non_send_sync)]
#[allow(unused_variables)]
pub fn render_with_state<F>(state: std::sync::Arc<script_rs::State>, mut resolve: F) -> velox_dom::VNode where F: FnMut(&str) -> String {{
    use velox_dom::*;
    {body_with_state}
}}"#,
        body_with_state = body_with_state
    ));

    // make_resolve: maps template interpolation keys to State getters so main.rs
    // does not hand-write a resolve closure. Supports both bare (`title()`) and
    // prefixed (`get_title()`) getter conventions.
    let interp_keys = collect_interpolation_keys(&nodes);
    out.push_str("\n\n");
    out.push_str(&generate_make_resolve(&interp_keys, &names));

    // make_on_event: dispatch every template handler (plus, for the root, every
    // handler anywhere in the component tree) to the owning State method.
    let handlers = collect_handlers(&nodes);
    out.push_str("\n\n");
    out.push_str(&generate_make_on_event(&handlers, &tree_handlers, &methods));

    // render_with_props: for presentational child components. Props take
    // priority; interpolations fall back to State getters.
    out.push_str("\n\n");
    out.push_str(&generate_render_with_props(&interp_keys, &names));

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
                    // Parse event modifiers: @click.stop, @click.prevent, etc.
                    let (base_event, modifier) = parse_event_modifier(v);
                    let handler_key = if let Some(m) = modifier {
                        format!("{}.{}", base_event, m)
                    } else {
                        base_event.clone()
                    };
                    set.insert(handler_key);
                }
                // Collect v-model handlers: v-model="field" → __vmodel_set_field
                if matches!(a.kind, AttrKind::Directive)
                    && a.name == "model"
                    && let Some(ref expr) = a.value
                {
                    set.insert(format!("__vmodel_set_{}", expr));
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

/// Parse an event handler string into (base_event, modifier).
/// e.g., "click.stop" → ("click", Some("stop"))
/// e.g., "click" → ("click", None)
fn parse_event_modifier(handler: &str) -> (String, Option<String>) {
    let parts: Vec<&str> = handler.split('.').collect();
    if parts.len() >= 2 {
        // Check if the last part is a known modifier
        let modifier = parts[parts.len() - 1].to_string();
        let known_modifiers = ["stop", "prevent", "once", "capture", "self"];
        if known_modifiers.iter().any(|&m| m == modifier) {
            let base: String = parts[..parts.len() - 1].join(".");
            (base, Some(modifier))
        } else {
            // Not a known modifier, treat as regular handler
            (handler.to_string(), None)
        }
    } else {
        (handler.to_string(), None)
    }
}

/// Collect every `@event` handler across the component tree, attributing each to
/// the nearest persistent State field (the field on this component's State that
/// matches the lowercased component tag). Used to build the root dispatcher so
/// child-component events route to `state.{owner}.{method}`.
fn collect_component_tree_handlers(
    nodes: &[Node],
    resolver: &mut ComponentResolver,
    fields: &[String],
) -> Vec<crate::script_index::TreeHandler> {
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    collect_tree_handlers_inner(nodes, resolver, fields, None, None, &mut out, &mut seen);
    out
}

#[allow(clippy::too_many_arguments)]
fn collect_tree_handlers_inner(
    nodes: &[Node],
    resolver: &mut ComponentResolver,
    fields: &[String],
    owner: Option<&str>,
    owner_methods: Option<&[StateMethod]>,
    out: &mut Vec<crate::script_index::TreeHandler>,
    seen: &mut HashSet<String>,
) {
    for n in nodes {
        let Node::Element {
            attrs, children, ..
        } = n
        else {
            continue;
        };

        // Local @event handlers on this element.
        for a in attrs {
            if let AttrKind::On = a.kind
                && let Some(v) = &a.value
            {
                let takes_payload = owner_methods
                    .map(|ms| ms.iter().any(|m| m.name == *v && m.takes_payload))
                    .unwrap_or(false);
                out.push(crate::script_index::TreeHandler {
                    name: v.clone(),
                    owner: owner.map(String::from),
                    takes_payload,
                });
            }
        }

        // Component element: descend into its template.
        if let Some(comp_attr) = attrs
            .iter()
            .find(|a| a.name == "data-velox-component" && a.kind == AttrKind::Static)
            && let Some(comp_name) = &comp_attr.value
        {
            let field = comp_name.to_lowercase();
            let becomes_owner = owner.is_none() && fields.contains(&field);
            let child_owner: Option<String> = if becomes_owner {
                Some(field)
            } else {
                owner.map(String::from)
            };

            if seen.insert(comp_name.clone())
                && let Ok(sfc) = resolver.load_component(comp_name)
            {
                // Clone content so `sfc`'s borrow ends before recursing.
                let child_script = sfc.script_setup.as_ref().map(|b| b.content.clone());
                let child_tpl = sfc.template.as_ref().map(|t| t.content.clone());

                // When this component becomes the owner, its State methods tell us
                // which handlers take a payload. Otherwise inherit the owner's.
                let subtree_methods = if becomes_owner {
                    child_script.as_deref().map(extract_state_methods)
                } else {
                    None
                };
                let child_methods: Option<&[StateMethod]> = if becomes_owner {
                    subtree_methods.as_deref()
                } else {
                    owner_methods
                };

                if let Some(tpl) = child_tpl
                    && let Ok(child_nodes) = crate::template_parse::parse_template_to_ast(&tpl)
                {
                    collect_tree_handlers_inner(
                        &child_nodes,
                        resolver,
                        fields,
                        child_owner.as_deref(),
                        child_methods,
                        out,
                        seen,
                    );
                }
            }
        }

        // Recurse into plain element children.
        collect_tree_handlers_inner(children, resolver, fields, owner, owner_methods, out, seen);
    }
}

fn generate_make_on_event(
    handlers: &[String],
    extra_handlers: &[crate::script_index::TreeHandler],
    methods: &[StateMethod],
) -> String {
    // Generate a dispatch helper that routes every template event to a State
    // method. v-model handlers (`__vmodel_set_*`) and methods declared with a
    // payload parameter receive the event payload; other handlers are zero-arg.
    // Handlers owned by a persistent child component state (`extra_handlers`)
    // route to `state.{owner}.{method}` instead of this component's State.
    // Event modifiers (`.stop`, `.prevent`, `.once`, `.capture`, `.self`) are
    // parsed from the handler name and the appropriate DOM method is called on
    // the payload event.
    let names = method_names(methods);
    let mut extra_by_name: HashMap<&str, &crate::script_index::TreeHandler> = HashMap::new();
    for eh in extra_handlers {
        extra_by_name.entry(&eh.name).or_insert(eh);
    }

    let mut arms = String::new();
    let mut used: HashSet<String> = HashSet::new();
    let mut uses_payload = false;

    // Known event modifiers and the Rust code to call on the DOM event
    let modifier_handlers: HashMap<&str, &str> = [
        ("stop", "e.stop_propagation()"),
        ("prevent", "e.prevent_default()"),
        ("once", "// mark as handled once"),
        ("capture", "// capture phase handling"),
        ("self", "// only handle if event target is this element"),
    ]
    .iter()
    .cloned()
    .collect();

    for h in handlers {
        if h.starts_with("__vmodel_set_") {
            arms.push_str(&format!(
                "        \"{name}\" => {{ if let Some(p) = payload {{ state.{name}(p); }} }},\n",
                name = h
            ));
            uses_payload = true;
        } else if let Some(eh) = extra_by_name.get(h.as_str())
            && let Some(owner) = eh.owner.as_deref()
        {
            // Handler owned by a persistent child component state field.
            if eh.takes_payload {
                arms.push_str(&format!(
                    "        \"{name}\" => {{ if let Some(p) = payload {{ state.{owner}.{method}(p); }} }},\n",
                    name = h, owner = owner, method = eh.name
                ));
                uses_payload = true;
            } else {
                arms.push_str(&format!(
                    "        \"{name}\" => {{ state.{owner}.{method}(); }},\n",
                    name = h,
                    owner = owner,
                    method = eh.name
                ));
            }
        } else {
            // Local handler on this component's State.
            // Parse event modifiers from the handler name (e.g., "click.stop")
            let (base_event, modifier) = parse_event_modifier(h);
            let method = resolve_method_name(&names, &base_event);
            let takes_payload = methods.iter().any(|m| m.name == method && m.takes_payload);

            if takes_payload {
                // Generate arm with modifier handling
                let mod_code = match modifier.as_deref() {
                    Some("stop") => "e.stop_propagation()",
                    Some("prevent") => "e.prevent_default()",
                    Some("once") => "// mark as handled once",
                    Some("capture") => "// capture phase handling",
                    Some("self") => "// only handle if event target is this element",
                    _ => "",
                };
                arms.push_str(&format!(
                    "        \"{original}\" => {{ if let Some(p) = payload {{ state.{method}(p); {mod_code} }} }},\n",
                    original = h,
                    method = method,
                    mod_code = mod_code
                ));
                uses_payload = true;
            } else {
                // Zero-arg handler with modifier
                let mod_code = match modifier.as_deref() {
                    Some("stop") => "e.stop_propagation()",
                    Some("prevent") => "e.prevent_default()",
                    Some("once") => "// mark as handled once",
                    Some("capture") => "// capture phase handling",
                    Some("self") => "// only handle if event target is this element",
                    _ => "",
                };
                arms.push_str(&format!(
                    "        \"{original}\" => {{ state.{method}(); {mod_code} }},\n",
                    original = h,
                    method = method,
                    mod_code = mod_code
                ));
            }
        }
        used.insert(h.clone());
    }

    // Extra handlers not already covered by a local template event still need
    // an arm — they may be defined only inside a child component's template.
    // Handlers without an owner are local to this component (already covered);
    // only owner-attributed ones route through a persistent State field.
    for eh in extra_handlers {
        if used.contains(&eh.name) {
            continue;
        }
        let Some(owner) = eh.owner.as_deref() else {
            continue;
        };
        if eh.takes_payload {
            arms.push_str(&format!(
                "        \"{name}\" => {{ if let Some(p) = payload {{ state.{owner}.{method}(p); }} }},\n",
                name = eh.name, owner = owner, method = eh.name
            ));
            uses_payload = true;
        } else {
            arms.push_str(&format!(
                "        \"{name}\" => {{ state.{owner}.{method}(); }},\n",
                name = eh.name,
                owner = owner,
                method = eh.name
            ));
        }
    }

    format!(
        r#"#[allow(clippy::arc_with_non_send_sync)]
pub fn make_on_event(state: std::sync::Arc<script_rs::State>) -> impl FnMut(&str, Option<&str>) + 'static {{
    move |name: &str, payload: Option<&str>| {{
        match name {{
{arms}            _ => {{}}
        }}
    }}
}}"#,
        arms = arms
    )
}

/// Generate a `make_resolve(state)` helper that maps template interpolation keys
/// to State getters. `main.rs` uses it instead of hand-writing a resolve closure.
fn generate_make_resolve(interp_keys: &[String], names: &[String]) -> String {
    let mut arms = String::new();
    for key in interp_keys {
        let method = resolve_method_name(names, key);
        arms.push_str(&format!(
            "        \"{}\" => state.{}().to_string(),\n",
            key, method
        ));
    }
    format!(
        r#"#[allow(clippy::arc_with_non_send_sync)]
pub fn make_resolve(state: std::sync::Arc<script_rs::State>) -> impl FnMut(&str) -> String {{
    move |name: &str| match name {{
{arms}        _ => String::new(),
    }}
}}"#,
        arms = arms
    )
}

/// Generate `render_with_props` — props take priority, interpolations fall back
/// to State getters so a presentational child renders correctly even when the
/// parent omits a prop.
fn generate_render_with_props(interp_keys: &[String], names: &[String]) -> String {
    if interp_keys.is_empty() {
        return r#"pub fn render_with_props(props: std::collections::HashMap<&str, String>) -> velox_dom::VNode {
    let resolve_props = |key: &str| -> String {
        props.get(key).cloned().unwrap_or_default()
    };
    render_with(resolve_props)
}"#
        .to_string();
    }
    let mut match_arms = String::new();
    for key in interp_keys {
        let method = resolve_method_name(names, key);
        match_arms.push_str(&format!(
            "            \"{}\" => state.{}().to_string(),\n",
            key, method
        ));
    }
    format!(
        r#"pub fn render_with_props(props: std::collections::HashMap<&str, String>) -> velox_dom::VNode {{
    let state = script_rs::State::new();
    let resolve_props = |key: &str| -> String {{
        if let Some(v) = props.get(key) {{
            v.clone()
        }} else {{
            match key {{
{match_arms}                _ => String::new(),
            }}
        }}
    }};
    render_with(resolve_props)
}}"#,
        match_arms = match_arms
    )
}

/// Minimal module emitted when a component has no template.
fn empty_component_module() -> String {
    r#"pub fn render() -> velox_dom::VNode {
    use velox_dom::*;
    text("")
}

pub fn render_with<F>(mut resolve: F) -> velox_dom::VNode where F: FnMut(&str) -> String {
    use velox_dom::*;
    text("")
}

#[allow(clippy::arc_with_non_send_sync)]
pub fn render_with_state<F>(state: std::sync::Arc<script_rs::State>, mut resolve: F) -> velox_dom::VNode where F: FnMut(&str) -> String {
    use velox_dom::*;
    text("")
}

#[allow(clippy::arc_with_non_send_sync)]
pub fn make_resolve(state: std::sync::Arc<script_rs::State>) -> impl FnMut(&str) -> String {
    move |_name: &str| String::new()
}

pub fn make_on_event(state: std::sync::Arc<script_rs::State>) -> impl FnMut(&str, Option<&str>) + 'static {
    move |_name: &str, _payload: Option<&str>| {}
}

pub fn render_with_props(props: std::collections::HashMap<&str, String>) -> velox_dom::VNode {
    let resolve_props = |key: &str| -> String {
        props.get(key).cloned().unwrap_or_default()
    };
    render_with(resolve_props)
}"#
    .to_string()
}

/// Emit code for a `<slot>` element.
///
/// `<slot>` elements appear inside component templates and act as outlets for
/// slot content passed from the parent component. The slot's name is determined
/// by the `name` attribute (defaulting to "default").
///
/// Slot content is passed from the parent as a `HashMap<&str, VNode>` under
/// the key "slot:{name}". At render time, `render_slot` looks up the slot name
/// and returns the slot VNode, falling back to the slot's own children
/// (fallback content) if no slot was provided.
fn emit_slot_node(
    attrs: &[TemplateAttr],
    children: &[Node],
    mode: TransformMode,
    fields: &[String],
    scope_id: Option<&str>,
) -> String {
    let slot_name = attrs
        .iter()
        .find(|a| a.name == "name" && matches!(a.kind, AttrKind::Static))
        .and_then(|a| a.value.clone())
        .unwrap_or_else(|| "default".to_string());

    // Emit fallback content (the slot's children).
    let fallback = emit_children_with_mode(children, mode, fields, scope_id);

    format!(
        r#"render_slot({slot_name_lit}, || {{ {fallback} }})"#,
        slot_name_lit = string_lit(&slot_name),
        fallback = fallback,
    )
}

pub(crate) fn emit_node(n: &Node) -> String {
    match n {
        Node::Text(t) => format!(r#"text({})"#, string_lit(t)),
        Node::Interpolation(expr) => {
            let key = string_lit(expr.trim());
            format!(r#"text(resolve({}))"#, key)
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

#[cfg_attr(test, allow(non_snake_case))]
pub(crate) fn rewrite_if_expr(expr: &str) -> String {
    let has_cmp = expr.contains("==")
        || expr.contains("!=")
        || expr.contains(">=")
        || expr.contains("<=")
        || expr.contains('>')
        || expr.contains('<');
    let has_logic = expr.contains("&&") || expr.contains("||");
    let has_negation = expr.contains('!');

    let mut out = String::new();
    let mut ident = String::new();
    let mut chars = expr.chars().peekable();

    fn flush_ident(out: &mut String, ident: &mut String, has_cmp: bool, _has_logic: bool) {
        if ident.is_empty() {
            return;
        }
        let token = ident.as_str();
        let keep = token == "true"
            || token == "false"
            || token == "resolve"
            || token == "state"
            || token.contains('.')
            || token.contains('(');
        if keep {
            out.push_str(token);
        } else if has_cmp {
            out.push_str(&format!(
                "resolve({}).parse::<f64>().unwrap_or(0.0)",
                string_lit(token)
            ));
        } else {
            // Use a let binding to avoid calling resolve() multiple times
            out.push_str(&format!(
                "{{ let __v = resolve({}); __v == \"true\" || (!__v.is_empty() && __v != \"false\") }}",
                string_lit(token)
            ));
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
            flush_ident(&mut out, &mut ident, has_cmp, has_logic);
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
        } else if ch == '!' && has_negation {
            // `!` operator: if followed by an identifier, negate the truthy check
            // e.g., `!is_positive` -> !(resolve("is_positive") == "true" || ...)
            out.push_str("!(");
            // The next token will be collected by the ident loop and flushed,
            // but we need to close the paren after it. We'll handle this by
            // tracking that we need a closing paren after the next ident flush.
            // Instead, let's handle ! more carefully: consume the ident after !
            let mut neg_ident = String::new();
            while let Some(&next) = chars.peek() {
                if next.is_ascii_alphanumeric() || next == '_' {
                    neg_ident.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
            if neg_ident.is_empty() {
                // standalone !, just emit it
                out.pop(); // remove the "(" we just pushed
                out.push('!');
            } else if neg_ident == "true" || neg_ident == "false" {
                out.push_str(&neg_ident);
                out.push(')');
            } else {
                // Use a let binding to avoid calling resolve() multiple times
                out.push_str(&format!(
                    "{{ let __v = resolve({}); __v == \"true\" || (!__v.is_empty() && __v != \"false\") }})",
                    string_lit(&neg_ident)
                ));
            }
        } else {
            out.push(ch);
        }
    }
    flush_ident(&mut out, &mut ident, has_cmp, has_logic);
    out
}

/// Extract v-model directive from attrs and convert to :value + @input.
/// Returns (remaining_attrs, optional_vmodel_handler_name).
/// The handler name follows the convention `__vmodel_set_{field}` so that
/// `make_on_event` can dispatch to it and the codegen can emit the setter.
fn extract_vmodel(attrs: &[TemplateAttr], _tag: &str) -> (Vec<TemplateAttr>, Option<String>) {
    let vmodel_pos = attrs
        .iter()
        .position(|a| matches!(a.kind, AttrKind::Directive) && a.name == "model");

    if let Some(pos) = vmodel_pos {
        let vmodel_attr = &attrs[pos];
        let model_expr = vmodel_attr.value.clone().unwrap_or_default();

        // Build remaining attrs without v-model
        let mut remaining: Vec<TemplateAttr> = attrs.to_vec();
        remaining.remove(pos);

        // Add :value binding for v-model
        remaining.push(TemplateAttr {
            name: "value".to_string(),
            value: Some(model_expr.clone()),
            kind: AttrKind::Bind,
        });

        // Generate @input handler that calls state.__vmodel_set_{field}(payload)
        let handler_name = format!("__vmodel_set_{}", model_expr.replace('.', "_"));
        remaining.push(TemplateAttr {
            name: "input".to_string(),
            value: Some(handler_name.clone()),
            kind: AttrKind::On,
        });

        (remaining, Some(model_expr))
    } else {
        (attrs.to_vec(), None)
    }
}

fn emit_node_with_mode(n: &Node, mode: TransformMode, fields: &[String], scope_id: Option<&str>) -> String {
    match n {
        Node::Text(t) => format!(r#"text({})"#, string_lit(t)),
        Node::Interpolation(expr) => {
            let key = string_lit(expr.trim());
            format!(r#"text(resolve({}))"#, key)
        }
        Node::Element {
            tag,
            attrs,
            children,
            ..
        } => {
            // Handle <slot> elements: render slot content passed as props from parent.
            // Slot content is stored as a serialized VNode tree in props under
            // "slot:{name}" (or "slot:default" for unnamed slots).
            if tag == "slot" {
                return emit_slot_node(attrs, children, mode, fields, scope_id);
            }

            // Handle v-model directive: convert to :value + @input
            let (attrs2, _v_model_handler) = extract_vmodel(attrs, tag);
            let attrs = &attrs2;

            // handle directive `v-if`
            if let Some(pos) = attrs
                .iter()
                .position(|a| matches!(a.kind, AttrKind::Directive) && a.name == "if")
            {
                let mut attrs2 = attrs.clone();
                let dir = attrs2.remove(pos);
                let expr = rewrite_if_expr(&dir.value.unwrap_or_default());
                let tmp = Node::Element {
                    tag: tag.clone(),
                    attrs: attrs2,
                    children: children.clone(),
                    self_closing: false,
                };
                let inner = emit_node_with_mode(&tmp, mode, fields, scope_id);
                return format!(r#"if {} {{ {} }} else {{ text("") }}"#, expr.trim(), inner);
            }

            // handle directive `v-show`
            // v-show always renders the element but toggles CSS `display: none`
            // based on the expression, preserving it in the DOM (unlike v-if).
            if let Some(pos) = attrs
                .iter()
                .position(|a| matches!(a.kind, AttrKind::Directive) && a.name == "show")
            {
                let mut attrs2 = attrs.clone();
                let dir = attrs2.remove(pos);
                let expr = rewrite_if_expr(&dir.value.unwrap_or_default());

                // Build the element without the v-show directive, then conditionally
                // set display style based on the expression value.
                let inner = emit_node_with_mode(
                    &Node::Element {
                        tag: tag.clone(),
                        attrs: attrs2,
                        children: children.clone(),
                        self_closing: false,
                    },
                    mode,
                    fields,
                    scope_id,
                );

                // If the element already has a `style` attribute, we merge the
                // display value. Otherwise, we add a style attribute.
                let has_style = attrs
                    .iter()
                    .any(|a| a.name == "style" && matches!(a.kind, AttrKind::Static | AttrKind::Bind));

                if has_style {
                    // The style attr is already emitted by emit_node_with_mode.
                    // We wrap the result: if expression is false, override display.
                    return format!(
                        r#"{{ let __node = {inner}; if !({expr}) {{ if let velox_dom::VNode::Element {{ ref mut props, .. }} = __node {{ props.attrs.entry("style".to_string()).and_modify(|s| {{ if s.contains("display:") {{ /* preserve existing display */ }} else {{ s.push_str("; display: none"); }} }}).or_insert_with(|| "display: none".to_string()); }} }} __node }}"#,
                        inner = inner,
                        expr = expr.trim()
                    );
                } else {
                    return format!(
                        r#"{{ let __node = {inner}; if !({expr}) {{ if let velox_dom::VNode::Element {{ ref mut props, .. }} = __node {{ props.attrs.insert("style".to_string(), "display: none".to_string()); }} }} __node }}"#,
                        inner = inner,
                        expr = expr.trim()
                    );
                }
            }

            // Check if this is a component (has data-velox-component marker).
            // A component with v-for falls through to the v-for branch below so
            // it iterates instead of rendering once.
            let has_v_for = attrs
                .iter()
                .any(|a| matches!(a.kind, AttrKind::Directive) && a.name == "for");
            if !has_v_for
                && let Some(component_attr) = attrs
                    .iter()
                    .find(|a| a.name == "data-velox-component" && a.kind == AttrKind::Static)
                && let Some(comp_name) = &component_attr.value
            {
                let mut clean_attrs = attrs.clone();
                clean_attrs.retain(|a| a.name != "data-velox-component");

                // Separate slot children from props/events.
                // Children that are plain Element/Text/Interpolation nodes are
                // slot content (default slot). Children with v-slot:foo directive
                // or with a parent that has #foo / #default shorthand go into
                // named slots — for MVP we treat all non-prop children as the
                // default slot.
                let has_slot_children = !children.is_empty();

                // Persistent child state: when the parent State declares a field
                // matching the lowercased component tag and the instance is
                // prop-less, render with the child's persistent State so it can
                // own mutable state across renders (state survives because it is
                // created once in the parent's State::new()).
                let field = comp_name.to_lowercase();
                let has_props_events = clean_attrs
                    .iter()
                    .any(|a| matches!(a.kind, AttrKind::Bind | AttrKind::On));
                if mode == TransformMode::State && !has_props_events && !has_slot_children && fields.contains(&field) {
                    return format!(
                        r#"{comp_name}::render_with_state(std::sync::Arc::clone(&state.{field}), {comp_name}::make_resolve(std::sync::Arc::clone(&state.{field})))"#,
                    );
                }

                let (_has_props, props_expr) = generate_component_props_expr(&clean_attrs);
                let callbacks = collect_component_callbacks(&clean_attrs);

                if callbacks.is_empty() && !has_slot_children {
                    return format!(
                        r#"{{ let __props = {props_expr}; {comp_name}::render_with_props(__props) }}"#,
                    );
                }

                // Build slots map from default slot children.
                // Named slots (#foo="slotProps" or v-slot:foo) are a future
                // extension; for now all children become the "default" slot.
                let slots_expr = if has_slot_children {
                    let slot_vnodes: Vec<String> = children
                        .iter()
                        .map(|c| emit_node_with_mode(c, mode, fields, scope_id))
                        .collect();
                    format!(
                        r#"std::collections::HashMap::from([("default", {})])"#,
                        if slot_vnodes.len() == 1 {
                            slot_vnodes[0].clone()
                        } else {
                            format!("velox_dom::h(\"slot\", velox_dom::Props::new(), vec![{}])", slot_vnodes.join(", "))
                        }
                    )
                } else {
                    "std::collections::HashMap::new()".to_string()
                };

                // No slot children but has callbacks: use render_with_callbacks
                if !has_slot_children {
                    let callback_map = format_callback_map(&callbacks);
                    let callback_names: Vec<String> = callbacks
                        .iter()
                        .map(|(_, handler)| handler.clone())
                        .collect();

                    return format!(
                        r#"{{ let __props = {props_expr}; let __callbacks = {callback_map}; {comp_name}::render_with_callbacks(__props, &__callbacks, &[{callback_names}]) }}"#,
                        callback_names = callback_names
                            .iter()
                            .map(|n| format!("\"{}\"", n))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }

                // Has both callbacks and slot children: use render_with_slots
                let callback_map = format_callback_map(&callbacks);
                let callback_names: Vec<String> = callbacks
                    .iter()
                    .map(|(_, handler)| handler.clone())
                    .collect();

                return format!(
                    r#"{{ let __props = {props_expr}; let __callbacks = {callback_map}; let __slots = {slots_expr}; {comp_name}::render_with_slots(__props, &__callbacks, &[{callback_names}], &__slots) }}"#,
                    callback_names = callback_names
                        .iter()
                        .map(|n| format!("\"{}\"", n))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }

            // handle v-for directive
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

                let key_attr_pos = attrs_f
                    .iter()
                    .position(|a| a.kind == AttrKind::Bind && a.name == "key");
                let key_expr: Option<Option<String>> = if let Some(kp) = key_attr_pos {
                    let key_val = attrs_f[kp].value.clone();
                    attrs_f.remove(kp);
                    Some(key_val)
                } else {
                    None
                };

                if let Some(for_info) = parse_v_for(&val) {
                    let tmp_elem = Node::Element {
                        tag: tag.clone(),
                        attrs: attrs_f.clone(),
                        children: children.clone(),
                        self_closing: false,
                    };

                    let mut loop_code = String::new();
                    loop_code
                        .push_str("{ let mut __children: Vec<velox_dom::VNode> = Vec::new();\n");

                    match mode {
                        TransformMode::Resolve => {
                            // String-based iteration via resolve()
                            loop_code.push_str(&format!(
                                "let __for_expr = resolve(\"{}\");\n",
                                for_info.expr
                            ));
                            loop_code.push_str(
                                "let __for_count = if let Ok(n) = __for_expr.parse::<usize>() {\n",
                            );
                            loop_code.push_str("    n\n");
                            loop_code.push_str("} else if __for_expr.is_empty() {\n");
                            loop_code.push_str("    0\n");
                            loop_code.push_str("} else {\n");
                            loop_code.push_str("    __for_expr.split(',').count()\n");
                            loop_code.push_str("};\n");
                            loop_code.push_str(&format!(
                                "for {idx_var} in 0..__for_count {{\n",
                                idx_var = for_info.index_name
                            ));

                            let inner = emit_node_with_ctx_for_loop(&tmp_elem, &for_info, scope_id);

                            let inner_with_key = if let Some(Some(key_val)) = &key_expr {
                                format!(
                                    "{{ let mut __node = {}; if let velox_dom::VNode::Element {{ ref mut props, .. }} = __node {{ props.attrs.insert(\"key\".to_string(), resolve({}).to_string()); }} __node }}",
                                    inner,
                                    rewrite_if_expr(key_val)
                                )
                            } else {
                                inner
                            };

                            if let Some(if_pos) = v_if_pos {
                                let dir_if = &attrs[if_pos];
                                let expr_if =
                                    rewrite_if_expr(&dir_if.value.clone().unwrap_or_default());
                                loop_code.push_str(&format!(
                                    "    if {} {{\n        __children.push({});\n    }}\n",
                                    expr_if.trim(),
                                    inner_with_key
                                ));
                            } else {
                                loop_code.push_str(&format!(
                                    "    __children.push({});\n",
                                    inner_with_key
                                ));
                            }
                        }
                        TransformMode::State => {
                            // Collection-based iteration via state.{expr}.get()
                            loop_code
                                .push_str(&format!("let __col = state.{}.get();\n", for_info.expr));
                            loop_code.push_str("if !__col.is_empty() {\n");
                            loop_code.push_str(&format!(
                                "    for ({idx_var}, {item_var}) in __col.iter().enumerate() {{\n",
                                idx_var = for_info.index_name,
                                item_var = for_info.item_name
                            ));

                            let inner = emit_node_with_ctx_state(
                                &tmp_elem,
                                Some(&for_info.item_name),
                                Some(&for_info.index_name),
                                scope_id,
                            );

                            let inner_with_key = if let Some(Some(key_val)) = &key_expr {
                                let key_path_str = key_val
                                    .strip_prefix(&for_info.item_name)
                                    .map(|s| s.trim_start_matches('.'))
                                    .unwrap_or("id");
                                format!(
                                    "{{ let mut __node = {}; if let velox_dom::VNode::Element {{ ref mut props, .. }} = __node {{ props.attrs.insert(\"key\".to_string(), {item_var}.{key_path}.to_string()); }} __node }}",
                                    inner,
                                    item_var = for_info.item_name,
                                    key_path = key_path_str
                                )
                            } else {
                                inner.clone()
                            };

                            if let Some(if_pos) = v_if_pos {
                                let dir_if = &attrs[if_pos];
                                let expr_if =
                                    rewrite_if_expr(&dir_if.value.clone().unwrap_or_default());
                                loop_code.push_str(&format!(
                                    "    if {} {{\n        __children.push({});\n    }}\n",
                                    expr_if.trim(),
                                    inner_with_key
                                ));
                            } else {
                                loop_code.push_str(&format!(
                                    "    __children.push({});\n",
                                    inner_with_key
                                ));
                            }

                            loop_code.push_str("    }\n");
                            loop_code.push_str("}\n");
                        }
                    }

                    loop_code.push_str("__children; }");
                    return loop_code;
                }
            }

            let props = emit_props_with(attrs);
            let kids = emit_children_with_mode(children, mode, fields, scope_id);
            format!(r#"h("{}", {}, {kids})"#, tag, append_scope_attr(&props, scope_id))
        }
    }
}

fn emit_node_with(n: &Node, fields: &[String], scope_id: Option<&str>) -> String {
    emit_node_with_mode(n, TransformMode::Resolve, fields, scope_id)
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
                // `:click-payload="..."` maps to the renderer's on:click-payload.
                let key = if a.name == "click-payload" || a.name == "payload" {
                    "on:click-payload"
                } else {
                    &a.name
                };
                let key = string_lit(key);
                let expr = a.value.clone().unwrap_or_else(|| a.name.clone());
                let val_key = string_lit(expr.trim());
                bind_entries.push(format!("({key}, resolve({val_key}).clone())"));
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

/// If `expr` references a v-for loop variable, return the direct Rust expression
/// to read it (e.g. `todo` → `todo`, `todo.text` → `todo.text`, `idx` → `idx`).
/// Returns `None` when the expression is not a loop-variable reference (codegen
/// falls back to `resolve(...)`).
fn rewrite_ctx_expr(expr: &str, item_name: Option<&str>, idx_name: Option<&str>) -> Option<String> {
    let expr = expr.trim();
    if let Some(item) = item_name {
        if expr == item {
            return Some(item.to_string());
        }
        if expr.starts_with(item) && expr.len() > item.len() && expr.as_bytes()[item.len()] == b'.'
        {
            return Some(expr.to_string());
        }
    }
    if let Some(idx) = idx_name
        && expr == idx
    {
        return Some(idx.to_string());
    }
    None
}

/// Like [`generate_component_props_expr`] but resolves bind values that reference
/// v-for loop variables directly instead of through `resolve()`.
fn generate_component_props_expr_with_ctx(
    clean_attrs: &[TemplateAttr],
    item_name: Option<&str>,
    idx_name: Option<&str>,
) -> (bool, String) {
    let mut bind_entries: Vec<String> = Vec::new();
    for a in clean_attrs {
        match a.kind {
            AttrKind::Bind => {
                let key = if a.name == "click-payload" || a.name == "payload" {
                    "on:click-payload"
                } else {
                    &a.name
                };
                let key = string_lit(key);
                let expr = a.value.clone().unwrap_or_else(|| a.name.clone());
                if let Some(direct) = rewrite_ctx_expr(&expr, item_name, idx_name) {
                    bind_entries.push(format!("({key}, format!(\"{{}}\", {direct}))"));
                } else {
                    let val_key = string_lit(expr.trim());
                    bind_entries.push(format!("({key}, resolve({val_key}).clone())"));
                }
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

/// Like [`emit_props_with`] but resolves loop-variable bindings directly.
fn emit_props_with_ctx(
    attrs: &[TemplateAttr],
    item_name: Option<&str>,
    idx_name: Option<&str>,
) -> String {
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
                let key = if a.name == "click-payload" || a.name == "payload" {
                    "on:click-payload"
                } else {
                    &a.name
                };
                if let Some(direct) = rewrite_ctx_expr(&expr, item_name, idx_name) {
                    parts.push(format!(r#".set("{}", &format!("{{}}", {}))"#, key, direct));
                } else {
                    let val_key = string_lit(expr.trim());
                    parts.push(format!(r#".set("{}", &resolve({}))"#, key, val_key));
                }
            }
            AttrKind::Directive => {
                // directives are not emitted as props
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

/// Append the CSS scope attribute (e.g. `data-v-abc123`) to a props chain string.
/// When `scope_id` is `Some("data-v-xxx")`, appends `.set("data-v-xxx", "")` to the
/// Props builder so scoped CSS selectors match this element.
fn append_scope_attr(props: &str, scope_id: Option<&str>) -> String {
    if let Some(id) = scope_id {
        format!(r#"{}.set("{}", "")"#, props, id)
    } else {
        props.to_string()
    }
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
                // Special handling for :class with object syntax: { className: condition, ... }
                if a.name == "class" && expr.trim().starts_with('{') && expr.trim().ends_with('}') {
                    let inner = &expr.trim()[1..expr.trim().len() - 1];
                    let mut class_parts: Vec<String> = Vec::new();
                    for pair in inner.split(',') {
                        let pair = pair.trim();
                        if pair.is_empty() {
                            continue;
                        }
                        if let Some((cls, cond)) = pair.split_once(':') {
                            let cls = cls.trim().to_string();
                            let cond = cond.trim().to_string();
                            if !cls.is_empty() && !cond.is_empty() {
                                // Generate: if condition is true, add class
                                let rewritten = rewrite_if_expr(&cond);
                                class_parts.push(format!(
                                    "if {} {{ __classes.push({}); }}",
                                    rewritten,
                                    string_lit(&cls)
                                ));
                            }
                        }
                    }
                    if !class_parts.is_empty() {
                        let code = format!(
                            "{{ let mut __classes: Vec<&str> = Vec::new(); {} __classes.join(\" \") }}",
                            class_parts.join(" ")
                        );
                        parts.push(format!(r#".set("class", {})"#, code));
                    }
                } else {
                    let key_attr = if a.name == "click-payload" || a.name == "payload" {
                        "on:click-payload"
                    } else {
                        &a.name
                    };
                    let val_key = string_lit(expr.trim());
                    parts.push(format!(r#".set("{}", &resolve({}))"#, key_attr, val_key));
                }
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

fn emit_children_with_mode(children: &[Node], mode: TransformMode, fields: &[String], scope_id: Option<&str>) -> String {
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
                // v-if chain handling
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
                    let inner_if = emit_node_with_mode(&tmp_if, mode, fields, scope_id);

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
                                let inner_ei = emit_node_with_mode(&tmp_ei, mode, fields, scope_id);
                                chain_parts.push(format!(
                                    r#"else if {} {{ {} }}"#,
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
                                let inner_e = emit_node_with_mode(&tmp_e, mode, fields, scope_id);
                                else_part = Some(format!(r#"else {{ {} }}"#, inner_e));
                                j += 1;
                                break;
                            }
                        }
                        if let Node::Text(t) = &children[j]
                            && is_all_ws(t)
                        {
                            j += 1;
                            continue;
                        }
                        break;
                    }

                    let mut cond = String::new();
                    cond.push_str(&format!(r#"{{ if {} {{ {} }}"#, expr_if.trim(), inner_if));
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
                    cond.push_str(" }");
                    out.push_str(&format!("__children.push({});\n", cond));
                    i = if j > i { j } else { i + 1 };
                    continue;
                }

                // v-for handling
                if let Some(posf) = attrs
                    .iter()
                    .position(|a| matches!(a.kind, AttrKind::Directive) && a.name == "for")
                {
                    let v_if_pos = attrs
                        .iter()
                        .position(|a| matches!(a.kind, AttrKind::Directive) && a.name == "if");

                    let mut attrs_f = attrs.clone();
                    let dir = attrs_f.remove(posf);
                    let val = dir.value.unwrap_or_default();

                    let key_attr_pos = attrs_f
                        .iter()
                        .position(|a| a.kind == AttrKind::Bind && a.name == "key");
                    let key_expr = if let Some(kp) = key_attr_pos {
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

                        match mode {
                            TransformMode::Resolve => {
                                out.push_str(&format!(
                                    "let __for_expr = resolve(\"{}\");\n",
                                    for_info.expr
                                ));
                                out.push_str(
                                    "let __for_count = if let Ok(n) = __for_expr.parse::<usize>() {\n",
                                );
                                out.push_str("    n\n");
                                out.push_str("} else if __for_expr.is_empty() {\n");
                                out.push_str("    0\n");
                                out.push_str("} else {\n");
                                out.push_str("    __for_expr.split(',').count()\n");
                                out.push_str("};\n");
                                out.push_str(&format!(
                                    "for {idx_var} in 0..__for_count {{\n",
                                    idx_var = for_info.index_name
                                ));

                                let inner = emit_node_with_ctx_for_loop(&tmp_elem, &for_info, scope_id);

                                let inner_with_key = if let Some(Some(key_val)) = &key_expr {
                                    format!(
                                        "{{ let mut __node = {}; if let velox_dom::VNode::Element {{ ref mut props, .. }} = __node {{ props.attrs.insert(\"key\".to_string(), resolve({}).to_string()); }} __node }}",
                                        inner,
                                        rewrite_if_expr(key_val)
                                    )
                                } else {
                                    inner.clone()
                                };

                                if let Some(if_pos) = v_if_pos {
                                    let dir_if = &attrs[if_pos];
                                    let expr_if =
                                        rewrite_if_expr(&dir_if.value.clone().unwrap_or_default());
                                    out.push_str(&format!(
                                        "    if {} {{\n        __children.push({});\n    }}\n",
                                        expr_if.trim(),
                                        inner_with_key
                                    ));
                                } else {
                                    out.push_str(&format!(
                                        "    __children.push({});\n",
                                        inner_with_key
                                    ));
                                }
                            }
                            TransformMode::State => {
                                out.push_str(&format!(
                                    "let __col = state.{}.get();\n",
                                    for_info.expr
                                ));
                                out.push_str("if !__col.is_empty() {\n");
                                out.push_str(&format!(
                                    "    for ({idx_var}, {item_var}) in __col.iter().enumerate() {{\n",
                                    idx_var = for_info.index_name,
                                    item_var = for_info.item_name
                                ));

                                let inner = emit_node_with_ctx_state(
                                    &tmp_elem,
                                    Some(&for_info.item_name),
                                    Some(&for_info.index_name),
                                    scope_id,
                                );

                                let inner_with_key = if let Some(Some(key_val)) = &key_expr {
                                    let key_path_str = key_val
                                        .strip_prefix(&for_info.item_name)
                                        .map(|s| s.trim_start_matches('.'))
                                        .unwrap_or("id");
                                    format!(
                                        "{{ let mut __node = {}; if let velox_dom::VNode::Element {{ ref mut props, .. }} = __node {{ props.attrs.insert(\"key\".to_string(), {item_var}.{key_path}.to_string()); }} __node }}",
                                        inner,
                                        item_var = for_info.item_name,
                                        key_path = key_path_str
                                    )
                                } else {
                                    inner.clone()
                                };

                                if let Some(if_pos) = v_if_pos {
                                    let dir_if = &attrs[if_pos];
                                    let expr_if =
                                        rewrite_if_expr(&dir_if.value.clone().unwrap_or_default());
                                    out.push_str(&format!(
                                        "    if {} {{\n        __children.push({});\n    }}\n",
                                        expr_if.trim(),
                                        inner_with_key
                                    ));
                                } else {
                                    out.push_str(&format!(
                                        "    __children.push({});\n",
                                        inner_with_key
                                    ));
                                }

                                out.push_str("    }\n");
                                out.push_str("}\n");
                            }
                        }

                        // Resolve mode opens `for {idx} in 0..count {` but does not
                        // close it inline; State mode already closes its for and if.
                        if mode == TransformMode::Resolve {
                            out.push_str("}\n");
                        }
                        i += 1;
                        continue;
                    }
                }

                // default element
                let expr = emit_node_with_mode(&children[i], mode, fields, scope_id);
                out.push_str(&format!("__children.push({});\n", expr));
                i += 1;
            }
            _ => {
                let expr = emit_node_with_mode(&children[i], mode, fields, scope_id);
                out.push_str(&format!("__children.push({});\n", expr));
                i += 1;
            }
        }
    }
    out.push_str("__children\n}");
    out
}

fn emit_node_with_state(n: &Node, fields: &[String], scope_id: Option<&str>) -> String {
    emit_node_with_mode(n, TransformMode::State, fields, scope_id)
}

fn emit_node_with_ctx_state(n: &Node, item_name: Option<&str>, idx_name: Option<&str>, scope_id: Option<&str>) -> String {
    match n {
        Node::Text(t) => format!(r#"text({})"#, string_lit(t)),
        Node::Interpolation(expr) => {
            let key = expr.trim();
            if let Some(item) = item_name {
                if key == item {
                    return format!(r#"text(format!("{{}}", {item}))"#);
                }
                // Handle dot notation: item.property or item.nested.property
                if key.starts_with(item)
                    && key.len() > item.len()
                    && key.as_bytes()[item.len()] == b'.'
                {
                    let prop_path = &key[item.len()..];
                    return format!(
                        r#"text({{ let __obj = &{item}; __obj{}.to_string() }})"#,
                        prop_path
                    );
                }
            }
            if let Some(idx) = idx_name
                && key == idx
            {
                return format!(r#"text({idx}.to_string())"#);
            }
            let key_lit = string_lit(key);
            format!(r#"text(resolve({}))"#, key_lit)
        }
        Node::Element {
            tag,
            attrs,
            children,
            ..
        } => {
            // Handle <slot> inside v-for context
            if tag == "slot" {
                let slot_name = attrs
                    .iter()
                    .find(|a| a.name == "name" && matches!(a.kind, AttrKind::Static))
                    .and_then(|a| a.value.clone())
                    .unwrap_or_else(|| "default".to_string());

                // Fallback children rendered with ctx state
                let fallback_children: Vec<String> = children
                    .iter()
                    .map(|c| emit_node_with_ctx_state(c, item_name, idx_name, scope_id))
                    .collect();
                let fallback = format!("vec![{}]", fallback_children.join(", "));

                return format!(
                    r#"render_slot({slot_name_lit}, || {{ velox_dom::h(\"slot\", velox_dom::Props::new(), {fallback}) }})"#,
                    slot_name_lit = string_lit(&slot_name),
                    fallback = fallback,
                );
            }

            // handle directive `v-show` inside the loop body
            // v-show always renders the element but toggles CSS `display: none`
            // based on the expression, preserving it in the DOM (unlike v-if).
            if let Some(pos) = attrs
                .iter()
                .position(|a| matches!(a.kind, AttrKind::Directive) && a.name == "show")
            {
                let mut attrs2 = attrs.clone();
                let dir = attrs2.remove(pos);
                let expr = rewrite_if_expr(&dir.value.unwrap_or_default());

                // Build the element without the v-show directive, then conditionally
                // set display style based on the expression value.
                let inner = emit_node_with_ctx_state(
                    &Node::Element {
                        tag: tag.clone(),
                        attrs: attrs2.clone(),
                        children: children.clone(),
                        self_closing: false,
                    },
                    item_name,
                    idx_name,
                    scope_id,
                );

                // If the element already has a `style` attribute, we merge the
                // display value. Otherwise, we add a style attribute.
                let has_style = attrs
                    .iter()
                    .any(|a| a.name == "style" && matches!(a.kind, AttrKind::Static | AttrKind::Bind));

                if has_style {
                    // The style attr is already emitted by emit_node_with_ctx_state.
                    // We wrap the result: if expression is false, override display.
                    return format!(
                        r#"{{ let __node = {inner}; if !({expr}) {{ if let velox_dom::VNode::Element {{ ref mut props, .. }} = __node {{ props.attrs.entry("style".to_string()).and_modify(|s| {{ if s.contains("display:") {{ /* preserve existing display */ }} else {{ s.push_str("; display: none"); }} }}).or_insert_with(|| "display: none".to_string()); }} }} __node }}"#,
                        inner = inner,
                        expr = expr.trim()
                    );
                }
                return format!(
                    r#"{{ let __node = {inner}; if !({expr}) {{ if let velox_dom::VNode::Element {{ ref mut props, .. }} = __node {{ props.attrs.insert("style".to_string(), "display: none".to_string()); }} }} __node }}"#,
                    inner = inner,
                    expr = expr.trim()
                );
            }

            // v-if inside the loop body
            if let Some(pos) = attrs
                .iter()
                .position(|a| matches!(a.kind, AttrKind::Directive) && a.name == "if")
            {
                let mut attrs2 = attrs.clone();
                let dir = attrs2.remove(pos);
                let expr_if = rewrite_if_expr(&dir.value.unwrap_or_default());
                let tmp = Node::Element {
                    tag: tag.clone(),
                    attrs: attrs2,
                    children: children.clone(),
                    self_closing: false,
                };
                let inner = emit_node_with_ctx_state(&tmp, item_name, idx_name, scope_id);
                return format!(
                    r#"if {} {{ {} }} else {{ text("") }}"#,
                    expr_if.trim(),
                    inner
                );
            }

            // Component inside the loop body: props must be built from the loop
            // variables directly (resolve() returns "" for loop vars).
            if let Some(component_attr) = attrs
                .iter()
                .find(|a| a.name == "data-velox-component" && a.kind == AttrKind::Static)
                && let Some(comp_name) = &component_attr.value
            {
                let mut clean_attrs = attrs.clone();
                clean_attrs.retain(|a| a.name != "data-velox-component");

                let has_slot_children = !children.is_empty();

                let (_has_props, props_expr) =
                    generate_component_props_expr_with_ctx(&clean_attrs, item_name, idx_name);
                let callbacks = collect_component_callbacks(&clean_attrs);

                // Build slots map from default slot children.
                let slots_expr = if has_slot_children {
                    let slot_vnodes: Vec<String> = children
                        .iter()
                        .map(|c| emit_node_with_ctx_state(c, item_name, idx_name, scope_id))
                        .collect();
                    format!(
                        r#"std::collections::HashMap::from([(\"default\", {})])"#,
                        if slot_vnodes.len() == 1 {
                            slot_vnodes[0].clone()
                        } else {
                            format!("velox_dom::h(\"slot\", velox_dom::Props::new(), vec![{}])", slot_vnodes.join(", "))
                        }
                    )
                } else {
                    "std::collections::HashMap::new()".to_string()
                };

                if callbacks.is_empty() {
                    if !has_slot_children {
                        return format!(
                            r#"{{ let __props = {props_expr}; {comp_name}::render_with_props(__props) }}"#,
                        );
                    }
                    return format!(
                        r#"{{ let __props = {props_expr}; let __slots = {slots_expr}; {comp_name}::render_with_slots(__props, &__slots) }}"#,
                    );
                }

                // No slot children but has callbacks: use render_with_callbacks
                if !has_slot_children {
                    let callback_map = format_callback_map(&callbacks);
                    let callback_names: Vec<String> = callbacks
                        .iter()
                        .map(|(_, handler)| handler.clone())
                        .collect();

                    return format!(
                        r#"{{ let __props = {props_expr}; let __callbacks = {callback_map}; {comp_name}::render_with_callbacks(__props, &__callbacks, &[{callback_names}]) }}"#,
                        callback_names = callback_names
                            .iter()
                            .map(|n| format!("\"{}\"", n))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }

                // Has both callbacks and slot children: use render_with_slots
                let callback_map = format_callback_map(&callbacks);
                let callback_names: Vec<String> = callbacks
                    .iter()
                    .map(|(_, handler)| handler.clone())
                    .collect();

                return format!(
                    r#"{{ let __props = {props_expr}; let __callbacks = {callback_map}; let __slots = {slots_expr}; {comp_name}::render_with_slots(__props, &__callbacks, &[{callback_names}], &__slots) }}"#,
                    callback_names = callback_names
                        .iter()
                        .map(|n| format!("\"{}\"", n))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }

            // Plain element inside the loop body.
            let props = emit_props_with_ctx(attrs, item_name, idx_name);
            let mut k_items: Vec<String> = Vec::new();
            for c in children {
                k_items.push(emit_node_with_ctx_state(c, item_name, idx_name, scope_id));
            }
            let kids = format!("vec![{}]", k_items.join(", "));
            format!(r#"h("{}", {}, {kids})"#, tag, append_scope_attr(&props, scope_id))
        }
    }
}

#[allow(dead_code)]
fn emit_node_with_ctx(n: &Node, loop_var: Option<&str>, scope_id: Option<&str>) -> String {
    match n {
        Node::Text(t) => format!(r#"text({})"#, string_lit(t)),
        Node::Interpolation(expr) => {
            let key = expr.trim();
            if let Some(var) = loop_var {
                if key == var {
                    return "text(__i.to_string())".to_string();
                }
                // Handle dot notation: item.property or item.nested.property
                if key.starts_with(var)
                    && key.len() > var.len()
                    && key.as_bytes()[var.len()] == b'.'
                {
                    let prop_path = &key[var.len()..];
                    return format!(
                        r#"text({{ let __obj = __i; __obj{}.to_string() }})"#,
                        prop_path
                    );
                }
            }
            let key_lit = string_lit(key);
            format!(r#"text(resolve({}))"#, key_lit)
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
                    k_items.push(emit_node_with_ctx(c, loop_var, scope_id));
                }
                format!("vec![{}]", k_items.join(", "))
            };
            format!(r#"h("{}", {}, {kids})"#, tag, append_scope_attr(&props, scope_id))
        }
    }
}

/// Emit a node with context for the render() path inside a v-for loop.
/// This version uses resolve() with indexed access for collection items.
/// For `item in items`, it generates code like `resolve("items[__i].property")`
/// or `resolve("items[__i]")` for the item itself.
fn emit_node_with_ctx_for_loop(n: &Node, for_info: &VForInfo, scope_id: Option<&str>) -> String {
    match n {
        Node::Text(t) => format!(r#"text({})"#, string_lit(t)),
        Node::Interpolation(expr) => {
            let key = expr.trim();
            if key == for_info.item_name {
                // Direct item reference: use indexed access via resolve
                return format!(
                    r#"text(resolve(&format!("{}[{{}}]", {})))"#,
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
                    r#"text(resolve(&format!("{expr}[{{}}]{prop_path}", {idx})))"#,
                    expr = for_info.expr,
                    idx = for_info.index_name,
                );
            }
            if key == for_info.index_name {
                return format!(r#"text({}.to_string())"#, for_info.index_name);
            }
            let key_lit = string_lit(key);
            format!(r#"text(resolve({}))"#, key_lit)
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
                    k_items.push(emit_node_with_ctx_for_loop(c, for_info, scope_id));
                }
                format!("vec![{}]", k_items.join(", "))
            };
            format!(r#"h("{}", {}, {kids})"#, tag, append_scope_attr(&props, scope_id))
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

/// Collect all v-model expressions from a template AST.
/// Returns a list of (model_expr, handler_name) pairs.
/// For `v-model="counter"`, returns `("counter", "__vmodel_set_counter")`.
pub fn collect_vmodel_expressions(nodes: &[Node]) -> Vec<(String, String)> {
    let mut results = Vec::new();
    fn walk(nodes: &[Node], out: &mut Vec<(String, String)>) {
        for node in nodes {
            if let Node::Element {
                attrs, children, ..
            } = node
            {
                for attr in attrs {
                    if matches!(attr.kind, AttrKind::Directive)
                        && attr.name == "model"
                        && let Some(expr) = &attr.value
                    {
                        let handler = format!("__vmodel_set_{}", expr.replace('.', "_"));
                        out.push((expr.clone(), handler));
                    }
                }
                walk(children, out);
            }
        }
    }
    walk(nodes, &mut results);
    results
}

/// Collect all interpolation key names from the template AST.
/// Returns unique keys like `["text", "completed", "counter"]` that are used
/// in `{{ key }}` expressions. These correspond to method names on the
/// component's State struct.
pub fn collect_interpolation_keys(nodes: &[Node]) -> Vec<String> {
    let mut keys = Vec::new();
    fn walk(nodes: &[Node], out: &mut Vec<String>) {
        for node in nodes {
            match node {
                Node::Interpolation(expr) => {
                    let key = expr.trim().to_string();
                    if !key.is_empty() && !out.contains(&key) {
                        out.push(key);
                    }
                }
                Node::Element { children, .. } => {
                    walk(children, out);
                }
                _ => {}
            }
        }
    }
    walk(nodes, &mut keys);
    keys
}

/// Generate setter methods on the State struct for v-model fields.
/// For `v-model="counter"` with State containing `counter: Cell<i32>`,
/// generates:
/// ```ignore
/// pub fn __vmodel_set_counter(&self, payload: &str) {
///     velox_core::vmodel::VModel::vmodel_set(&self.counter, payload);
/// }
/// ```
pub fn generate_vmodel_setters(vmodels: &[(String, String)]) -> String {
    if vmodels.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for (expr, handler) in vmodels {
        let field_path = format!("self.{}", expr);
        out.push_str(&format!(
            r#"
    pub fn {handler}(&self, payload: &str) {{
        velox_core::vmodel::VModel::vmodel_set(&{field_path}, payload);
    }}"#,
            handler = handler,
            field_path = field_path,
        ));
    }
    out
}
