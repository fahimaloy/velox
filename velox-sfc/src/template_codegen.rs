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

/// The resolver key a `:key` attribute contributes in the Resolve-mode `v-for`
/// body.
///
/// Nothing has rewritten the attribute value before it reaches the emit site, so
/// it is normalized here exactly once: surrounding whitespace is dropped and the
/// documented `{{ … }}` spelling is unwrapped to the bare expression it contains.
/// The result is emitted as a single `resolve("…")` lookup — the same shape the
/// sibling `v-for` branch uses for its collection (`let __for_expr =
/// resolve("items");`) — so the expression is interpolated exactly once and the
/// generated code compiles.
fn resolve_key_expr(key_val: &str) -> String {
    let trimmed = key_val.trim();
    let unwrapped = trimmed
        .strip_prefix("{{")
        .and_then(|rest| rest.strip_suffix("}}"))
        .map(str::trim)
        .filter(|expr| !expr.is_empty());
    unwrapped.unwrap_or(trimmed).to_string()
}

/// The Resolve-mode read of an expression rooted at a `v-for` loop variable.
///
/// The Resolve-mode loop body iterates an index over the collection string, so
/// the loop item is *not* a Rust binding there: it is read the same way the same
/// body already reads an interpolated `{ todo.text }` — an indexed resolver
/// lookup. The loop index is a real binding and is read directly.
///
/// Returns `None` for anything this cannot read (a compound expression such as
/// `todo.a + todo.b`), so the caller keeps the plain resolver lookup and the
/// diagnostic still reports the binding rather than leaving it silently empty.
fn resolve_loop_item_expr(expr: &str, for_info: &VForInfo) -> Option<String> {
    let expr = expr.trim();
    if expr == for_info.index_name {
        // The loop index *is* a real binding in this body.
        return Some(for_info.index_name.clone());
    }
    if expr == for_info.item_name {
        return Some(format!(
            r#"resolve(&format!("{}[{{}}]", {idx}))"#,
            for_info.expr,
            idx = for_info.index_name
        ));
    }
    if let Some(path) = expr.strip_prefix(&for_info.item_name) {
        if let Some(path) = path.strip_prefix('.') {
            if is_field_path(path) {
                return Some(format!(
                    r#"resolve(&format!("{}[{{}}].{path}", {idx}))"#,
                    for_info.expr,
                    idx = for_info.index_name
                ));
            }
        }
    }
    None
}

/// Whether `s` is a plain field path — dot-separated Rust identifiers, and
/// nothing else. It keeps a compound expression such as `todo.a + todo.b` out of
/// the indexed-resolver rewrite, where only the field path is rewritable.
fn is_field_path(s: &str) -> bool {
    !s.is_empty()
        && s.split('.').all(|seg| {
            let mut chars = seg.chars();
            matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
                && chars.all(is_ident_char)
        })
}

/// The `String` value a Resolve-mode `v-for` element's `:key` is set from.
///
/// `make_resolve` is built once, outside every loop, so a key rooted at the loop
/// variable has no arm to answer it there. It is read the way the same loop body
/// reads an interpolated loop value instead; a key the loop cannot read that way
/// keeps the resolver lookup, which the diagnostic reports.
fn resolve_mode_key_value(key_val: &str, for_info: &VForInfo) -> String {
    let authored = resolve_key_expr(key_val);
    match resolve_loop_item_expr(&authored, for_info) {
        Some(indexed) => format!("{indexed}.to_string()"),
        None => format!("resolve({}).to_string()", string_lit(&authored)),
    }
}

/// The bool a resolver-backed condition evaluates to, using the same string
/// truthiness a non-loop condition gets from `rewrite_if_expr`.
///
/// A loop item read in the Resolve renderer is a `String` from the resolver, not
/// a `bool`, so it needs this test in an `if` position exactly as a resolver-backed
/// condition does.
fn resolver_truthiness(read: &str) -> String {
    format!(r#"{{ let __v = {read}; __v == "true" || (!__v.is_empty() && __v != "false") }}"#)
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

/// Which renderer a compiled component is expected to be rendered through.
///
/// A generated module always exposes both entry points, so this is not a
/// codegen switch: it records which one the consumer calls, which decides
/// whether an unresolvable template binding is worth reporting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderMode {
    /// The component is rendered through `render_with_state`, which reads
    /// `Signal`/`Ref` fields and `v-for` loop items directly from the persistent
    /// `State`. This is what `velox-cli` and every generated `main.rs` use.
    #[default]
    State,
    /// The component is rendered through `render_with`/`render_with_props`,
    /// which can only see strings a resolver closure supplies. A binding rooted
    /// at a `v-for` loop item cannot resolve there, so it is reported.
    Resolve,
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
/// The component is assumed to be rendered in [`RenderMode::State`]; use
/// [`compile_template_to_rs_full_with_mode`] for a resolver-only consumer.
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
    component_name: &str,
    resolver: Option<&mut ComponentResolver>,
    script_setup: Option<&str>,
    scope_id: Option<&str>,
) -> Result<String, String> {
    compile_template_to_rs_full_with_mode(
        template_src,
        component_name,
        resolver,
        script_setup,
        scope_id,
        RenderMode::State,
    )
}

/// [`compile_template_to_rs_full`] with the renderer's mode, which decides
/// whether a binding the resolver cannot satisfy is reported: a `v-for`
/// loop-rooted binding renders correctly in [`RenderMode::State`] and is only
/// reported for [`RenderMode::Resolve`]. The generated module is identical in
/// both modes — only the diagnostics differ.
pub fn compile_template_to_rs_full_with_mode(
    template_src: &str,
    _component_name: &str,
    mut resolver: Option<&mut ComponentResolver>,
    script_setup: Option<&str>,
    scope_id: Option<&str>,
    mode: RenderMode,
) -> Result<String, String> {
    let nodes = crate::template_parse::parse_template_to_ast(template_src)?;

    // Validate template structure before codegen.
    let validation_errors = validate_template(&nodes);
    if !validation_errors.is_empty() {
        return Err(validation_errors.join("\n"));
    }

    // Surface `unknown component` warnings here: pure template parsing is
    // component-agnostic, but this is the point where the SFC's registered
    // component names are known — via the resolver (imports parsed from
    // `<script setup>` by the caller) and/or the raw `<script setup>` block.
    // Must run before `transform_components` renames known component tags.
    {
        let mut known: Vec<String> = Vec::new();
        if let Some(resolver) = &resolver {
            known.extend(resolver.component_names());
        }
        if let Some(script_setup) = script_setup {
            let mut scratch = ComponentResolver::new(std::path::PathBuf::from("."));
            scratch.parse_imports(script_setup);
            for name in scratch.component_names() {
                if !known.contains(&name) {
                    known.push(name);
                }
            }
        }
        let known_refs: Vec<&str> = known.iter().map(String::as_str).collect();
        for warning in
            crate::template_parse::unknown_component_warnings(&nodes, &known_refs, template_src)
        {
            eprintln!("velox: warning: {warning}");
        }
    }

    // For MVP, assume a single root node.
    let mut nodes = nodes;

    // Index the script block so codegen can resolve template keys to the real
    // State method names and match component tags to persistent state fields.
    let methods = script_setup.map(extract_state_methods).unwrap_or_default();
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

    // make_resolve: maps every key the render path looks up through `resolve()`
    // (interpolations and bound-attribute expressions) to State getters so
    // main.rs does not hand-write a resolve closure. Supports both bare
    // (`title()`) and prefixed (`get_title()`) getter conventions.
    let resolver_keys = collect_resolver_keys(&nodes, &methods, &fields, mode);
    for warning in &resolver_keys.warnings {
        eprintln!("velox: warning: {warning}");
    }
    out.push_str("\n\n");
    out.push_str(&generate_make_resolve(&resolver_keys.keys, &methods));

    // make_on_event: dispatch every template handler (plus, for the root, every
    // handler anywhere in the component tree) to the owning State method.
    let handlers = collect_handlers(&nodes);
    out.push_str("\n\n");
    out.push_str(&generate_make_on_event(&handlers, &tree_handlers, &methods));

    // render_with_props: for presentational child components. Props take
    // priority; interpolations fall back to State getters.
    out.push_str("\n\n");
    out.push_str(&generate_render_with_props(&resolver_keys.keys, &methods));

    Ok(out)
}

fn collect_handlers(nodes: &[Node]) -> Vec<String> {
    let mut set: HashSet<String> = HashSet::new();
    // The `v-for` loop variables in scope, outermost first. A `v-model` rooted
    // at one of them has no setter to dispatch to — see
    // `vmodel_is_loop_rooted` — and `extract_vmodel` emits no handler for it, so
    // neither can the dispatcher.
    fn walk(n: &Node, set: &mut HashSet<String>, loop_vars: &mut Vec<(String, String)>) {
        if let Node::Element {
            attrs, children, ..
        } = n
        {
            if let Some(info) = v_for_scope(attrs) {
                loop_vars.push((info.item_name.clone(), info.index_name.clone()));
            }
            let (item_name, idx_name) = loop_vars.last().map_or((None, None), |(item, idx)| {
                (Some(item.as_str()), Some(idx.as_str()))
            });
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
                    && !vmodel_is_loop_rooted(expr, item_name, idx_name)
                {
                    set.insert(vmodel_setter_name(expr));
                }
            }
            for c in children {
                walk(c, set, loop_vars);
            }
            if v_for_scope(attrs).is_some() {
                loop_vars.pop();
            }
        }
    }
    let mut loop_vars: Vec<(String, String)> = Vec::new();
    for n in nodes {
        walk(n, &mut set, &mut loop_vars);
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
    #[allow(unused_variables, unused_assignments)]
    let mut _uses_payload = false;

    // Known event modifiers — reserved for future modifier codegen (currently inlined)
    let _modifier_handlers: HashMap<&str, &str> = [
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
            _uses_payload = true;
        } else if let Some(eh) = extra_by_name.get(h.as_str())
            && let Some(owner) = eh.owner.as_deref()
        {
            // Handler owned by a persistent child component state field.
            if eh.takes_payload {
                arms.push_str(&format!(
                    "        \"{name}\" => {{ if let Some(p) = payload {{ state.{owner}.{method}(p); }} }},\n",
                    name = h, owner = owner, method = eh.name
                ));
                _uses_payload = true;
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
                _uses_payload = true;
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
            _uses_payload = true;
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
fn generate_make_resolve(interp_keys: &[String], methods: &[StateMethod]) -> String {
    let mut arms = String::new();
    for key in interp_keys {
        let method = resolve_getter_call(methods, key);
        arms.push_str(&format!(
            "        \"{}\" => state.{}.to_string(),\n",
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

/// Why a key cannot be answered by a resolver arm, in the vocabulary the
/// `v-for` loop diagnostic uses.
///
/// One predicate for every diagnostic that asks "can this key be looked up?",
/// so the bound-attribute path and the `v-model` value path cannot drift into
/// describing the same mistake differently.
fn unresolvable_key_reason(key: &str, methods: &[StateMethod], fields: &[String]) -> String {
    // A member path is answered by a chain rather than by a getter of that name,
    // so what is wrong with it is whatever is wrong with its ROOT. Anything else —
    // a call expression, an index into a collection — has no root to name.
    if let Some((root, _)) = member_path(key) {
        return unanswerable_root_reason(root, true, methods, fields);
    }
    let named = methods.iter().find(|m| &m.name == key);
    match named {
        Some(m) if m.takes_payload => {
            format!("`{key}` is a payload-taking State method, not a getter")
        }
        Some(m) if m.return_type.is_none() => {
            format!("`{key}` declares no return type, so there is no text to render")
        }
        Some(m) => format!(
            "`{key}` returns `{}`, which cannot be rendered as text",
            m.return_type.as_deref().unwrap_or_default()
        ),
        None if fields.iter().any(|f| f == key) => format!(
            "`{key}` is a `State` field, not a getter — reading a field in a \
             template needs a typed field resolver, which is not implemented yet"
        ),
        None if !is_bare_identifier(key) => {
            format!("`{key}` is an expression, not a `State` getter name")
        }
        None => format!("`{key}` is not a zero-argument `State` getter"),
    }
}

/// The `v-for` write a `v-model` on a loop item cannot make, reported in State
/// mode.
///
/// The read is real — the input shows the loop item's own field — but the write
/// has no route: `on:input` carries a handler *name*, and `make_on_event`
/// resolves it against `State`, which has no handle on the loop item. So no
/// setter is generated and no dispatcher arm is emitted; an arm naming one would
/// be `state.__vmodel_set_todo_text(p)` against a method that cannot exist, and
/// the setter body would be `vmodel_set(&self.todo.text, …)` on a field the
/// `State` does not have. Both are E0609, not a working binding. The same
/// family and the same remedy sentence as [`resolve_loop_warning`].
fn vmodel_loop_write_warning(expr: &str) -> String {
    format!(
        "v-model=\"{expr}\" on a `v-for` item cannot be written — the event dispatcher resolves \
         `on:input` handler names against `State`, which has no handle on a loop item, so the \
         input renders the item's value but typing does not change it. Write the field from a \
         real handler on the item, or bind `:value` plus an `@input` that reaches it."
    )
}

/// The write half a `v-model` does not have in Resolve mode.
///
/// Resolve mode renders through `render_with(resolve)`, which has no `State` at
/// all: the generated `make_on_event` takes one, so nothing dispatches the write
/// there. R-1's ruling is that Resolve mode reports rather than grows a write
/// path, and this is that report — the same family and remedy as
/// [`resolve_loop_warning`], and deliberately a separate function from it,
/// because a `v-model` is a different condition from a loop that cannot render.
fn vmodel_resolve_mode_warning(expr: &str) -> String {
    format!(
        "v-model=\"{expr}\" cannot be written in Resolve mode — the resolver renders values but \
         dispatches no events, so only the read side exists here. Render this component through \
         `render_with_state`, which reads the value and writes it from the State directly."
    )
}

/// Generate `render_with_props` — props take priority, interpolations fall back
/// to State getters so a presentational child renders correctly even when the
/// parent omits a prop.
fn generate_render_with_props(interp_keys: &[String], methods: &[StateMethod]) -> String {
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
        let method = resolve_getter_call(methods, key);
        match_arms.push_str(&format!(
            "            \"{}\" => state.{}.to_string(),\n",
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
                if is_ident_char(next) {
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
                if is_ident_char(next) {
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
/// The `v-for` binding an element declares, or `None`.
///
/// The scope a `v-model` expression is resolved against: the two loop variables
/// in scope when the element is inside a `v-for` body.
fn v_for_scope<'a>(attrs: &'a [TemplateAttr]) -> Option<VForInfo> {
    attrs
        .iter()
        .find(|a| matches!(a.kind, AttrKind::Directive) && a.name == "for")
        .and_then(|a| a.value.as_deref())
        .and_then(parse_v_for)
}

/// Whether a `v-model` expression is rooted at a `v-for` loop variable.
///
/// A loop-rooted `v-model` is a read the loop item can answer — `todo.text` is a
/// real field read in State mode — but its write has nowhere to go: the event
/// dispatcher resolves a handler name against `State`, which has no handle on a
/// loop item. `extract_vmodel` therefore emits the read and no handler, and
/// `collect_resolver_keys` reports the missing write instead of emitting an arm
/// that cannot compile. The predicate is [`expr_reads_root`] over the loop
/// variables in scope — the same test the read path uses to decide it can
/// answer the expression.
fn vmodel_is_loop_rooted(expr: &str, item_name: Option<&str>, idx_name: Option<&str>) -> bool {
    let roots: Vec<&str> = [item_name, idx_name].into_iter().flatten().collect();
    !roots.is_empty() && expr_reads_roots(expr, &roots)
}

/// The generated setter name for a `v-model` expression.
///
/// ONE spelling, produced by every side of the write path so the dispatcher arm
/// and the `State` method it calls can never disagree: the template emitter
/// (`extract_vmodel`) and the handler collector (`collect_handlers`) both build
/// it here, and the setter generator is fed by [`collect_vmodel_expressions`],
/// which uses the same rule. A `.` becomes `_` because the name has to be a Rust
/// method name, so `v-model="form.name"` maps to `__vmodel_set_form_name` and the
/// setter it pairs with writes `self.form.name`.
fn vmodel_setter_name(expr: &str) -> String {
    format!("__vmodel_set_{}", expr.replace('.', "_"))
}

/// Desugar `v-model="expr"` into a `:value` bind plus an `@input` handler, and
/// report the expression.
///
/// `item_name`/`idx_name` are the `v-for` loop variables in scope. A loop-rooted
/// expression gets the value bind only: its read is a direct field read of the
/// loop item — the shape State mode already emits for a loop-rooted binding — and
/// there is no handler to name, because a write to a loop item cannot be routed
/// through the `State`-rooted dispatcher. `collect_resolver_keys` reports that
/// case; see [`vmodel_is_loop_rooted`].
fn extract_vmodel(
    attrs: &[TemplateAttr],
    item_name: Option<&str>,
    idx_name: Option<&str>,
) -> (Vec<TemplateAttr>, Option<String>) {
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

        // Generate @input handler that calls state.__vmodel_set_{field}(payload).
        // A loop-rooted expression has no such setter: its write target is the
        // loop item, which the dispatcher cannot reach, so the input is emitted
        // as a read-only value and the missing write is reported instead.
        if !vmodel_is_loop_rooted(&model_expr, item_name, idx_name) {
            remaining.push(TemplateAttr {
                name: "input".to_string(),
                value: Some(vmodel_setter_name(&model_expr)),
                kind: AttrKind::On,
            });
        }

        (remaining, Some(model_expr))
    } else {
        (attrs.to_vec(), None)
    }
}

fn emit_node_with_mode(
    n: &Node,
    mode: TransformMode,
    fields: &[String],
    scope_id: Option<&str>,
) -> String {
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
            let (attrs2, _v_model_handler) = extract_vmodel(attrs, None, None);
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
                let has_style = attrs.iter().any(|a| {
                    a.name == "style" && matches!(a.kind, AttrKind::Static | AttrKind::Bind)
                });

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
                if mode == TransformMode::State
                    && !has_props_events
                    && !has_slot_children
                    && fields.contains(&field)
                {
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
                            format!(
                                "velox_dom::h(\"slot\", velox_dom::Props::new(), vec![{}])",
                                slot_vnodes.join(", ")
                            )
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
                                    "{{ let mut __node = {}; if let velox_dom::VNode::Element {{ ref mut props, .. }} = __node {{ props.attrs.insert(\"key\".to_string(), {}); }} __node }}",
                                    inner,
                                    resolve_mode_key_value(key_val, &for_info)
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
            format!(
                r#"h("{}", {}, {kids})"#,
                tag,
                append_scope_attr(&props, scope_id)
            )
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
                let value = bind_attr_value(&a.name, &expr, None, None);
                bind_entries.push(format!("({key}, {value})"));
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
                let value = bind_attr_value(&a.name, &expr, item_name, idx_name);
                bind_entries.push(format!("({key}, {value})"));
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
    emit_props_in_loop(attrs, item_name, idx_name, None)
}

/// The props chain for an element that is not inside a `v-for` body.
fn emit_props_with(attrs: &[TemplateAttr]) -> String {
    emit_props_in_loop(attrs, None, None, None)
}

/// Build the props chain for an element.
///
/// `resolve_loop` is the `v-for` body the element sits in, and is set only when
/// the Resolve renderer is being generated: there the loop item is an indexed
/// resolver lookup, not a field read, so a loop-rooted `:bind` is normalized
/// accordingly. It is `None` for the State renderer, where the loop item is a
/// real binding and is read directly.
fn emit_props_in_loop(
    attrs: &[TemplateAttr],
    item_name: Option<&str>,
    idx_name: Option<&str>,
    resolve_loop: Option<&VForInfo>,
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
                parts.push(bind_prop_entry(
                    key,
                    &a.name,
                    &expr,
                    item_name,
                    idx_name,
                    resolve_loop,
                ));
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

fn emit_children_with_mode(
    children: &[Node],
    mode: TransformMode,
    fields: &[String],
    scope_id: Option<&str>,
) -> String {
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

                                let inner =
                                    emit_node_with_ctx_for_loop(&tmp_elem, &for_info, scope_id);

                                let inner_with_key = if let Some(Some(key_val)) = &key_expr {
                                    format!(
                                        "{{ let mut __node = {}; if let velox_dom::VNode::Element {{ ref mut props, .. }} = __node {{ props.attrs.insert(\"key\".to_string(), {}); }} __node }}",
                                        inner,
                                        resolve_mode_key_value(key_val, &for_info)
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

fn emit_node_with_ctx_state(
    n: &Node,
    item_name: Option<&str>,
    idx_name: Option<&str>,
    scope_id: Option<&str>,
) -> String {
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
            // Desugar `v-model` here, where the loop variables are in scope:
            // this is the only loop-body emitter on the State-mode path, and
            // `emit_props_with_ctx` drops every directive, so a `v-model` that
            // reached it would lose both its value bind and its handler. The
            // read is a direct field read of the loop item, the shape State mode
            // already emits for a loop-rooted binding; a loop-rooted expression
            // gets no handler, and `collect_resolver_keys` reports the write it
            // cannot route.
            let (attrs2, _v_model_handler) = extract_vmodel(attrs, item_name, idx_name);
            let attrs = &attrs2;
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
                let has_style = attrs.iter().any(|a| {
                    a.name == "style" && matches!(a.kind, AttrKind::Static | AttrKind::Bind)
                });

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
                            format!(
                                "velox_dom::h(\"slot\", velox_dom::Props::new(), vec![{}])",
                                slot_vnodes.join(", ")
                            )
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
            format!(
                r#"h("{}", {}, {kids})"#,
                tag,
                append_scope_attr(&props, scope_id)
            )
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
            format!(
                r#"h("{}", {}, {kids})"#,
                tag,
                append_scope_attr(&props, scope_id)
            )
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
            // Inside a Resolve-mode loop body the loop item is an indexed
            // resolver read, so a loop-rooted binding is normalized as such.
            let props = emit_props_in_loop(
                attrs,
                Some(&for_info.item_name),
                Some(&for_info.index_name),
                Some(for_info),
            );
            let kids = {
                let mut k_items: Vec<String> = Vec::new();
                for c in children {
                    k_items.push(emit_node_with_ctx_for_loop(c, for_info, scope_id));
                }
                format!("vec![{}]", k_items.join(", "))
            };
            format!(
                r#"h("{}", {}, {kids})"#,
                tag,
                append_scope_attr(&props, scope_id)
            )
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
    // The `v-for` loop variables in scope, outermost first: a loop-rooted
    // `v-model` gets no setter, because the dispatcher cannot reach a loop item
    // (see `vmodel_is_loop_rooted`).
    fn walk(
        nodes: &[Node],
        out: &mut Vec<(String, String)>,
        loop_vars: &mut Vec<(String, String)>,
    ) {
        for node in nodes {
            if let Node::Element {
                attrs, children, ..
            } = node
            {
                if let Some(info) = v_for_scope(attrs) {
                    loop_vars.push((info.item_name.clone(), info.index_name.clone()));
                }
                let (item_name, idx_name) = loop_vars.last().map_or((None, None), |(item, idx)| {
                    (Some(item.as_str()), Some(idx.as_str()))
                });
                for attr in attrs {
                    if matches!(attr.kind, AttrKind::Directive)
                        && attr.name == "model"
                        && let Some(expr) = &attr.value
                        && !vmodel_is_loop_rooted(expr, item_name, idx_name)
                    {
                        let handler = vmodel_setter_name(expr);
                        out.push((expr.clone(), handler));
                    }
                }
                walk(children, out, loop_vars);
                if v_for_scope(attrs).is_some() {
                    loop_vars.pop();
                }
            }
        }
    }
    let mut loop_vars: Vec<(String, String)> = Vec::new();
    walk(nodes, &mut results, &mut loop_vars);
    results
}

/// Collect all interpolation key names from the template AST.
/// Returns unique keys like `["text", "completed", "counter"]` that are used
/// in `{{ key }}` expressions. These correspond to method names on the
/// component's State struct.
///
/// An interpolation inside a `v-for` body that names a loop variable is NOT a
/// key: both renderers read the loop item and index directly, from the binding
/// the loop itself introduces, so it is a real read rather than a
/// `resolve(...)` lookup. Registering one would emit an arm against the
/// `State` — `"item.name" => state.item.name().to_string()` — and the loop
/// item is not a `State` field, so that arm does not compile (E0609). This is
/// the same rule the bind path applies, reached there through `emit_bind_attr`.
pub fn collect_interpolation_keys(nodes: &[Node]) -> Vec<String> {
    let mut keys = Vec::new();
    // Every loop variable in scope, outermost first. A nested `v-for` that
    // rebinds the same names shadows them, but either way an expression naming
    // one of them is read by the loop, so every enclosing scope is consulted.
    let mut loop_vars: Vec<(String, String)> = Vec::new();
    fn walk(nodes: &[Node], out: &mut Vec<String>, loop_vars: &mut Vec<(String, String)>) {
        for node in nodes {
            match node {
                Node::Interpolation(expr) => {
                    let key = expr.trim().to_string();
                    if key.is_empty() || out.contains(&key) {
                        continue;
                    }
                    let roots: Vec<&str> = loop_vars
                        .iter()
                        .flat_map(|(item, index)| [item.as_str(), index.as_str()])
                        .collect();
                    if expr_reads_roots(&key, &roots) {
                        continue;
                    }
                    out.push(key);
                }
                Node::Element {
                    attrs, children, ..
                } => {
                    let scope = attrs
                        .iter()
                        .find(|a| matches!(a.kind, AttrKind::Directive) && a.name == "for")
                        .and_then(|a| a.value.as_deref())
                        .and_then(parse_v_for)
                        .map(|info| (info.item_name, info.index_name));
                    if let Some(pair) = &scope {
                        loop_vars.push(pair.clone());
                    }
                    walk(children, out, loop_vars);
                    if scope.is_some() {
                        loop_vars.pop();
                    }
                }
                _ => {}
            }
        }
    }
    walk(nodes, &mut keys, &mut loop_vars);
    keys
}

/// Is `expr` a bare identifier (no field access, call, or literal)?
fn is_bare_identifier(expr: &str) -> bool {
    let mut chars = expr.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(is_ident_char)
}
/// The `State` method that provides the value for `key`, when that method is a
/// genuine zero-argument getter whose return type can be rendered as text.
/// Payload-taking methods are event handlers, not getters: `state.on_input()`
/// does not compile, so their names must never be registered as resolver keys.
/// Neither can a method that returns nothing (`state.reset()` has no `Display`
/// form), so the return type is checked as well.
fn getter_method_name<'a>(methods: &'a [StateMethod], key: &'a str) -> Option<&'a str> {
    find_state_method(methods, key, |m| {
        !m.takes_payload && return_type_is_renderable(m.return_type.as_deref())
    })
}

/// A zero-argument `State` method that yields a value, whatever its type.
///
/// [`getter_method_name`] additionally requires that value to be renderable as
/// text, which is right when the arm renders the value itself
/// (`"title" => state.title().to_string()`) and wrong for the ROOT of a member
/// path: `{{ user.name }}` needs `state.user()` to return the user's own type,
/// and the `String` that renders is the last segment's. This is the same lookup
/// with the renderability test lifted, not a second notion of "answerable" —
/// both call sites go through [`find_state_method`], so the `get_`/`is_`/`has_`
/// convention and the payload test cannot drift between them.
fn accessor_method_name<'a>(methods: &'a [StateMethod], key: &'a str) -> Option<&'a str> {
    find_state_method(methods, key, |m| {
        !m.takes_payload && returns_a_value(m.return_type.as_deref())
    })
}

/// Does the method yield something a template can go on to read?
///
/// Deliberately permissive: a user struct is a perfectly good start of a member
/// path even though it has no `Display` form of its own, and the inner segments
/// decide what the chain ends up producing. A unit return is the one case that
/// cannot start anything.
fn returns_a_value(ty: Option<&str>) -> bool {
    match ty.map(str::trim) {
        Some(ty) => !matches!(strip_borrow(ty), "()" | "Self" | ""),
        None => false,
    }
}

/// The `State` method `key` names: the exact name first, then the `get_`/`is_`/
/// `has_` variants, keeping the first that satisfies `usable`.
fn find_state_method<'a>(
    methods: &'a [StateMethod],
    key: &str,
    usable: impl Fn(&StateMethod) -> bool,
) -> Option<&'a str> {
    if let Some(m) = methods.iter().find(|m| m.name == key && usable(m)) {
        return Some(m.name.as_str());
    }
    ["get_", "is_", "has_"].iter().find_map(|prefix| {
        let candidate = format!("{prefix}{key}");
        methods
            .iter()
            .find(|m| m.name == candidate && usable(m))
            .map(|m| m.name.as_str())
    })
}

/// Can a method's declared return type be rendered as text?
///
/// A resolver arm calls `state.<getter>().to_string()`, so only types that
/// implement `Display` qualify: the string types, `bool`, `char`, the integer
/// and float primitives (templates already render `:class="{ done: completed }"`
/// from a `bool` getter, and the truthy check matches the resulting `"true"`),
/// and string wrappers. An absent return type means `()`, and collections,
/// `Result`/`Option` and framework handles have no `Display` form, so they are
/// rejected rather than guessed at.
fn return_type_is_renderable(ty: Option<&str>) -> bool {
    let Some(ty) = ty else {
        return false;
    };
    let base = strip_borrow(ty);
    if matches!(
        base,
        "String"
            | "str"
            | "bool"
            | "char"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "f32"
            | "f64"
    ) {
        return true;
    }
    ["Rc<", "Arc<", "Cow<"].iter().any(|wrapper| {
        base.strip_prefix(wrapper)
            .and_then(|inner| inner.strip_suffix('>'))
            .is_some_and(|inner| matches!(strip_borrow(inner.trim()), "String" | "str"))
    })
}

/// Strip a leading borrow and lifetime from a type, e.g. `&'static str` → `str`.
fn strip_borrow(ty: &str) -> &str {
    let mut ty = ty.trim();
    if let Some(rest) = ty.strip_prefix('&') {
        ty = rest.trim_start();
    }
    if let Some(rest) = ty.strip_prefix('\'') {
        // A lifetime: `'static`, `'a`, or the anonymous `'_` of `Cow<'_, str>`.
        ty = match rest.strip_prefix("_,") {
            Some(after) => after.trim_start(),
            None => {
                let end = rest.find(|c: char| !is_ident_char(c)).unwrap_or(rest.len());
                rest[end..].trim_start()
            }
        };
    }
    ty
}

/// Does `State` expose a zero-argument getter for `key`? Only expressions backed
/// by such a getter are registered as resolver keys, so a binding over a plain
/// `Signal`/`Ref` field (which has no getter to call) keeps generating
/// compiling code.
fn has_state_getter(methods: &[StateMethod], key: &str) -> bool {
    getter_method_name(methods, key).is_some()
}

/// The call expression a resolver arm should make on `state` for `key`,
/// parentheses included: `title()`, `is_open()`, `user().name`.
///
/// Both key sources are gated before they get here — bound-attribute keys by
/// [`has_state_getter`], interpolation keys by
/// [`keep_answerable_interpolation_keys`] — so a key that reaches an arm always
/// has a getter to call, spelled `title`, `get_title`, `is_title` or `has_title`
/// by the same [`find_state_method`] convention. The fallbacks below are what a
/// key the gates let through but this call cannot chain would use; they are not
/// a route to an arm for a name `State` does not expose.
///
/// A member path is the exception: it is emitted as a chain of calls on the
/// root accessor (see [`member_path_call`]), because `state.user.name()` is not
/// a legal expression for a hand-written `State` — it needs a `user` *field*
/// holding something with a `name` method, and a template cannot produce that.
fn resolve_getter_call(methods: &[StateMethod], key: &str) -> String {
    if let Some(chain) = member_path_call(methods, key) {
        return chain;
    }
    match getter_method_name(methods, key) {
        Some(method) => format!("{method}()"),
        None => format!("{}()", resolve_method_name(&method_names(methods), key)),
    }
}

/// The call chain a member path resolves to, or `None` when `key` is not one.
///
/// `user.name` → `user().name`, `a.b.c` → `a().b().c`, `items[0].name` →
/// `items()[0].name`: the root is the zero-argument accessor the arm already
/// looks up, and every later segment is a method call on what came before. An
/// index segment is a place rather than a method, so it is emitted as written
/// and the segments after it stay calls.
///
/// The whole chain is rewritten uniformly. Rewriting only the segments codegen
/// can vouch for would leave code whose legality depends on types it cannot
/// see, which is the silent-wrongness this exists to remove.
fn member_path_call(methods: &[StateMethod], key: &str) -> Option<String> {
    let (root, segments) = member_path(key)?;
    let mut chain = format!("{}()", accessor_method_name(methods, root)?);
    for segment in segments {
        if segment.starts_with('[') {
            chain.push_str(segment);
        } else {
            chain.push('.');
            chain.push_str(segment);
            chain.push_str("()");
        }
    }
    Some(chain)
}

/// Split a member path into its root and the segments after it: `user.name` →
/// `("user", ["name"])`, `items[0].name` → `("items", ["[0]", "name"])`.
///
/// `None` for anything that is not a member path, so every caller keeps its
/// existing behaviour: a bare name has nothing to chain, and a segment that is
/// neither a name nor an index is not a path this can speak about.
///
/// A key with a call in it (`user.name()`) is `None` too, and **nothing in this
/// crate emits `state.user(state.name())`** — such a key falls through to the
/// name-only fallback, which is [`keep_answerable_interpolation_keys`]'s caller
/// and is deliberately not gated. That is pre-existing behaviour and out of
/// scope here; see the report.
fn member_path(key: &str) -> Option<(&str, Vec<&str>)> {
    if key.contains('(') || key.contains(')') {
        return None;
    }
    let split = key
        .find(['.', '['])
        .filter(|&i| i > 0 && key[..i].chars().all(is_ident_char))?;
    let root = &key[..split];
    let mut segments = Vec::new();
    // The separator that ended the root is not itself a segment: `user.name`
    // continues with `name`, and `items[0]` continues with the index.
    let rest = key[split..].strip_prefix('.').unwrap_or(&key[split..]);
    for segment in rest.split('.') {
        let is_index = segment.starts_with('[') && segment.ends_with(']');
        let is_name = !segment.is_empty() && segment.chars().all(is_ident_char);
        if !is_index && !is_name {
            return None;
        }
        segments.push(segment);
    }
    if segments.is_empty() {
        return None;
    }
    Some((root, segments))
}

/// The shared identifier predicate: one definition for every site that asks
/// "is this character part of an identifier?".
///
/// ITS CONTRACT, for a caller: a scanner that enters on a *first-character* test
/// and then extends with this predicate must advance the cursor on the entry
/// character before extending. Otherwise the two predicates are coupled by an
/// invariant nobody wrote down — this one being wider than the entry test — and
/// narrowing it to exclude a character the entry test accepts leaves the cursor
/// where it is and the scanner spins forever. `condition_resolver_keys` and
/// `expr_reads_roots` both advance unconditionally for exactly that reason, and
/// `a_scanner_that_enters_on_a_first_char_test_advances_even_when_this_is_narrow`
/// pins it.
///
/// Narrowing it is otherwise a behaviour change at every call site, including the
/// two scanners' tokenisation, and must be measured rather than assumed.
fn is_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

/// The interpolation keys that have a getter, or a root accessor, to answer
/// them — with a warning for each one that has not.
///
/// A **bare name** (`{{ title }}`) is registered only when a zero-argument,
/// renderable `State` method answers it, because the arm is
/// `"title" => state.title().to_string()` and a `State` without `title` cannot
/// compile that. A **member path** (`{{ user.name }}`) is registered only when
/// its ROOT has a zero-argument accessor, because the arm is
/// `state.user().name().to_string()` and without a `user()` there is nothing to
/// call. Both decisions ask the same lookup the emitter asks, so a key that is
/// kept is a key whose arm has something to call.
///
/// A key that is neither a bare name nor a member path — `user.name()`, `a.b[0`
/// — is **not gated** and keeps the long-standing fallback. Those keys are
/// refused by [`member_path`] and are not a name a `State` method can have, so
/// the two rules above have nothing to say about them; gating them is a
/// separate task, and the report discloses them rather than claiming this
/// function closes them.
///
/// This is the same family diagnostic the bound-attribute path already reports,
/// asked of one predicate: the name the emitter will call, which is the ROOT of
/// a member path and the whole key otherwise. The decision reads
/// [`StateMethod`]s only — never the loop families, and never the `v-for`
/// collection — so it cannot silence a loop report.
/// Whether `key` has a resolver arm. This is the one question a key's origin must
/// not change: a bare name needs a getter of that name, and a member path needs
/// an accessor for its root plus a method for every segment after it. A key that
/// is neither — a call expression, say — is left alone, exactly as
/// `keep_answerable_interpolation_keys` leaves it, because an arm for it is the
/// fallback's business.
///
/// `keep_answerable_interpolation_keys` asks this for interpolated text and the
/// `v-model` path asks it for a two-way binding, so `{{ form.name }}` and
/// `v-model="form.name"` are answered or dropped together instead of one of them
/// quietly rendering empty.
fn key_is_answerable(methods: &[StateMethod], key: &str) -> bool {
    if is_bare_identifier(key) {
        getter_method_name(methods, key).is_some()
    } else if member_path(key).is_some() {
        member_path_call(methods, key).is_some()
    } else {
        true
    }
}

/// Why a binding cannot be read: what is wrong with `root`, the first segment of
/// whatever shape the key has. `keep_answerable_interpolation_keys` and
/// `unresolvable_key_reason` both call it, so the interpolation gate and the
/// `v-model` gate name the same cause in the same words — `v-model="form.name"`
/// and `{{ form.name }}` fail or succeed together, and they say so identically.
fn unanswerable_root_reason(
    root: &str,
    is_path: bool,
    methods: &[StateMethod],
    fields: &[String],
) -> String {
    let declared = methods.iter().find(|m| &m.name == root);
    match declared {
        Some(m) if m.takes_payload => {
            format!("`{root}` is a payload-taking State method, not an accessor")
        }
        Some(m) if !returns_a_value(m.return_type.as_deref()) => {
            format!("`{root}` returns nothing, so there is no value to read from")
        }
        Some(_) if is_path => {
            format!("`{root}` is a State method, but a member path needs an accessor")
        }
        Some(_) => {
            format!("`{root}` is a `State` method, but its return type cannot be rendered as text")
        }
        None if fields.iter().any(|f| f == root) => format!(
            "`{root}` is a `State` field, not an accessor — reading a field in a \
             template needs a typed field resolver, which is not implemented yet"
        ),
        None => format!("`{root}` is not a zero-argument `State` method"),
    }
}

fn keep_answerable_interpolation_keys(
    methods: &[StateMethod],
    fields: &[String],
    keys: Vec<String>,
    warnings: &mut Vec<String>,
) -> Vec<String> {
    let mut answerable = Vec::with_capacity(keys.len());
    for key in keys {
        let path = member_path(&key);
        let root = if is_bare_identifier(&key) {
            Some(key.as_str())
        } else {
            path.as_ref().map(|(root, _)| *root)
        };
        let Some(root) = root else {
            answerable.push(key);
            continue;
        };
        let answered = key_is_answerable(methods, &key);
        if answered {
            answerable.push(key);
            continue;
        }
        let reason = unanswerable_root_reason(root, path.is_some(), methods, fields);
        let remedy = if path.is_some() {
            format!(
                "Declare `pub fn {root}(&self) -> YourType` on `State` and a method for each \
                 segment after it."
            )
        } else {
            format!("Declare `pub fn {root}(&self) -> impl std::fmt::Display` on `State`.")
        };
        warnings.push(format!(
            "interpolation {{{{ {key} }}}} cannot be resolved — {reason}; the interpolation \
             renders empty. {remedy}"
        ));
    }
    answerable
}

/// The Rust value one `:attr="expr"` binding emits, together with the resolver
/// keys that value looks up.
enum BindValue {
    /// `resolve("<key>")` — the authored expression is looked up verbatim.
    Lookup(String),
    /// Object-syntax `:class="{ cls: cond, ... }"` — a block that joins the
    /// classes whose condition is truthy.
    ClassObject(String),
    /// A v-for loop variable or one of its fields, read directly.
    Direct(String),
}

impl BindValue {
    /// The emitted Rust expression, ready to be interpolated into generated
    /// code (`resolve("key")`, a direct loop-variable read, or a block that
    /// joins class names).
    fn expr(&self) -> &str {
        match self {
            BindValue::Lookup(expr) | BindValue::ClassObject(expr) | BindValue::Direct(expr) => {
                expr
            }
        }
    }
}

/// One normalized bound attribute.
struct BindEmission {
    value: BindValue,
    /// The resolver keys `value` looks up, in lookup order.
    keys: Vec<String>,
    /// The authored expression, kept for diagnostics.
    authored: String,
}

/// Normalize a bound attribute for emission.
///
/// Every `resolve(...)` codegen emits for a `:attr="expr"` binding comes from
/// this function, and [`collect_resolver_keys`] reads the `keys` it reports, so
/// the lookups codegen emits and the keys the resolver registers cannot drift
/// apart.
///
/// `resolve_loop` is the `v-for` body the element sits in, set only when the
/// Resolve renderer is being generated. It is `None` for the State renderer,
/// where the loop item is a real binding and is read directly.
fn emit_bind_attr(
    attr_name: &str,
    expr: &str,
    item_name: Option<&str>,
    idx_name: Option<&str>,
    resolve_loop: Option<&VForInfo>,
) -> BindEmission {
    let authored = if attr_name == "key" {
        // A `:key` value is normalized by the same helper the `v-for` key emit
        // sites use, so this path analyzes exactly the expression codegen emits
        // instead of the raw `{{ … }}` spelling. The `v-for` branches strip `:key`
        // out before emitting, so the only way a `:key` reaches here as a prop is
        // a non-`v-for` element — which is normalized consistently too.
        resolve_key_expr(expr)
    } else {
        expr.trim().to_string()
    };

    if attr_name == "class" {
        let pairs = class_object_pairs(&authored);
        if !pairs.is_empty() {
            let mut conditions: Vec<String> = Vec::new();
            let mut keys: Vec<String> = Vec::new();
            for (cls, cond) in pairs {
                if cls.is_empty() || cond.is_empty() {
                    continue;
                }

                // A loop-rooted condition is a direct field read in the State
                // renderer. In the Resolve one the loop item is an indexed
                // resolver read — a `String`, which needs the same truthiness
                // test every resolver-backed condition gets to sit in an `if`.
                // A bare loop index is left as the direct read, which is
                // KNOWN-BROKEN in both renderers: a condition of `:class="{even:
                // i}"` becomes `if i`, a `usize` in `if` position (E0308). It
                // is left alone because no emitted form of it compiles without
                // inventing a `bool` the template never wrote.
                let value = match rewrite_ctx_expr(&cond, item_name, idx_name) {
                    Some(direct) => match resolve_loop {
                        Some(info) => match resolve_loop_item_expr(&direct, info) {
                            Some(read) if read != info.index_name => resolver_truthiness(&read),
                            _ => direct,
                        },
                        None => direct,
                    },
                    None => {
                        for key in condition_resolver_keys(&cond) {
                            push_unique(&mut keys, key);
                        }
                        rewrite_if_expr(&cond)
                    }
                };
                conditions.push(format!(
                    "if {} {{ __classes.push({}); }}",
                    value,
                    string_lit(&cls)
                ));
            }
            if !conditions.is_empty() {
                let value = format!(
                    "{{ let mut __classes: Vec<&str> = Vec::new(); {} __classes.join(\" \") }}",
                    conditions.join(" ")
                );
                return BindEmission {
                    value: BindValue::ClassObject(value),
                    keys,
                    authored,
                };
            }
        }
    }

    if let Some(direct) = rewrite_ctx_expr(&authored, item_name, idx_name) {
        // In the Resolve renderer the loop item is not a Rust binding, so a
        // loop-rooted binding is normalized to the indexed resolver read the
        // same loop body already uses for an interpolated `{ todo.text }`.
        // A root the loop cannot read that way (a compound expression such as
        // `todo.a + todo.b`) keeps the resolver lookup, so the diagnostic
        // reports it rather than leaving it silently empty.
        let direct = match resolve_loop {
            Some(info) => match resolve_loop_item_expr(&direct, info) {
                Some(indexed) => indexed,
                None => {
                    return BindEmission {
                        value: BindValue::Lookup(format!("resolve({})", string_lit(&authored))),
                        keys: vec![authored.clone()],
                        authored,
                    };
                }
            },
            None => direct,
        };
        return BindEmission {
            value: BindValue::Direct(direct),
            keys: Vec::new(),
            authored,
        };
    }

    BindEmission {
        value: BindValue::Lookup(format!("resolve({})", string_lit(&authored))),
        keys: vec![authored.clone()],
        authored,
    }
}

/// The component-prop form of a normalized binding value: both are `String`.
fn bind_attr_value(
    attr_name: &str,
    expr: &str,
    item_name: Option<&str>,
    idx_name: Option<&str>,
) -> String {
    let emission = emit_bind_attr(attr_name, expr, item_name, idx_name, None);
    match &emission.value {
        BindValue::Direct(direct) => format!("format!(\"{{}}\", {direct})"),
        _ => format!("{}.clone()", emission.value.expr()),
    }
}

/// The element-prop form of a normalized binding value (`Props::set` entry).
fn bind_prop_entry(
    key: &str,
    attr_name: &str,
    expr: &str,
    item_name: Option<&str>,
    idx_name: Option<&str>,
    resolve_loop: Option<&VForInfo>,
) -> String {
    let emission = emit_bind_attr(attr_name, expr, item_name, idx_name, resolve_loop);
    match &emission.value {
        BindValue::Direct(direct) => format!(r#".set("{}", &format!("{{}}", {direct}))"#, key),
        BindValue::ClassObject(value) => format!(r#".set("class", {value})"#),
        BindValue::Lookup(value) => format!(r#".set("{}", &{value})"#, key),
    }
}

/// The `class: condition` pairs of an object-syntax `:class="{ ... }"` binding.
fn class_object_pairs(expr: &str) -> Vec<(String, String)> {
    let trimmed = expr.trim();
    if !(trimmed.starts_with('{') && trimmed.ends_with('}')) {
        return Vec::new();
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    inner
        .split(',')
        .filter_map(|pair| pair.split_once(':'))
        .map(|(cls, cond)| (cls.trim().to_string(), cond.trim().to_string()))
        .filter(|(cls, cond)| !cls.is_empty() && !cond.is_empty())
        .collect()
}

/// The resolver keys a `v-if`/`:class` condition expression looks up.
///
/// Mirrors the identifier runs `rewrite_if_expr` turns into `resolve("<key>")`
/// lookups, so a condition's operands are registered as keys — `:class`
/// conditions included — whether they are bare (`completed`), negated
/// (`!completed`) or part of a comparison (`count > 0`).
fn condition_resolver_keys(expr: &str) -> Vec<String> {
    let chars: Vec<char> = expr.chars().collect();
    let mut keys: Vec<String> = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            // The advance past the entry character is UNCONDITIONAL. This scanner
            // enters on `[A-Za-z_]` and extends on `is_ident_char`, and a
            // scanner that enters on one predicate while extending on another
            // must advance on the entry character: if `is_ident_char` is ever
            // narrowed to exclude a character this test accepts, the old
            // `while is_ident_char(chars[i])` loop would not move the cursor and
            // this would spin forever. Every token is therefore at least one
            // character wide, which is what the old loop produced too, because
            // `is_ident_char` accepts every character the entry test accepts.
            i += 1;
            while i < chars.len() && is_ident_char(chars[i]) {
                i += 1;
            }
            let token: String = chars[start..i].iter().collect();
            // Literals and emitted code are not lookups.
            if !matches!(token.as_str(), "true" | "false" | "resolve" | "state") {
                keys.push(token);
            }
        } else {
            i += 1;
        }
    }
    keys
}

/// A `v-for` in the Resolve renderer, and whether anything in its body reads the
/// loop in a way the renderer cannot answer.
struct ResolveLoopFamily {
    /// The collection expression, the one the renderer hands to `resolve(...)`.
    collection: String,
    /// Whether a binding, condition, `:key` or interpolation in the body reads
    /// the loop in a form the Resolve renderer cannot answer.
    ///
    /// THE DECISION RULE, in full:
    ///
    /// * A read of the loop ITEM always counts. Every item form is emitted as
    ///   the indexed resolver read `items[0]…`, which no flat arm answers.
    /// * A read of the loop INDEX counts UNLESS the site emits the direct
    ///   binding read. The two index forms that render are a bare-index binding
    ///   — a `:bind`, including `:class` and `:style`, and the `:key` the loop
    ///   branch reads itself (`format!("{}", index)`) — and a bare-index
    ///   interpolation (`text(index.to_string())`): in each case exactly the
    ///   expression that is the index name and nothing else. Any other
    ///   expression naming the index, such as `i + 1` or `{a: i > 2}`, is
    ///   emitted as the flat `resolve("…")` lookup, which answers `"""`, and so
    ///   is a condition: `rewrite_if_expr` rewrites every bare identifier it is
    ///   given into `resolve("<ident>")`, which is why `v-if="i"` is not a
    ///   direct read however bare it is. The exemption is therefore per
    ///   expression, never per loop: a direct index read does not launder a
    ///   compound sibling on the same element.
    body_reads_loop: bool,
}

/// How a site emits a read of the loop index.
#[derive(Clone, Copy, PartialEq)]
enum IndexEmission {
    /// The site emits the index itself — a binding or an interpolation — so a
    /// bare index name is the direct binding read.
    Direct,
    /// The site hands the expression to the flat resolver, so even a bare index
    /// name is an empty answer. A `v-for` collection emits
    /// `let __for_expr = resolve("<expr>")`, and a condition on a loop element
    /// is rewritten by `rewrite_if_expr` into `resolve("<ident>")`; see
    /// [`directive_emission`].
    Resolver,
}

/// Where an attribute sits relative to the `v-for` it belongs to.
///
/// The two positions are emitted by different paths, and that difference — not
/// the attribute's name — decides how a read of the loop index is emitted. A
/// directive on the loop element is handled by `emit_node_with_mode` before the
/// loop branch, and its value goes through `rewrite_if_expr`; a directive on any
/// element inside the body is dropped by `emit_props_in_loop` and emits nothing.
#[derive(Clone, Copy, PartialEq)]
enum AttrSite {
    /// The element that carries the `v-for`.
    LoopElement,
    /// An element inside that loop's body, emitted by `emit_props_in_loop`.
    InLoopBody,
}

/// How a directive emits a read of the loop index.
///
/// The classification follows the EMIT PATH, read off `emit_node_with_mode` and
/// `emit_props_in_loop`, not the directive's name. `rewrite_if_expr` rewrites
/// every bare identifier into `resolve("<ident>")`, so a condition never emits
/// the direct binding read — `v-if="i"` is `resolve("i")`, an empty answer.
///
/// The two fallback arms are deliberately set at opposite defaults, and the
/// asymmetry is the point:
///
/// - Inside the body, an unrecognised directive is `Direct`. Every directive
///   there is dropped by `emit_props_in_loop` (`:1600-1602`), so it emits no
///   read at all and there is nothing to be wrong about; `Resolver` there
///   would report loops that render perfectly.
/// - On the loop element, an unrecognised directive is `Resolver`. Today the
///   emitter handles only `v-for`, `v-if`/`v-else-if`/`v-else`, `v-show` and
///   `v-model`, and `v-model` is listed explicitly below, so nothing reaches
///   this arm in the current tree. A directive added later is the case this
///   rule has to be right about, and if it is routed through
///   `rewrite_if_expr` like every other condition then `Resolver` is the
///   answer — while `Direct` would silently exempt it. A read wrongly
///   exempted is the silent failure this rule exists to prevent (N3, N6), so
///   the unknown case is reported rather than dismissed; an extra sentence in
///   a diagnostic is the cheap direction to be wrong in.
fn directive_emission(name: &str, site: AttrSite) -> IndexEmission {
    match (name, site) {
        // `v-for` is not classified here: its value is `(item, i) in expr`, not
        // a read, so the caller has already parsed out the collection and passes
        // that to `expr_reads_loop_var` itself — see `attrs_read_loop_root`.
        //
        // Conditions on the loop element: `v-if` (which owns the whole
        // `v-if`/`v-else-if`/`v-else` chain) and `v-show` are both rewritten by
        // `rewrite_if_expr` in `emit_node_with_mode`, before the loop branch,
        // so the read is a resolver lookup. `v-else-if` on the same element as
        // the `v-for` is rejected by the parser, but it is listed with its
        // chain rather than left to the fallback.
        ("if" | "show" | "else-if" | "elseif" | "else", AttrSite::LoopElement) => {
            IndexEmission::Resolver
        }
        // Every directive inside the body is dropped by `emit_props_in_loop`,
        // so it emits no read at all and the direct index exemption is right.
        (_, AttrSite::InLoopBody) => IndexEmission::Direct,
        // `v-model` becomes a `:value` bind plus an `@input` handler in
        // `extract_vmodel` before the loop branch, so its value read is a
        // bind's — the same path as `:value`, which renders. This is the only
        // loop-element directive that is not a condition, and naming it keeps
        // the fallback below free to fail loud.
        ("model", AttrSite::LoopElement) => IndexEmission::Direct,
        // Any directive the emitter does not handle today. Treated as a
        // resolver read: see the note on the two fallbacks above.
        (_, AttrSite::LoopElement) => IndexEmission::Resolver,
    }
}

/// Whether `attrs` read either loop variable in a form the Resolve renderer
/// cannot answer.
fn attrs_read_loop_root(attrs: &[TemplateAttr], item: &str, index: &str, site: AttrSite) -> bool {
    attrs.iter().any(|a| match a.kind {
        AttrKind::Bind => a
            .value
            .as_deref()
            .map(|value| expr_reads_loop_var(value, item, index, IndexEmission::Direct))
            .unwrap_or(false),
        AttrKind::Directive => a
            .value
            .as_deref()
            // A `v-for` introduces its loop variables rather than reading them,
            // so only its collection expression can read an enclosing loop.
            .map(|value| match parse_v_for(value) {
                Some(info) => expr_reads_loop_var(&info.expr, item, index, IndexEmission::Resolver),
                None => expr_reads_loop_var(value, item, index, directive_emission(&a.name, site)),
            })
            .unwrap_or(false),
        AttrKind::Static | AttrKind::On => false,
    })
}

/// Whether anything in `node`'s subtree reads either loop variable in a form
/// the Resolve renderer cannot answer.
///
/// A nested `v-for` rebinding those same names shadows them for its own
/// attributes. It does NOT shadow them for its children: those children see the
/// NESTED binding, not the outer one, so a read there is attributed to the
/// outer loop as well. That over-attributes — a nested loop body cannot really
/// be reading the outer item — and the rule deliberately errs that way, because
/// a false positive here is one extra sentence in a diagnostic while a false
/// negative is a loop that renders empty with nothing said about it.
fn reads_loop_root(node: &Node, item: &str, index: &str) -> bool {
    match node {
        Node::Text(_) => false,
        Node::Interpolation(expr) => expr_reads_loop_var(expr, item, index, IndexEmission::Direct),
        Node::Element {
            attrs, children, ..
        } => {
            let roots = [item, index];
            let shadowed = attrs
                .iter()
                .find(|a| matches!(a.kind, AttrKind::Directive) && a.name == "for")
                .and_then(|a| a.value.as_deref())
                .and_then(parse_v_for)
                .is_some_and(|info| {
                    !roots.is_empty()
                        && roots
                            .iter()
                            .all(|root| *root == info.item_name || *root == info.index_name)
                });
            (!shadowed && attrs_read_loop_root(attrs, item, index, AttrSite::InLoopBody))
                || children.iter().any(|c| reads_loop_root(c, item, index))
        }
    }
}

/// Whether an authored expression reads either loop variable — as the variable
/// itself, as a field path rooted at it, or anywhere inside a compound
/// expression such as `todo.a == todo.b` — in a form the resolver cannot
/// answer, per the rule on [`ResolveLoopFamily::body_reads_loop`].
fn expr_reads_loop_var(expr: &str, item: &str, index: &str, emission: IndexEmission) -> bool {
    if expr_reads_roots(expr, &[item]) {
        return true;
    }
    expr_reads_roots(expr, &[index])
        && (emission == IndexEmission::Resolver || expr.trim() != index)
}

/// Whether an authored expression reads either loop variable at all, with no
/// exemption for the direct index read.
///
/// This is the per-BINDING skip's test, not the family's: a binding rooted at
/// the loop — including a compound one — is left to the loop's own message,
/// which says the whole body cannot render. Whether that body is a false
/// positive is decided by the family, above, not here.
fn expr_reads_loop_root(expr: &str, item: &str, index: &str) -> bool {
    expr_reads_roots(expr, &[item, index])
}

/// Whether an authored expression names `root` as an identifier token.
fn expr_reads_roots(expr: &str, roots: &[&str]) -> bool {
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i].is_ascii_alphabetic() || chars[i] == '_' {
            let start = i;
            // The advance past the entry character is UNCONDITIONAL, for the same
            // reason as in `condition_resolver_keys`: entering on `[A-Za-z_]` and
            // extending on `is_ident_char` means the cursor must move on the entry
            // character, or a narrowing of `is_ident_char` that excludes a
            // character this test accepts leaves the cursor where it is and this
            // loop never terminates. Every token stays at least one character wide,
            // which is what the old `while is_ident_char` produced too.
            i += 1;
            while i < chars.len() && is_ident_char(chars[i]) {
                i += 1;
            }
            let token: String = chars[start..i].iter().collect();
            if roots.contains(&token.as_str()) {
                return true;
            }
        } else {
            i += 1;
        }
    }
    false
}

/// The diagnostic for one `v-for` in the Resolve renderer.
///
/// That renderer's resolver is a flat `&str -> String` table built once, outside
/// every loop. An arm is only useful to a loop when it is backed by a real
/// zero-argument `State` getter: a key collected from an interpolation elsewhere
/// in the template is emitted as `"<key>" => state.<key>().to_string()` against a
/// struct that has no such field, so it does not answer the collection either.
/// With no arm the loop counts zero items and never runs its body at all; with an
/// arm the loop item is still read as `items[0].name`, which no arm answers, so
/// every read of the ITEM is empty while a direct read of the index still renders.
/// One message says so for the whole body rather than one contradictory message
/// per binding, and it also names the case it cannot check — a read the emitter
/// drops — rather than asserting that every value in the body is empty.
fn resolve_loop_warning(family: &ResolveLoopFamily, methods: &[StateMethod]) -> String {
    let collection = &family.collection;
    let has_arm = has_state_getter(methods, collection);
    let cause = if has_arm {
        format!(
            "the loop's values are read as indexed resolver keys (`{collection}[0]…`), which \
             no resolver arm answers, so every read of the loop ITEM renders empty — only a \
             direct read of the loop index, such as `{{{{ index }}}}`, survives. This is also \
             reported for a read that the emitter DROPS rather than makes, such as a condition \
             on an element inside the body: that read renders nothing to be wrong, but it \
             cannot be checked from here, and a loop reported for the wrong reason is better \
             than one that is broken for the wrong reason without saying so"
        )
    } else {
        format!(
            "`{collection}` is not a registered `State` getter, so the resolver returns an empty \
             string for it, the loop counts zero items and its body never runs at all"
        )
    };
    format!(
        "`v-for` over `{collection}` cannot render in Resolve mode — {cause}. Render this \
         component through `render_with_state`, which reads the collection and the loop item \
         directly."
    )
}

/// The resolver keys to register, plus diagnostics for bound expressions that
/// cannot be resolved.
struct ResolverKeys {
    keys: Vec<String>,
    warnings: Vec<String>,
}

/// Collect every key the generated render path looks up through `resolve(...)`.
///
/// Interpolation keys are the historical source; bound attributes are emitted
/// as `resolve("..."")` just the same (`:value="draft"`, `:placeholder="hint"`,
/// `:click-payload="index"`), as are the conditions of an object-syntax
/// `:class`. Both sides are derived from [`emit_bind_attr`], so a key codegen
/// emits is always a key this registers or a diagnostic it reports.
///
/// The generated module carries two renderers: `render_with` (Resolve mode,
/// which only has the resolver's strings) and `render_with_state` (State mode,
/// which reads loop items and fields directly). A binding rooted at a loop
/// variable is a real read in both: a field read in State mode, an indexed
/// resolver lookup in Resolve mode. Neither registers a key, so `mode` does not
/// change which bound expressions are reportable. Every diagnostic is therefore
/// mode-independent: it describes a key the resolver cannot satisfy in either
/// renderer — a loop-rooted expression the loop cannot read, such as
/// `todo.a + todo.b`, among them.
fn collect_resolver_keys(
    nodes: &[Node],
    methods: &[StateMethod],
    fields: &[String],
    mode: RenderMode,
) -> ResolverKeys {
    let mut warnings: Vec<String> = Vec::new();
    // An interpolation key is registered only when the arm for it can be
    // answered; a member path whose root has no accessor is reported instead.
    let mut keys = keep_answerable_interpolation_keys(
        methods,
        fields,
        collect_interpolation_keys(nodes),
        &mut warnings,
    );
    // Resolve-mode `v-for` bodies, reported once the key set is final: whether a
    // collection has a resolver arm depends on every key collected in the tree.
    let mut resolve_loops: Vec<ResolveLoopFamily> = Vec::new();

    fn walk(
        nodes: &[Node],
        methods: &[StateMethod],
        fields: &[String],
        mode: RenderMode,
        item_name: Option<&str>,
        idx_name: Option<&str>,
        loop_info: Option<&VForInfo>,
        keys: &mut Vec<String>,
        warnings: &mut Vec<String>,
        resolve_loops: &mut Vec<ResolveLoopFamily>,
    ) {
        for node in nodes {
            let Node::Element {
                attrs, children, ..
            } = node
            else {
                continue;
            };
            // A `v-for` element is emitted — props and children alike — with its
            // own loop variables in scope, which the bindings read directly.
            let loop_scope = attrs
                .iter()
                .find(|a| matches!(a.kind, AttrKind::Directive) && a.name == "for")
                .and_then(|a| a.value.as_deref())
                .and_then(parse_v_for);
            let (item_name, idx_name, loop_info) = match &loop_scope {
                Some(info) => (
                    Some(info.item_name.as_str()),
                    Some(info.index_name.as_str()),
                    Some(info),
                ),
                None => (item_name, idx_name, loop_info),
            };
            // A `v-for` the Resolve renderer will emit is one whole family: the
            // collection it counts and every value its body reads. It is
            // reported once, below, from that collection's getter alone — not
            // from the collected key set, which counts interpolation keys too
            // and would silence a loop whose collection has no arm.
            if let (RenderMode::Resolve, Some(info)) = (mode, &loop_scope) {
                resolve_loops.push(ResolveLoopFamily {
                    collection: info.expr.clone(),
                    body_reads_loop: attrs_read_loop_root(
                        attrs,
                        &info.item_name,
                        &info.index_name,
                        AttrSite::LoopElement,
                    ) || children
                        .iter()
                        .any(|c| reads_loop_root(c, &info.item_name, &info.index_name)),
                });
            }
            for attr in attrs {
                // A `v-model` desugars to a `:value` bind at emit time, so its
                // key is not in the AST this walks. It is collected here
                // instead, through the same gate every other key goes through,
                // so a `v-model` value reads a real getter or reports why it
                // cannot. The write half is reported on its own below.
                if matches!(attr.kind, AttrKind::Directive) && attr.name == "model" {
                    let Some(expr) = attr.value.as_deref() else {
                        continue;
                    };
                    if vmodel_is_loop_rooted(expr, item_name, idx_name) {
                        // The read is a direct field read of the loop item in
                        // State mode and an indexed resolver lookup in Resolve
                        // mode, so it registers no key. The write is the part
                        // that cannot be routed, and in Resolve mode the write
                        // half does not exist at all.
                        if matches!(mode, RenderMode::State) {
                            warnings.push(vmodel_loop_write_warning(expr));
                        }
                        continue;
                    }
                    // The same question the interpolation gate asks, so a dotted
                    // `v-model` is answered by the same method chain `{{ form.name }}`
                    // reads, and an unanswerable one is reported in the same terms.

                    if key_is_answerable(methods, expr) {
                        push_unique(keys, expr);
                    } else {
                        // The write is named, not claimed: the generated setter is
                        // `VModel::vmodel_set(&self.{expr}, payload)`, so it
                        // compiles only when every segment of `{expr}` is a field
                        // of `State` and the last one is a type `VModel` is
                        // implemented for. Saying the write works when the
                        // expression names no such field would promise a
                        // capability the emitted code does not have.
                        warnings.push(format!(
                            "v-model=\"{expr}\" cannot render its value — {}; the input renders \
             empty. The write needs every segment of `{expr}` to be a `State` \
             field and the last one to be a type `VModel` is implemented for \
             (`Signal<T>`, `RefCell<String>` or `Cell<T>`); otherwise the \
             generated setter does not compile. Bind a zero-argument `State` \
             getter, or a method for each segment of a dotted expression.",
                            unresolvable_key_reason(expr, methods, fields)
                        ));
                    }

                    if matches!(mode, RenderMode::Resolve) {
                        warnings.push(vmodel_resolve_mode_warning(expr));
                    }
                    continue;
                }
                if !matches!(attr.kind, AttrKind::Bind) {
                    continue;
                }
                let Some(expr) = attr.value.as_deref() else {
                    continue;
                };
                // The Resolve renderer reads a loop-rooted binding through the
                // indexed resolver the loop body itself uses; the State renderer
                // reads the loop item directly. Either way it registers no key,
                // and the whole loop is reported once, below.
                let resolve_loop = match mode {
                    RenderMode::Resolve => loop_info,
                    RenderMode::State => None,
                };
                let emission = emit_bind_attr(&attr.name, expr, item_name, idx_name, resolve_loop);
                if matches!(emission.value, BindValue::Direct(_)) {
                    continue;
                }

                for key in &emission.keys {
                    if has_state_getter(methods, key) {
                        push_unique(keys, key);
                        continue;
                    }
                    // A loop-rooted binding inside a Resolve-mode `v-for` is
                    // already covered, as a whole, by the loop's own diagnostic
                    // below: that one says the loop cannot render and why. A
                    // second message about the same binding would only restate
                    // it. A binding that is not rooted at the loop is an ordinary
                    // one and is still reported.
                    if matches!(mode, RenderMode::Resolve)
                        && loop_info.is_some()
                        && expr_reads_loop_root(
                            expr,
                            item_name.unwrap_or(""),
                            idx_name.unwrap_or(""),
                        )
                    {
                        continue;
                    }
                    // Only bare, zero-argument State getters with a renderable
                    // return type can be looked up. Everything else is a mistake
                    // worth saying out loud rather than rendering as an empty
                    // string.
                    let reason = unresolvable_key_reason(key, methods, fields);
                    warnings.push(format!(
                        "bound attribute :{}=\"{}\" cannot be resolved — {reason}; \
                         the binding renders empty. Bind a zero-argument State getter instead.",
                        attr.name, emission.authored
                    ));
                }
            }
            walk(
                children,
                methods,
                fields,
                mode,
                item_name,
                idx_name,
                loop_info,
                keys,
                warnings,
                resolve_loops,
            );
        }
    }

    walk(
        nodes,
        methods,
        fields,
        mode,
        None,
        None,
        None,
        &mut keys,
        &mut warnings,
        &mut resolve_loops,
    );
    for family in &resolve_loops {
        // Only a real `State` getter backs an arm that can answer the
        // collection; a key that merely appears as an interpolation elsewhere in
        // the template does not, and treating it as one would leave this loop
        // silent and unrenderable.
        let has_arm = has_state_getter(methods, &family.collection);
        if !has_arm || family.body_reads_loop {
            warnings.push(resolve_loop_warning(family, methods));
        }
    }
    ResolverKeys { keys, warnings }
}

fn push_unique(out: &mut Vec<String>, key: impl AsRef<str>) {
    let key = key.as_ref();
    if !out.iter().any(|k| k == key) {
        out.push(key.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn methods(script: &str) -> Vec<StateMethod> {
        extract_state_methods(script)
    }

    /// A bound expression that cannot become a resolver lookup has to be
    /// reported, not silently rendered as an empty string.
    #[test]
    fn unsupported_bound_expression_is_reported_as_a_warning() {
        let nodes =
            crate::template_parse::parse_template_to_ast(r#"<input :value="draft.trim()" />"#)
                .unwrap();
        let collected = collect_resolver_keys(&nodes, &methods(""), &[], RenderMode::State);

        assert!(
            collected.keys.is_empty(),
            "an unresolvable expression must not be registered: {:?}",
            collected.keys
        );
        assert_eq!(collected.warnings.len(), 1, "{:?}", collected.warnings);
        let warning = &collected.warnings[0];
        assert!(warning.contains(":value=\"draft.trim()\""), "{warning}");
        assert!(warning.contains("`draft.trim()`"), "{warning}");
        assert!(warning.contains("renders empty"), "{warning}");
    }

    /// A bound name that is a payload-taking method is reported the same way:
    /// registering it would emit `state.on_input().to_string()`.
    #[test]
    fn payload_taking_handler_binding_is_reported_as_a_warning() {
        let script = "impl State { pub fn on_input(&self, payload: &str) { let _ = payload; } }";
        let nodes =
            crate::template_parse::parse_template_to_ast(r#"<input :value="on_input" />"#).unwrap();
        let collected = collect_resolver_keys(&nodes, &methods(script), &[], RenderMode::State);

        assert!(collected.keys.is_empty(), "{:?}", collected.keys);
        assert_eq!(collected.warnings.len(), 1, "{:?}", collected.warnings);
        assert!(
            collected.warnings[0].contains("payload-taking State method"),
            "{}",
            collected.warnings[0]
        );
    }

    /// A binding over a `Signal`/`Ref` field has no getter, so it cannot be
    /// looked up: the author is told instead of getting an empty render.
    #[test]
    fn field_backed_binding_is_reported_as_a_warning() {
        let script = "pub struct State { pub count: std::rc::Rc<velox_core::signal::Signal<i32>> }";
        let nodes =
            crate::template_parse::parse_template_to_ast(r#"<input :value="count" />"#).unwrap();
        let collected = collect_resolver_keys(
            &nodes,
            &methods(script),
            &["count".to_string()],
            RenderMode::State,
        );

        assert!(collected.keys.is_empty(), "{:?}", collected.keys);
        assert_eq!(collected.warnings.len(), 1, "{:?}", collected.warnings);
        let warning = &collected.warnings[0];
        assert!(warning.contains(":value=\"count\""), "{warning}");
        assert!(warning.contains("`count`"), "{warning}");
        assert!(warning.contains("renders empty"), "{warning}");
    }

    /// A loop-rooted binding is a real read in State mode — a field read off the
    /// loop item — so State mode has no key to register and nothing to report.
    ///
    /// In Resolve mode the loop as a whole cannot render, so it is reported once,
    /// for the loop, and not once per binding inside it: the loop message already
    /// says every one of those reads comes back empty.
    #[test]
    fn loop_rooted_binding_is_covered_by_the_loop_diagnostic_in_resolve_mode() {
        let nodes = crate::template_parse::parse_template_to_ast(
            r#"<div v-for="(todo, idx) in todos"><p :value="todo.text">x</p></div>"#,
        )
        .unwrap();

        let state = collect_resolver_keys(&nodes, &methods(""), &[], RenderMode::State);
        assert!(state.keys.is_empty(), "{:?}", state.keys);
        assert!(
            state.warnings.is_empty(),
            "State mode reads the field off the loop item: {:?}",
            state.warnings
        );

        let resolve = collect_resolver_keys(&nodes, &methods(""), &[], RenderMode::Resolve);
        assert!(resolve.keys.is_empty(), "{:?}", resolve.keys);
        assert_eq!(
            resolve.warnings.len(),
            1,
            "one message for the loop, not one for the loop and one for the \
             binding inside it: {:?}",
            resolve.warnings
        );
        let warning = &resolve.warnings[0];
        assert!(warning.contains("`todos`"), "{warning}");
        assert!(
            !warning.contains(":value=\"todo.text\""),
            "the loop message must not restate the binding: {warning}"
        );
    }

    /// A loop body is reported as a whole, so an expression the loop itself
    /// cannot reduce is not reported a second time on its own. State mode reads
    /// the same expression straight off the loop item, so it resolves there and
    /// is not reported.
    #[test]
    fn compound_loop_rooted_binding_is_covered_by_the_loop_diagnostic() {
        let nodes = crate::template_parse::parse_template_to_ast(
            r#"<div v-for="todo in todos"><p :value="todo.a + todo.b">x</p></div>"#,
        )
        .unwrap();

        let resolve = collect_resolver_keys(&nodes, &methods(""), &[], RenderMode::Resolve);
        assert!(resolve.keys.is_empty(), "{:?}", resolve.keys);
        assert_eq!(resolve.warnings.len(), 1, "{:?}", resolve.warnings);
        let warning = &resolve.warnings[0];
        assert!(warning.contains("`todos`"), "{warning}");
        assert!(
            !warning.contains("renders empty"),
            "the loop message already covers every read in the body, so this \
             binding must not add a second one: {warning}"
        );

        let state = collect_resolver_keys(&nodes, &methods(""), &[], RenderMode::State);
        assert!(
            state.warnings.is_empty(),
            "State mode reads the fields off the loop item, so it resolves: {:?}",
            state.warnings
        );
    }

    /// A binding inside a Resolve-mode loop that is *not* rooted at the loop is
    /// an ordinary one, and the loop diagnostic must not swallow its own.
    #[test]
    fn non_loop_rooted_binding_inside_a_resolve_loop_is_still_reported() {
        let nodes = crate::template_parse::parse_template_to_ast(
            r#"<div v-for="todo in todos"><input :value="draft.trim()" /></div>"#,
        )
        .unwrap();

        let resolve = collect_resolver_keys(&nodes, &methods(""), &[], RenderMode::Resolve);
        assert_eq!(resolve.warnings.len(), 2, "{:?}", resolve.warnings);
        let binding = resolve
            .warnings
            .iter()
            .find(|w| w.contains(":value=\"draft.trim()\""))
            .unwrap_or_else(|| panic!("{:?}", resolve.warnings));
        assert!(binding.contains("renders empty"), "{binding}");
    }

    /// A `State` field resolves through no renderer, so its diagnostic is
    /// mode-independent.
    #[test]
    fn field_backed_binding_is_reported_in_both_modes() {
        let script = "pub struct State { pub count: Rc<Signal<i32>> }";
        let nodes =
            crate::template_parse::parse_template_to_ast(r#"<input :value="count" />"#).unwrap();
        let methods = methods(script);
        let fields = ["count".to_string()];

        for mode in [RenderMode::State, RenderMode::Resolve] {
            let collected = collect_resolver_keys(&nodes, &methods, &fields, mode);
            assert!(collected.keys.is_empty(), "{mode:?}: {:?}", collected.keys);
            assert_eq!(
                collected.warnings.len(),
                1,
                "{mode:?}: {:?}",
                collected.warnings
            );
            assert!(
                collected.warnings[0].contains(":value=\"count\""),
                "{mode:?}: {}",
                collected.warnings[0]
            );
        }
    }

    /// `:key` on a `v-for` element is the same case, and both spellings of the
    /// key expression normalize to the same read. State mode reads the loop item
    /// directly and reports nothing; Resolve mode reports the loop, once, because
    /// the loop cannot render — never the key on its own.
    #[test]
    fn v_for_key_binding_is_covered_by_the_loop_diagnostic_in_resolve_mode() {
        // Both spellings of the same key expression: the collector must analyze
        // the normalized expression codegen emits, not the raw `{{ … }}` text.
        for tpl in [
            r#"<div v-for="todo in todos" :key="todo.id">x</div>"#,
            r#"<div v-for="todo in todos" :key="{{ todo.id }}">x</div>"#,
        ] {
            let nodes = crate::template_parse::parse_template_to_ast(tpl).unwrap();

            let state = collect_resolver_keys(&nodes, &methods(""), &[], RenderMode::State);
            assert!(state.keys.is_empty(), "{tpl} {:?}", state.keys);
            assert!(
                state.warnings.is_empty(),
                "{tpl}: State mode reads the key off the loop item: {:?}",
                state.warnings
            );

            let resolve = collect_resolver_keys(&nodes, &methods(""), &[], RenderMode::Resolve);
            assert!(resolve.keys.is_empty(), "{tpl} {:?}", resolve.keys);
            assert_eq!(resolve.warnings.len(), 1, "{tpl} {:?}", resolve.warnings);
            assert!(
                !resolve.warnings[0].contains(":key="),
                "{tpl}: the key must not be reported separately from its loop: {}",
                resolve.warnings[0]
            );
        }
    }

    /// The `{{ … }}` spelling of a `v-for` `:key` is a loop-rooted binding whose
    /// key codegen normalizes before emitting, so the raw `{{ … }}` text is never
    /// an expression codegen produces. No diagnostic may quote it: it would
    /// describe a defect that is not there.
    #[test]
    fn mustache_key_is_never_quoted_by_a_diagnostic() {
        let nodes = crate::template_parse::parse_template_to_ast(
            r#"<div v-for="todo in todos" :key="{{ todo.id }}">x</div>"#,
        )
        .unwrap();

        for mode in [RenderMode::Resolve, RenderMode::State] {
            let collected = collect_resolver_keys(&nodes, &methods(""), &[], mode);
            for warning in &collected.warnings {
                assert!(
                    !warning.contains("{{"),
                    "{mode:?}: a diagnostic quoting the raw mustache text: {warning}"
                );
            }
        }
    }

    /// What R-1 delivers, stated as an outcome rather than as a shape: a
    /// Resolve-mode `v-for` over a collection the resolver cannot answer runs
    /// its body zero times, so every read inside it is worth nothing — and the
    /// case is reported instead of passing silently.
    ///
    /// Nothing in this workspace compiles a generated module and runs it, so a
    /// runtime value assertion is not available here. The chain asserted below is
    /// the strongest honest form of one, and each link is the generated text a
    /// reader can check: the loop bound is a count, the count is computed from
    /// one resolver answer, the resolver has no arm for that answer, and the
    /// catch-all is the empty string.
    #[test]
    fn resolve_mode_v_for_over_an_unregistered_collection_is_diagnosed_as_empty() {
        const TPL: &str =
            r#"<ul><li v-for="item in items" :key="item.id">{{ item.name }}</li></ul>"#;
        let rs = compile_template_to_rs_full_with_mode(
            TPL,
            "App",
            None,
            None,
            None,
            RenderMode::Resolve,
        )
        .expect("template compiles");

        // The bound of the loop is a count, and the count is one resolver answer.
        assert!(rs.contains("for __idx in 0..__for_count {"), "{rs}");
        assert!(
            rs.contains("let __for_count = if let Ok(n) = __for_expr.parse::<usize>()"),
            "the loop bound must be derived from the collection read: {rs}"
        );
        assert!(
            rs.contains(r#"let __for_expr = resolve("items");"#),
            "the collection is read through the resolver: {rs}"
        );
        // That answer is the empty string: no arm, and the catch-all.
        assert!(
            !rs.contains(r#""items" =>"#),
            "the collection must have no resolver arm: {rs}"
        );
        assert!(rs.contains("_ => String::new()"), "{rs}");
        // An empty answer takes the `is_empty()` branch, so the count is zero and
        // the body never runs. Reported, not silent.
        let nodes = crate::template_parse::parse_template_to_ast(TPL).unwrap();
        let resolve = collect_resolver_keys(&nodes, &methods(""), &[], RenderMode::Resolve);
        assert_eq!(resolve.warnings.len(), 1, "{:?}", resolve.warnings);
        let warning = &resolve.warnings[0];
        assert!(warning.contains("`items`"), "{warning}");
        assert!(
            warning.contains("never runs"),
            "the diagnosis must say the body never runs: {warning}"
        );

        // State mode reads the collection and the item straight off the state, so
        // the same template renders and is not reported there.
        let state = collect_resolver_keys(&nodes, &methods(""), &[], RenderMode::State);
        assert!(state.warnings.is_empty(), "{:?}", state.warnings);
    }

    /// An object `:class` condition the loop cannot reduce is covered by the
    /// loop's own diagnostic, like any other binding in the body. The generated
    /// condition is still wrong in Resolve mode — it names the loop item, which
    /// that renderer does not bind — and that is left for the collection work;
    /// what this pins is that it is not left *silent*, and not reported twice.
    #[test]
    fn compound_class_condition_in_a_resolve_loop_adds_no_second_warning() {
        let nodes = crate::template_parse::parse_template_to_ast(
            r#"<li v-for="todo in todos" :class="{ active: todo.a == todo.b }">x</li>"#,
        )
        .unwrap();

        let resolve = collect_resolver_keys(&nodes, &methods(""), &[], RenderMode::Resolve);
        assert_eq!(resolve.warnings.len(), 1, "{:?}", resolve.warnings);
        assert!(
            resolve.warnings[0].contains("`todos`"),
            "{}",
            resolve.warnings[0]
        );
    }

    /// The loop diagnostic must name the loop only when the loop really cannot
    /// render. A collection the resolver can answer, over a body that reads no
    /// loop variable, renders: reporting it would train the reader to ignore it.
    #[test]
    fn resolve_loop_over_a_registered_collection_is_not_reported() {
        let script = r#"
        impl State {
            pub fn items(&self) -> String { String::new() }
            pub fn draft(&self) -> String { String::new() }
        }"#;
        let nodes = crate::template_parse::parse_template_to_ast(
            r#"<div v-for="item in items"><p :value="draft">x</p></div>"#,
        )
        .unwrap();

        let resolve = collect_resolver_keys(&nodes, &methods(script), &[], RenderMode::Resolve);
        assert_eq!(
            resolve.keys,
            vec!["draft".to_string()],
            "{:?}",
            resolve.keys
        );
        assert!(
            resolve.warnings.is_empty(),
            "the collection is a registered getter and the body reads no loop \
             variable, so the loop renders: {:?}",
            resolve.warnings
        );
    }

    /// The loop index is a real binding in the loop body — an interpolated
    /// index becomes `text(i.to_string())` and a bare-index bind, directive or
    /// `:class` becomes `format!("{}", index)` — so a loop whose body reads
    /// ONLY the index, directly, renders, and reporting it would be a false
    /// positive. Every index form here is the direct read: the value is the
    /// index name and nothing else.
    #[test]
    fn resolve_loop_reading_only_the_index_is_not_reported() {
        let script = r#"
        impl State {
            pub fn items(&self) -> String { String::new() }
        }"#;
        let nodes = crate::template_parse::parse_template_to_ast(
            r#"<ul><li v-for="(item, i) in items">{{ i }}<b :value="i" :class="i" :style="i">x</b></li></ul>"#,
        )
        .unwrap();

        let resolve = collect_resolver_keys(&nodes, &methods(script), &[], RenderMode::Resolve);
        assert!(
            resolve.warnings.is_empty(),
            "the collection is a registered getter and the body reads only the loop \
             index, directly, which is a real binding: {:?}",
            resolve.warnings
        );
    }

    /// A CONDITION on the loop element is not a direct read, however bare the
    /// expression is. `emit_node_with_mode` routes `v-if` through
    /// `rewrite_if_expr`, which rewrites every bare identifier into
    /// `resolve("<ident>")`, so `v-if="i"` is emitted as
    /// `{ let __v = resolve("i"); … }` around the whole loop: the flat resolver
    /// answers `""`, the condition is falsy and the loop renders nothing.
    #[test]
    fn resolve_loop_with_a_condition_reading_only_the_index_is_reported() {
        let script = r#"
        impl State {
            pub fn items(&self) -> String { String::new() }
        }"#;
        let nodes = crate::template_parse::parse_template_to_ast(
            r#"<ul><li v-for="(item, i) in items" v-if="i">x</li></ul>"#,
        )
        .unwrap();

        let resolve = collect_resolver_keys(&nodes, &methods(script), &[], RenderMode::Resolve);
        assert_loop_reported_for_unanswerable_reads(&resolve.warnings);
    }

    /// The same through `v-show` on the template's ROOT loop element, the other
    /// directive the emitter rewrites with `rewrite_if_expr`:
    /// `if !({ let __v = resolve("i"); … })`, so the element is hidden whatever
    /// the loop renders.
    #[test]
    fn resolve_loop_with_a_show_condition_reading_only_the_index_is_reported() {
        let script = r#"
        impl State {
            pub fn items(&self) -> String { String::new() }
        }"#;
        let nodes = crate::template_parse::parse_template_to_ast(
            r#"<li v-for="(item, i) in items" v-show="i">x</li>"#,
        )
        .unwrap();

        let resolve = collect_resolver_keys(&nodes, &methods(script), &[], RenderMode::Resolve);
        assert_loop_reported_for_unanswerable_reads(&resolve.warnings);
    }

    /// A `v-show` on a loop element that is NOT the template root is dropped
    /// outright: only `v-if` is honoured by the children emitter's `v-for`
    /// branch, so no read is emitted and the condition is silently ignored.
    ///
    /// It is reported anyway, deliberately. Erring toward reporting is this
    /// diagnostic's standing direction — a false positive is one extra sentence,
    /// a false negative is silence — and the classification also stays correct
    /// if the dropped directive is ever fixed, whereas exempting it now would
    /// become a fresh silence the day that happens.
    #[test]
    fn resolve_loop_reports_a_dropped_show_condition_on_a_nested_loop_element() {
        let script = r#"
        impl State {
            pub fn items(&self) -> String { String::new() }
        }"#;
        let nodes = crate::template_parse::parse_template_to_ast(
            r#"<ul><li v-for="(item, i) in items" v-show="i">x</li></ul>"#,
        )
        .unwrap();

        let resolve = collect_resolver_keys(&nodes, &methods(script), &[], RenderMode::Resolve);
        assert_loop_reported_for_unanswerable_reads(&resolve.warnings);
    }

    /// F1: the two fallback arms are set at opposite defaults, and each is
    /// pinned here with its reason.
    ///
    /// On the loop element an unrecognised directive is `Resolver`: it is the
    /// one classification with no emit path to read off, it is the case a future
    /// condition directive arrives as, and `Direct` there would silently exempt
    /// a read the resolver cannot answer — the N3/N6 failure. Inside the body it
    /// is `Direct`, because every directive there is dropped by
    /// `emit_props_in_loop` and emits no read at all, so `Resolver` would report
    /// loops that render perfectly. `v-model` is the one loop-element directive
    /// that is not a condition; `extract_vmodel` re-emits it as a `:value` bind,
    /// so it must stay exempt.
    #[test]
    fn unrecognised_directives_fail_loud_on_the_loop_element_and_stay_quiet_in_the_body() {
        assert!(
            directive_emission("if", AttrSite::LoopElement) == IndexEmission::Resolver,
            "a condition on the loop element is rewritten by `rewrite_if_expr`"
        );
        assert!(
            directive_emission("show", AttrSite::LoopElement) == IndexEmission::Resolver,
            "so is `v-show`, through the same handler"
        );
        assert!(
            directive_emission("text", AttrSite::LoopElement) == IndexEmission::Resolver,
            "a directive the emitter does not handle has no direct read to claim, \
             so it must not be exempted from the loop diagnostic"
        );
        assert!(
            directive_emission("text", AttrSite::InLoopBody) == IndexEmission::Direct,
            "inside the body every directive is dropped, so it emits no read to \
             be wrong about and reporting it would flag a loop that renders"
        );
        assert!(
            directive_emission("model", AttrSite::LoopElement) == IndexEmission::Direct,
            "`v-model` becomes a `:value` bind, which renders"
        );
    }

    /// F1, at the decision: an unrecognised directive naming the index on the
    /// loop element must not be exempt, so the loop is reported.
    #[test]
    fn an_unrecognised_directive_naming_the_index_is_reported() {
        let script = r#"
        impl State {
            pub fn items(&self) -> String { String::new() }
        }"#;
        let nodes = crate::template_parse::parse_template_to_ast(
            r#"<ul><li v-for="(item, i) in items" v-when="i">x</li></ul>"#,
        )
        .unwrap();

        let resolve = collect_resolver_keys(&nodes, &methods(script), &[], RenderMode::Resolve);
        assert_loop_reported_for_unanswerable_reads(&resolve.warnings);
    }

    /// F2: the cause for a collection WITH an arm must not claim that every
    /// value in the body renders empty — a direct index read renders. It names
    /// the loop item instead.
    #[test]
    fn the_loop_message_blames_the_item_read_not_every_value() {
        let script = r#"
        impl State {
            pub fn items(&self) -> String { String::new() }
        }"#;
        let nodes = crate::template_parse::parse_template_to_ast(
            r#"<ul><li v-for="(item, i) in items">{{ item.name }}</li></ul>"#,
        )
        .unwrap();

        let resolve = collect_resolver_keys(&nodes, &methods(script), &[], RenderMode::Resolve);
        assert_loop_reported_for_unanswerable_reads(&resolve.warnings);
        let warning = &resolve.warnings[0];
        assert!(
            warning.contains("every read of the loop ITEM renders empty"),
            "the unanswerable read is the loop item's: {warning}"
        );
        assert!(
            !warning.contains("every value in the body renders empty"),
            "a direct index read renders, so that claim would be false: {warning}"
        );
    }

    /// F2: when the only read that triggered the report is one the emitter
    /// DROPS, the message must say so instead of claiming the body renders
    /// empty. A diagnostic that fires for a right reason with a wrong
    /// explanation teaches its reader to ignore the explanation.
    ///
    /// The `v-show` here is on a loop element nested in a parent, and
    /// `emit_children_with_mode`'s `v-for` branch has no `v-show` branch (only
    /// `emit_node_with_mode`'s does), so the read is dropped — see the
    /// `resolves_index: false` case in `index_classification_agrees_with_the_
    /// emitted_read`. The loop is reported anyway, which is the safe direction.
    #[test]
    fn the_loop_message_says_a_dropped_read_is_not_checkable() {
        let script = r#"
        impl State {
            pub fn items(&self) -> String { String::new() }
        }"#;
        let nodes = crate::template_parse::parse_template_to_ast(
            r#"<ul><li v-for="(item, i) in items" v-show="i">x</li></ul>"#,
        )
        .unwrap();

        let resolve = collect_resolver_keys(&nodes, &methods(script), &[], RenderMode::Resolve);
        // Reported on purpose — the direction that is safe to be wrong in.
        assert_loop_reported_for_unanswerable_reads(&resolve.warnings);
        let warning = &resolve.warnings[0];
        assert!(
            warning.contains("the emitter DROPS rather than makes"),
            "the read that triggered this is dropped, so the message must say so: {warning}"
        );
    }

    /// The boundary across attribute KINDS: one element carrying both a
    /// genuinely-rendering direct index bind and a condition the resolver
    /// answers `""` for is still reported. Without this, reclassifying
    /// conditions could have been done by suppressing the loop instead.
    #[test]
    fn resolve_loop_reports_a_condition_beside_a_direct_index_read() {
        let script = r#"
        impl State {
            pub fn items(&self) -> String { String::new() }
        }"#;
        let nodes = crate::template_parse::parse_template_to_ast(
            r#"<ul><li v-for="(item, i) in items" :value="i" v-if="i">x</li></ul>"#,
        )
        .unwrap();

        let resolve = collect_resolver_keys(&nodes, &methods(script), &[], RenderMode::Resolve);
        assert_loop_reported_for_unanswerable_reads(&resolve.warnings);
    }

    /// A directive on an element INSIDE the body is dropped by
    /// `emit_props_in_loop`, so it emits no read and cannot leave the body
    /// empty; the loop is not reported for it. (That the condition is silently
    /// ignored is a separate pre-existing defect, not this rule.)
    #[test]
    fn resolve_loop_condition_on_a_child_in_the_body_is_not_reported() {
        let script = r#"
        impl State {
            pub fn items(&self) -> String { String::new() }
        }"#;
        let nodes = crate::template_parse::parse_template_to_ast(
            r#"<ul><li v-for="(item, i) in items"><b v-if="i">x</b></li></ul>"#,
        )
        .unwrap();

        let resolve = collect_resolver_keys(&nodes, &methods(script), &[], RenderMode::Resolve);
        assert!(
            resolve.warnings.is_empty(),
            "a dropped directive emits no read, so nothing in the body renders \
             empty: {:?}",
            resolve.warnings
        );
    }

    /// The classification table, checked against BOTH facts at once: what the
    /// Resolve renderer emits for that read of the index, and what the family
    /// diagnostic then says about the loop. Neither column is computed from the
    /// other — the emitted form comes from codegen and the decision from the
    /// collector — so a classification that drifts from the emit path fails here
    /// instead of silently exempting an unanswerable read.
    ///
    /// Every expectation below was read off the emitted module, not assumed from
    /// the attribute's name. Only the first `pub fn render_with_state` onwards is
    /// examined: the module also carries the State renderer, whose loop body
    /// reads the loop variables differently.
    #[test]
    fn index_classification_agrees_with_the_emitted_read() {
        // (label, template, emits a resolver read of the index, reported, why)
        let cases: [(&str, &str, bool, bool, &str); 10] = [
            (
                "v-if on a nested loop element",
                r#"<ul><li v-for="(item, i) in items" v-if="i">x</li></ul>"#,
                true,
                true,
                "the children emitter's v-for branch rewrites it to resolve(\"i\")",
            ),
            (
                "v-if on the root loop element",
                r#"<li v-for="(item, i) in items" v-if="i">x</li>"#,
                true,
                true,
                "emit_node_with_mode rewrites it before the loop branch",
            ),
            (
                "v-show on the root loop element",
                r#"<li v-for="(item, i) in items" v-show="i">x</li>"#,
                true,
                true,
                "the same handler, negated",
            ),
            (
                "v-show on a nested loop element",
                r#"<ul><li v-for="(item, i) in items" v-show="i">x</li></ul>"#,
                false,
                true,
                "dropped by that branch, and still reported on purpose",
            ),
            (
                "a call condition naming the index",
                r#"<ul><li v-for="(item, i) in items" v-if="i()">x</li></ul>"#,
                true,
                true,
                "the expression is not the index name, so no exemption",
            ),
            (
                "a compound binding",
                r#"<ul><li v-for="(item, i) in items" :value="i + 1">x</li></ul>"#,
                true,
                true,
                "emitted as resolve(\"i + 1\")",
            ),
            (
                "a direct index bind",
                r#"<ul><li v-for="(item, i) in items" :value="i">x</li></ul>"#,
                false,
                false,
                "emitted as format!(\"{}\", i)",
            ),
            (
                "a bare index interpolation",
                r#"<ul><li v-for="(item, i) in items">{{ i }}</li></ul>"#,
                false,
                false,
                "emitted as i.to_string()",
            ),
            (
                "a condition on a child in the body",
                r#"<ul><li v-for="(item, i) in items"><b v-if="i">x</b></li></ul>"#,
                false,
                false,
                "emit_props_in_loop drops the directive, so it reads nothing",
            ),
            (
                "v-model on the loop element",
                r#"<ul><li v-for="(item, i) in items" v-model="i">x</li></ul>"#,
                false,
                false,
                "no index read is emitted for it",
            ),
        ];
        let script = r#"
        impl State {
            pub fn items(&self) -> String { String::new() }
        }"#;
        for (label, tpl, resolves_index, reported, why) in cases {
            let rs = crate::compile_template_to_rs_full_with_mode(
                tpl,
                script,
                None,
                None,
                None,
                RenderMode::Resolve,
            )
            .expect("template compiles");
            let resolve_renderer = rs
                .split("pub fn render_with_state")
                .next()
                .expect("the module has a Resolve renderer");
            let emitted_resolver_read = resolve_renderer.contains("resolve(\"i\")")
                || resolve_renderer.contains("resolve(\"i + 1\")");
            let nodes = crate::template_parse::parse_template_to_ast(tpl).expect("template parses");
            let resolve = collect_resolver_keys(&nodes, &methods(script), &[], RenderMode::Resolve);
            assert_eq!(
                emitted_resolver_read, resolves_index,
                "{label}: the emitted read changed ({why})\n{resolve_renderer}"
            );
            assert_eq!(
                !resolve.warnings.is_empty(),
                reported,
                "{label}: the decision disagrees with the emitted read ({why}): {:?}",
                resolve.warnings
            );
        }
    }

    /// A COMPOUND expression rooted at the index is not the direct binding read.
    /// `rewrite_ctx_expr` reduces only a bare index or an `item.`-prefixed path,
    /// so `i + 1` falls through to the flat resolver and is emitted as
    /// `resolve("i + 1")`, which the one-arm-per-key table answers `""`. The
    /// loop renders an empty attribute, so it must be reported.
    #[test]
    fn resolve_loop_reading_a_compound_index_binding_is_reported() {
        let script = r#"
        impl State {
            pub fn items(&self) -> String { String::new() }
        }"#;
        let nodes = crate::template_parse::parse_template_to_ast(
            r#"<ul><li v-for="(item, i) in items" :value="i + 1">x</li></ul>"#,
        )
        .unwrap();

        let resolve = collect_resolver_keys(&nodes, &methods(script), &[], RenderMode::Resolve);
        assert_loop_reported_for_unanswerable_reads(&resolve.warnings);
    }

    /// The same compound-index hole through a `:class` condition, which also
    /// happens to emit a non-compiling comparison in both renderers — the
    /// closed compound-`:class` limitation, reached here through the index.
    #[test]
    fn resolve_loop_reading_a_compound_index_condition_is_reported() {
        let script = r#"
        impl State {
            pub fn items(&self) -> String { String::new() }
        }"#;
        let nodes = crate::template_parse::parse_template_to_ast(
            r#"<ul><li v-for="(item, i) in items" :class="{a: i > 2}">x</li></ul>"#,
        )
        .unwrap();

        let resolve = collect_resolver_keys(&nodes, &methods(script), &[], RenderMode::Resolve);
        assert_loop_reported_for_unanswerable_reads(&resolve.warnings);
    }

    /// The same compound-index hole through an interpolation, which is emitted
    /// as `text(resolve("i + 1"))` and answers `""`.
    #[test]
    fn resolve_loop_reading_a_compound_index_interpolation_is_reported() {
        let script = r#"
        impl State {
            pub fn items(&self) -> String { String::new() }
        }"#;
        let nodes = crate::template_parse::parse_template_to_ast(
            r#"<ul><li v-for="(item, i) in items">{{ i + 1 }}</li></ul>"#,
        )
        .unwrap();

        let resolve = collect_resolver_keys(&nodes, &methods(script), &[], RenderMode::Resolve);
        assert_loop_reported_for_unanswerable_reads(&resolve.warnings);
    }

    /// The exemption is per EXPRESSION, not per loop: one element carrying both
    /// a direct index read and a compound one is still reported, so the direct
    /// forms cannot launder a compound sibling.
    #[test]
    fn resolve_loop_reports_a_compound_index_read_beside_direct_ones() {
        let script = r#"
        impl State {
            pub fn items(&self) -> String { String::new() }
        }"#;
        let nodes = crate::template_parse::parse_template_to_ast(
            r#"<ul><li v-for="(item, i) in items" :value="i" :title="i + 1">{{ i }} {{ i * 2 }}</li></ul>"#,
        )
        .unwrap();

        let resolve = collect_resolver_keys(&nodes, &methods(script), &[], RenderMode::Resolve);
        assert_loop_reported_for_unanswerable_reads(&resolve.warnings);
    }

    /// A nested `v-for` whose COLLECTION is the outer index is not a direct
    /// read: the collection is emitted as `let __for_expr = resolve("i")`, which
    /// answers `""`, so the nested loop counts zero items and its body never
    /// runs. The direct-index exemption must not reach that site, and the outer
    /// loop — whose body contains that unreadable read — must not be masked by
    /// it either.
    #[test]
    fn resolve_loop_reading_the_index_as_a_nested_collection_is_reported() {
        let script = r#"
        impl State {
            pub fn items(&self) -> String { String::new() }
        }"#;
        let nodes = crate::template_parse::parse_template_to_ast(
            r#"<ul><li v-for="(item, i) in items"><p v-for="row in i">{{ row }}</p></li></ul>"#,
        )
        .unwrap();

        let resolve = collect_resolver_keys(&nodes, &methods(script), &[], RenderMode::Resolve);
        assert_eq!(resolve.warnings.len(), 2, "{:?}", resolve.warnings);
        let nested = resolve
            .warnings
            .iter()
            .find(|w| w.contains("`v-for` over `i` cannot render in Resolve mode"))
            .unwrap_or_else(|| {
                panic!(
                    "the nested loop over the index must be reported: {:?}",
                    resolve.warnings
                )
            });
        assert!(
            nested.contains("is not a registered `State` getter"),
            "`resolve(\"i\")` answers an empty string, so the count is zero: {nested}"
        );
        assert!(
            resolve
                .warnings
                .iter()
                .any(|w| w.contains("`v-for` over `items` cannot render in Resolve mode")),
            "the outer loop reads the index, so it reports too: {:?}",
            resolve.warnings
        );
    }

    /// Exactly one message for the loop, naming the loop and blaming the
    /// unreadable indexed read — the collection here IS a registered getter, so
    /// "not a registered `State` getter" would be the wrong cause.
    fn assert_loop_reported_for_unanswerable_reads(warnings: &[String]) {
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        let warning = &warnings[0];
        assert!(
            warning.contains("`v-for` over `items` cannot render in Resolve mode"),
            "{warning}"
        );
        assert!(
            warning.contains("indexed resolver keys"),
            "the collection has a getter, so the cause must be the unreadable \
             indexed read: {warning}"
        );
    }

    /// A body that reads the ITEM and nothing else still reports: the direct
    /// index exemption is about the index, never a blanket suppression of the
    /// loop. Every item form — an indexed resolver read, a bound field, a
    /// `{{ … }}` — is unanswerable.
    #[test]
    fn resolve_loop_reading_only_the_item_is_reported() {
        let script = r#"
        impl State {
            pub fn items(&self) -> String { String::new() }
        }"#;
        let nodes = crate::template_parse::parse_template_to_ast(
            r#"<ul><li v-for="(item, i) in items"><b>{{ item.name }}</b></li></ul>"#,
        )
        .unwrap();

        let resolve = collect_resolver_keys(&nodes, &methods(script), &[], RenderMode::Resolve);
        assert_loop_reported_for_unanswerable_reads(&resolve.warnings);
    }

    /// The converse of the index-only case: reading the ITEM as well still
    /// cannot be answered by any resolver arm, so it stays reported. Without
    /// this the N1 fix would be a blanket suppression.
    #[test]
    fn resolve_loop_reading_the_index_and_the_item_is_still_reported() {
        let script = r#"
        impl State {
            pub fn items(&self) -> String { String::new() }
        }"#;
        let nodes = crate::template_parse::parse_template_to_ast(
            r#"<ul><li v-for="(item, i) in items">{{ i }}{{ item.name }}</li></ul>"#,
        )
        .unwrap();

        let resolve = collect_resolver_keys(&nodes, &methods(script), &[], RenderMode::Resolve);
        assert_eq!(resolve.warnings.len(), 1, "{:?}", resolve.warnings);
        let warning = &resolve.warnings[0];
        assert!(
            warning.contains("`v-for` over `items` cannot render in Resolve mode"),
            "{warning}"
        );
        assert!(
            warning.contains("indexed resolver keys"),
            "the collection has a getter, so the cause must be the unreadable \
             indexed read: {warning}"
        );
    }

    /// A collection `State` has no getter for cannot be answered by an arm, so a
    /// loop over it counts zero items and must be reported.
    ///
    /// R-1 found this while a collection name could still reach the key set
    /// without a getter, which is why `has_arm` consults the getter alone: a key
    /// is not an arm. R-1d's gate closes that route for interpolation keys, and
    /// the binding route was already gated by `has_state_getter`, so the hazard
    /// is now unreachable from the template — asserted below rather than
    /// assumed, because a test that no longer holds its premise is how a
    /// narrowing like this hides.
    #[test]
    fn resolve_loop_over_a_collection_with_no_accessor_is_reported() {
        let script = r#"
        impl State {
            pub fn draft(&self) -> String { String::new() }
        }"#;
        let nodes = crate::template_parse::parse_template_to_ast(
            r#"<p :x="items">{{ items }}</p><ul><li v-for="(item, i) in items">{{ i }}</li></ul>"#,
        )
        .unwrap();

        let resolve = collect_resolver_keys(&nodes, &methods(script), &[], RenderMode::Resolve);
        assert!(
            !resolve.keys.iter().any(|k| k == "items"),
            "neither route can register a key `State` has no getter for — the \
             binding is gated by `has_state_getter` and the interpolation by \
             R-1d's gate — so no key can stand in for the getter: {:?}",
            resolve.keys
        );
        assert!(
            resolve
                .warnings
                .iter()
                .any(|w| w.contains("interpolation {{ items }} cannot be resolved")),
            "the interpolation reports why it was dropped: {:?}",
            resolve.warnings
        );
        // Three warnings, all of them about `items`, in this order: the dropped
        // interpolation (this round's gate), the dropped binding (the bind path's
        // own gate) and the loop itself.
        assert_eq!(resolve.warnings.len(), 3, "{:?}", resolve.warnings);
        let warning = &resolve.warnings[2];
        assert!(
            warning.contains("`v-for` over `items` cannot render in Resolve mode"),
            "{warning}"
        );
        assert!(
            warning.contains("not a registered `State` getter"),
            "no getter backs the key, so the cause must be the empty answer: {warning}"
        );

        // The State renderer reads the collection and the item directly, so the
        // loop is not reported there — the family is pushed only under
        // `if let (RenderMode::Resolve, Some(info))`, and this is the pin: the two
        // key-gate warnings are mode-independent, the loop report is not.
        let state = collect_resolver_keys(&nodes, &methods(script), &[], RenderMode::State);
        assert_eq!(state.keys, resolve.keys, "the key set is mode-independent");
        assert_eq!(state.warnings.len(), 2, "{:?}", state.warnings);
        assert!(
            !state
                .warnings
                .iter()
                .any(|w| w.contains("`v-for` over `items` cannot render")),
            "a loop report is Resolve-mode only: {:?}",
            state.warnings
        );
    }

    /// A zero-argument method that returns nothing is not a getter: the arm would
    /// be `state.reset().to_string()`, which does not compile.
    #[test]
    fn method_without_return_type_is_reported_as_a_warning() {
        let script = "impl State { pub fn reset(&self) {} }";
        let nodes =
            crate::template_parse::parse_template_to_ast(r#"<input :value="reset" />"#).unwrap();
        let collected = collect_resolver_keys(&nodes, &methods(script), &[], RenderMode::State);

        assert!(collected.keys.is_empty(), "{:?}", collected.keys);
        assert_eq!(collected.warnings.len(), 1, "{:?}", collected.warnings);
        let warning = &collected.warnings[0];
        assert!(warning.contains(":value=\"reset\""), "{warning}");
        assert!(
            warning.contains("return type") || warning.contains("returns"),
            "the warning must explain the return type: {warning}"
        );
    }

    /// A zero-argument method with an unrenderable return type is not a getter.
    #[test]
    fn unrenderable_return_type_is_reported_as_a_warning() {
        let script = "impl State { pub fn items(&self) -> Vec<String> { Vec::new() } }";
        let nodes =
            crate::template_parse::parse_template_to_ast(r#"<input :value="items" />"#).unwrap();
        let collected = collect_resolver_keys(&nodes, &methods(script), &[], RenderMode::State);

        assert!(collected.keys.is_empty(), "{:?}", collected.keys);
        assert_eq!(collected.warnings.len(), 1, "{:?}", collected.warnings);
        assert!(
            collected.warnings[0].contains("Vec<String>"),
            "the warning must name the return type: {}",
            collected.warnings[0]
        );
    }

    /// Only zero-argument methods whose return type renders as text are getters.
    #[test]
    fn getter_method_name_requires_a_zero_argument_renderable_getter() {
        let script = r#"
impl State {
    pub fn title(&self) -> String { String::new() }
    pub fn get_count(&self) -> i32 { 0 }
    pub fn is_open(&self) -> bool { true }
    pub fn has_items(&self) -> bool { true }
    pub fn on_input(&self, payload: &str) { let _ = payload; }
    pub fn pick(&self, index: usize) -> usize { index }
    pub fn reset(&self) {}
    pub fn rows(&self) -> Vec<String> { Vec::new() }
    pub fn result(&self) -> Result<String, String> { Ok(String::new()) }
}
"#;
        let methods = methods(script);

        assert_eq!(getter_method_name(&methods, "title"), Some("title"));
        assert_eq!(getter_method_name(&methods, "count"), Some("get_count"));
        assert_eq!(getter_method_name(&methods, "open"), Some("is_open"));
        // The prefixed convention still resolves a key whose exact-name method
        // is not a getter, as long as the prefixed one is.
        assert_eq!(getter_method_name(&methods, "items"), Some("has_items"));
        assert_eq!(getter_method_name(&methods, "on_input"), None);
        assert_eq!(getter_method_name(&methods, "pick"), None);
        assert_eq!(getter_method_name(&methods, "reset"), None);
        assert_eq!(getter_method_name(&methods, "rows"), None);
        assert_eq!(getter_method_name(&methods, "result"), None);
        assert!(has_state_getter(&methods, "title"));
        assert!(!has_state_getter(&methods, "on_input"));
        // A prefixed payload-taking method is a handler, not a getter either.
        assert!(!has_state_getter(&methods, "input"));
    }

    /// The two lookups differ in exactly one test, and both go through the same
    /// search — a user struct is a fine start of a member path even though it
    /// has no `Display` form, while the same type as the value itself is not
    /// renderable. Proving both call sites here is what keeps them one notion.
    #[test]
    fn accessor_and_getter_share_one_lookup_and_differ_only_on_renderability() {
        let script = r#"
impl State {
    pub fn user(&self) -> User { User }
    pub fn get_count(&self) -> i32 { 0 }
    pub fn title(&self) -> String { String::new() }
    pub fn on_input(&self, payload: &str) { let _ = payload; }
    pub fn reset(&self) {}
}
"#;
        let methods = methods(script);

        // A user struct: an accessor (it starts a chain), not a getter (it does
        // not render).
        assert_eq!(accessor_method_name(&methods, "user"), Some("user"));
        assert_eq!(getter_method_name(&methods, "user"), None);
        // The `get_`/`is_`/`has_` convention is shared, not re-invented.
        assert_eq!(accessor_method_name(&methods, "count"), Some("get_count"));
        assert_eq!(getter_method_name(&methods, "count"), Some("get_count"));
        assert_eq!(accessor_method_name(&methods, "title"), Some("title"));
        // A payload-taking method and a unit-returning one are neither.
        assert_eq!(accessor_method_name(&methods, "on_input"), None);
        assert_eq!(accessor_method_name(&methods, "reset"), None);
    }

    /// A member path is split into a root and the segments after it, and
    /// anything that is not one is refused so every caller keeps its own
    /// behaviour.
    #[test]
    fn member_path_splits_a_path_and_refuses_everything_else() {
        assert_eq!(member_path("user.name"), Some(("user", vec!["name"])));
        assert_eq!(member_path("a.b.c"), Some(("a", vec!["b", "c"])));
        assert_eq!(
            member_path("items[0].name"),
            Some(("items", vec!["[0]", "name"]))
        );
        for not_a_path in [
            "title",       // a bare name has nothing to chain
            "user.name()", // a call is not a path; it keeps the fallback, ungated
            "user.",       // a trailing separator names no segment
            ".name",       // no root
            "user..name",  // an empty segment
            "user name",   // not an identifier
            "items[0",     // an unterminated index
            "",            // nothing at all
        ] {
            assert_eq!(member_path(not_a_path), None, "{not_a_path:?}");
        }
    }

    /// The invariant, on the key set itself: a member path is registered only
    /// when its root has an accessor, and dropped with a diagnostic otherwise.
    #[test]
    fn a_member_path_key_is_registered_only_when_its_root_has_an_accessor() {
        let script = r#"
impl State {
    pub fn user(&self) -> User { User }
    pub fn title(&self) -> String { String::new() }
}
"#;
        let with_accessor =
            crate::template_parse::parse_template_to_ast(r#"<p>{{ user.name }} {{ title }}</p>"#)
                .unwrap();
        let answerable =
            collect_resolver_keys(&with_accessor, &methods(script), &[], RenderMode::State);
        assert_eq!(
            answerable.keys,
            vec!["user.name".to_string(), "title".to_string()],
            "an answerable member path keeps its key, in order"
        );
        assert!(answerable.warnings.is_empty(), "{:?}", answerable.warnings);

        let without =
            crate::template_parse::parse_template_to_ast(r#"<p>{{ user.name }} {{ title }}</p>"#)
                .unwrap();
        for mode in [RenderMode::State, RenderMode::Resolve] {
            let dropped = collect_resolver_keys(&without, &methods(""), &[], mode);
            assert_eq!(
                dropped.keys,
                Vec::<String>::new(),
                "an unanswerable member path is not registered, so it cannot emit an arm, and \
                 neither is the bare name beside it, in {mode:?}"
            );
            assert_eq!(dropped.warnings.len(), 2, "{:?}", dropped.warnings);
            let warning = &dropped.warnings[0];
            assert!(warning.contains("{{ user.name }}"), "{warning}");
            assert!(
                warning.contains("`user` is not a zero-argument `State` method"),
                "{warning}"
            );
            assert!(warning.contains("renders empty"), "{warning}");
        }
    }

    /// The diagnostic says which part of the path is the problem and why, using
    /// the same reasons the bound-attribute path reports.
    #[test]
    fn the_member_path_diagnostic_names_the_root_and_why() {
        let tpl = r#"<p>{{ user.name }}</p>"#;
        let nodes = crate::template_parse::parse_template_to_ast(tpl).unwrap();

        let payload = r#"
impl State { pub fn user(&self, payload: &str) -> String { payload.to_string() } }
"#;
        let unit = r#"
impl State { pub fn user(&self) {} }
"#;
        let script = r#"
pub struct State { user: String }
impl State { pub fn new() -> Self { Self { user: String::new() } } }
"#;
        for (script_text, fields, expected) in [
            (
                payload,
                Vec::new(),
                "`user` is a payload-taking State method",
            ),
            (unit, Vec::new(), "`user` returns nothing"),
            (
                script,
                vec!["user".to_string()],
                "`user` is a `State` field",
            ),
        ] {
            let collected =
                collect_resolver_keys(&nodes, &methods(script_text), &fields, RenderMode::State);
            assert_eq!(collected.keys, Vec::<String>::new(), "{expected}");
            assert_eq!(collected.warnings.len(), 1, "{:?}", collected.warnings);
            assert!(
                collected.warnings[0].contains(expected),
                "expected {expected:?} in {:?}",
                collected.warnings
            );
        }
    }

    /// A bare name IS gated, and only on a getter — the arm it produces is
    /// `state.title().to_string()`, which a `State` without `title` cannot
    /// compile. With a getter it is registered unchanged, which is the case the
    /// examples and the goldens rest on.
    ///
    /// `{{ user.name() }}` is NOT gated: a key with a call in it is not a name a
    /// `State` method can have and not a member path, so it keeps the
    /// long-standing fallback. That is pre-existing and disclosed, not claimed
    /// closed.
    #[test]
    fn a_bare_interpolation_key_is_gated_on_a_getter_and_otherwise_untouched() {
        let template = r#"<p>{{ title }} {{ user.name() }} {{ count }}</p>"#;
        let nodes = crate::template_parse::parse_template_to_ast(template).unwrap();

        // No getters at all: every bare name is dropped, and the paren key — which
        // no rule here can speak about — is the only one that survives.
        let collected = collect_resolver_keys(&nodes, &methods(""), &[], RenderMode::State);
        assert_eq!(
            collected.keys,
            vec!["user.name()".to_string()],
            "a bare name with no getter is not registered, so it cannot emit an \
             unanswerable arm; a call key is not gated"
        );
        assert_eq!(collected.warnings.len(), 2, "{:?}", collected.warnings);
        for (key, warning) in [
            ("title", &collected.warnings[0]),
            ("count", &collected.warnings[1]),
        ] {
            assert!(warning.contains(&format!("{{{{ {key} }}}}")), "{warning}");
            assert!(
                warning.contains(&format!("`{key}` is not a zero-argument `State` method")),
                "{warning}"
            );
            assert!(warning.contains("Declare `pub fn"), "{warning}");
        }

        // A getter answers the bare name, and the key keeps its place in the set.
        let with_getters = r#"
impl State {
    pub fn title(&self) -> String { String::new() }
    pub fn count(&self) -> i32 { 0 }
}
"#;
        let answered =
            collect_resolver_keys(&nodes, &methods(with_getters), &[], RenderMode::State);
        assert_eq!(
            answered.keys,
            vec![
                "title".to_string(),
                "user.name()".to_string(),
                "count".to_string()
            ],
            "a getter-backed bare name is registered exactly as before, in order"
        );
        assert!(answered.warnings.is_empty(), "{:?}", answered.warnings);
    }

    /// A bare name whose accessor returns something with no `Display` form is
    /// dropped too: `state.items().to_string()` on a `Vec<Item>` is the same
    /// compile break as a missing method.
    #[test]
    fn a_bare_name_with_an_unrenderable_return_type_is_not_registered() {
        let script = r#"
impl State {
    pub fn items(&self) -> Vec<Item> { Vec::new() }
}
"#;
        let nodes = crate::template_parse::parse_template_to_ast(r#"<p>{{ items }}</p>"#).unwrap();
        let collected = collect_resolver_keys(&nodes, &methods(script), &[], RenderMode::State);
        assert_eq!(collected.keys, Vec::<String>::new(), "{:?}", collected.keys);
        assert_eq!(collected.warnings.len(), 1, "{:?}", collected.warnings);
        assert!(
            collected.warnings[0].contains(
                "`items` is a `State` method, but its return type cannot be rendered as text"
            ),
            "{}",
            collected.warnings[0]
        );
    }

    /// The renderability gate accepts what `.to_string()` accepts for the scalar
    /// and string types a template can show, and nothing else.
    #[test]
    fn return_type_is_renderable_accepts_only_display_types() {
        for ty in [
            Some("String"),
            Some("&str"),
            Some("&'static str"),
            Some("str"),
            Some("bool"),
            Some("char"),
            Some("i32"),
            Some("usize"),
            Some("f64"),
            Some("Rc<String>"),
            Some("Cow<'_, str>"),
        ] {
            assert!(return_type_is_renderable(ty), "{ty:?} should be renderable");
        }
        for ty in [
            None,
            Some("()"),
            Some("Self"),
            Some("Vec<String>"),
            Some("Option<String>"),
            Some("Result<String, String>"),
            Some("Signal<i32>"),
            Some("Todo"),
        ] {
            assert!(!return_type_is_renderable(ty), "{ty:?} is not renderable");
        }
    }

    /// The condition keys collected must match the lookups
    /// `rewrite_if_expr` actually emits, for every supported condition shape.
    #[test]
    fn condition_resolver_keys_match_rewrite_if_expr_lookups() {
        for cond in [
            "completed",
            "!completed",
            "count > 0",
            "todo.completed",
            "is_empty()",
            "true",
            "!false",
            "filter == \"all\"",
        ] {
            let rewritten = rewrite_if_expr(cond);
            let emitted: Vec<&str> = rewritten
                .match_indices("resolve(\"")
                .map(|(i, _)| {
                    let rest = &rewritten[i + "resolve(\"".len()..];
                    &rest[..rest.find('"').expect("closing quote")]
                })
                .collect();
            let collected = condition_resolver_keys(cond);
            assert_eq!(
                collected,
                emitted
                    .iter()
                    .map(|k| k.to_string())
                    .collect::<Vec<String>>(),
                "condition `{cond}` emits {emitted:?} but collects {collected:?}"
            );
        }
    }

    /// Both identifier scanners must RETURN on input whose continuation stops at
    /// once, and they must do it observably.
    ///
    /// Each scanner enters on `[A-Za-z_]` and extends with the shared
    /// `is_ident_char`, then advances. That is two predicates, so the cursor is
    /// moved on the entry character unconditionally — see the comment at each
    /// scanner. If `is_ident_char` is ever narrowed to exclude a character the
    /// entry test accepts, a scanner that only advanced inside the `while` would
    /// leave the cursor where it is and spin forever.
    ///
    /// WHY A THREAD AND A TIMEOUT: with today's `is_ident_char` the two forms are
    /// extensionally identical, so no input can tell them apart and **this test
    /// passes with or without the unconditional advance — it is a GUARD, not
    /// evidence for the advance.** What it buys is that the landmine is
    /// observable: without the thread a narrowed predicate would hang the whole
    /// suite with no output at all, and here the spin arrives as a failed
    /// assertion on this test instead. The evidence for the advance itself is the
    /// narrowing experiment in the report, not this test.
    ///
    /// The inputs are the ones a continuation-only `while` stops on: a letter or
    /// underscore followed by a non-identifier character, and the digit-leading
    /// form that the entry test declines (so it must be skipped, not consumed).
    #[test]
    fn a_scanner_that_enters_on_a_first_char_test_advances_even_when_this_is_narrow() {
        use std::sync::mpsc;
        use std::time::Duration;

        // (expression, keys the condition scanner must collect, whether the
        // root scanner must report a read of `item`)
        let cases = [
            ("a!", vec!["a"], false),
            ("_x!", vec!["_x"], false),
            ("a!b", vec!["a", "b"], false),
            ("9a!", vec!["a"], false),
            ("item!", vec!["item"], true),
            ("item.name", vec!["item", "name"], true),
        ];

        for (expr, expected_keys, reads_item) in cases {
            let (tx, rx) = mpsc::channel();
            let expr_owned = expr.to_string();
            let probe = tx.clone();
            std::thread::spawn(move || {
                let _ = probe.send(condition_resolver_keys(&expr_owned));
            });
            let (tx2, rx2) = mpsc::channel();
            let expr_owned = expr.to_string();
            std::thread::spawn(move || {
                let _ = tx2.send(expr_reads_roots(&expr_owned, &["item", "i"]));
            });

            let collected = rx.recv_timeout(Duration::from_secs(10)).unwrap_or_else(|_| {
                panic!("condition_resolver_keys did not return on `{expr}` — the scanner is not making progress")
            });
            let reads = rx2.recv_timeout(Duration::from_secs(10)).unwrap_or_else(|_| {
                panic!("expr_reads_roots did not return on `{expr}` — the scanner is not making progress")
            });

            assert_eq!(collected, expected_keys, "keys collected from `{expr}`");
            assert_eq!(reads, reads_item, "root read reported for `{expr}`");
        }
    }

    // ---- R-1c: a loop item is not a `State` field, so an interpolation that
    // reads one must not become a resolver key. Before this, `{{ item.name }}`
    // inside `v-for="item in items"` collected the top-level key `item.name`
    // and `generate_make_resolve` emitted `"item.name" => state.item.name()`,
    // which is E0609: the field is `items` and there is no `item()` getter.
    // The key set is asserted WHOLE, not by absence of one name, so a key
    // reappearing under another name fails here too.

    /// R-1c's script, plus the accessor a member path needs: R-1d gates a
    /// member-path key on its ROOT having a zero-argument accessor, so a script
    /// without one would make these tests fail for a reason that has nothing to
    /// do with what they are about (loop-rooted keys, and document order).
    const R1C_SCRIPT: &str = r#"
        pub struct User { name: String }
        impl State {
            pub fn items(&self) -> String { String::new() }
            pub fn title(&self) -> String { String::new() }
            pub fn user(&self) -> User { User { name: String::new() } }
        }"#;

    fn keys_of(tpl: &str, mode: RenderMode) -> Vec<String> {
        let nodes = crate::template_parse::parse_template_to_ast(tpl).expect("template parses");
        collect_resolver_keys(&nodes, &methods(R1C_SCRIPT), &[], mode).keys
    }

    #[test]
    fn a_loop_rooted_interpolation_is_not_a_resolver_key() {
        for tpl in [
            // the canonical idiom, the one the golden used to encode as E0609
            r#"<ul><li v-for="item in items">{{ item.name }}</li></ul>"#,
            // an explicitly indexed loop, so the item name is `__idx` by default
            r#"<ul><li v-for="item in items">{{ item.name }} {{ item.id }}</li></ul>"#,
            // the index, and the collection itself
            r#"<ul><li v-for="(item, i) in items">{{ i }}</li></ul>"#,
            // an interpolation nested deeper in the body
            r#"<ul><li v-for="item in items"><span><b>{{ item.name }}</b></span></li></ul>"#,
            // the outer item read inside a nested loop, which the inner `v-for`
            // does not shadow
            r#"<ul><li v-for="(item, i) in items"><ul><li v-for="c in rows">{{ item.name }} {{ c }}</li></ul></li></ul>"#,
        ] {
            for mode in [RenderMode::State, RenderMode::Resolve] {
                assert_eq!(
                    keys_of(tpl, mode),
                    Vec::<String>::new(),
                    "{mode:?}: a read of a loop variable cannot be answered by a \
                     flat arm over `State` fields: {tpl}"
                );
            }
        }
    }

    /// The fix is scoped to loop-rooted keys. A dotted interpolation that
    /// appears at the top level of the component is a lookup of a state field
    /// path and is collected exactly as before — the whole set, in order.
    #[test]
    fn a_root_level_dotted_interpolation_is_still_a_key() {
        for mode in [RenderMode::State, RenderMode::Resolve] {
            assert_eq!(
                keys_of(
                    r#"<p>{{ user.name }} {{ items[0].name }} {{ title }}</p>"#,
                    mode
                ),
                vec!["user.name", "items[0].name", "title"],
                "{mode:?}: nothing here is inside a loop, so nothing is dropped"
            );
        }
    }

    /// Dropping a loop-rooted key must not disturb the ones that stay: the
    /// surrounding keys keep their document order, and a key read inside a
    /// loop body does not leak out to the top level either.
    #[test]
    fn loop_rooted_interpolations_are_dropped_and_the_rest_keep_document_order() {
        for mode in [RenderMode::State, RenderMode::Resolve] {
            assert_eq!(
                keys_of(
                    r#"<header>{{ title }}</header><ul><li v-for="(item, i) in items">{{ item.name }} {{ i }} {{ i + 1 }}</li></ul><footer>{{ user.name }}</footer>"#,
                    mode
                ),
                vec!["title", "user.name"],
                "{mode:?}: the two loop-rooted keys go, the two that surround \
                 them keep their order"
            );
        }
    }

    /// The open question R-1 left behind, pinned from both sides at once.
    ///
    /// R-1 treats a bare-index interpolation (`{{ i }}`) as one of only two
    /// forms that emit a DIRECT read of the loop variable and therefore
    /// render, so its diagnostic does not report a loop whose body reads
    /// nothing but the index. That is a decision about whether to REPORT a
    /// loop; this task removes `{{ i }}` from the KEY set, which is a decision
    /// about whether to EMIT an arm. The two are independent, and this test
    /// asserts both facts on the same two templates: `{{ i }}` is not a key
    /// and is not reported, while `{{ i + 1 }}` is also not a key but IS
    /// reported (it is emitted as the flat `resolve("i + 1")`, which the arm
    /// table cannot answer). Neither decision can move without this test
    /// showing the other.
    #[test]
    fn the_bare_index_interpolation_is_direct_and_is_still_not_a_key() {
        let direct = r#"<ul><li v-for="(item, i) in items">{{ i }}</li></ul>"#;
        let compound = r#"<ul><li v-for="(item, i) in items">{{ i + 1 }}</li></ul>"#;
        let nodes =
            |tpl: &str| crate::template_parse::parse_template_to_ast(tpl).expect("template parses");
        let direct_resolve = collect_resolver_keys(
            &nodes(direct),
            &methods(R1C_SCRIPT),
            &[],
            RenderMode::Resolve,
        );
        let compound_resolve = collect_resolver_keys(
            &nodes(compound),
            &methods(R1C_SCRIPT),
            &[],
            RenderMode::Resolve,
        );

        assert_eq!(
            direct_resolve.keys,
            Vec::<String>::new(),
            "`{{ i }}` is loop-rooted, so it is not a top-level key: {:?}",
            direct_resolve.keys
        );
        assert!(
            direct_resolve.warnings.is_empty(),
            "R-1's exemption: a bare-index interpolation emits `text(i.to_string())`, \
             a direct read, so the loop is not reported: {:?}",
            direct_resolve.warnings
        );
        assert_eq!(
            compound_resolve.keys,
            Vec::<String>::new(),
            "the same key-set rule applies to a compound index read: {:?}",
            compound_resolve.keys
        );
        assert_eq!(
            compound_resolve.warnings.len(),
            1,
            "and R-1's N3 rule is untouched: the body emits `text(resolve(\"i + 1\"))`, \
             which no arm answers, so the loop IS reported: {:?}",
            compound_resolve.warnings
        );
    }

    /// The same closed state, asserted from the key-set side: a `v-for`
    /// collection with no accessor contributes no key from either route, and the
    /// loop is still reported (R-1's N2 finding, re-pinned because this task
    /// edits the key set its premise was about).
    #[test]
    fn a_collection_with_no_accessor_registers_no_key_and_is_still_reported() {
        let nodes = crate::template_parse::parse_template_to_ast(
            r#"<p :x="items">{{ items }}</p><ul><li v-for="(item, i) in items">{{ i }}</li></ul>"#,
        )
        .expect("template parses");
        let script = r#"
        impl State {
            pub fn draft(&self) -> String { String::new() }
        }"#;

        let resolve = collect_resolver_keys(&nodes, &methods(script), &[], RenderMode::Resolve);
        assert!(
            !resolve.keys.iter().any(|k| k == "items"),
            "a collection with no accessor is registered from neither route: {:?}",
            resolve.keys
        );
        assert_eq!(
            resolve.warnings.len(),
            3,
            "the loop report, the dropped interpolation and the dropped binding, and \
             nothing else: {:?}",
            resolve.warnings
        );
        assert!(
            !resolve.keys.iter().any(|k| k == "item" || k == "i"),
            "but nothing loop-rooted is: {:?}",
            resolve.keys
        );
    }
    // =========================================================================
    // R-2. `v-model`: the read side is a resolver key like any other, and the two
    // conditions codegen cannot honour are reported in R-1's family.
    // =========================================================================

    const R2_SCRIPT: &str = r#"
        pub struct User { name: String }
        impl User {
            pub fn name(&self) -> String { String::new() }
        }
        impl State {
            pub fn items(&self) -> String { String::new() }
            pub fn draft(&self) -> String { String::new() }
            pub fn user(&self) -> User { User { name: String::new() } }
        }"#;

    fn r2_keys_and_warnings(
        tpl: &str,
        script: &str,
        fields: &[&str],
        mode: RenderMode,
    ) -> (Vec<String>, Vec<String>) {
        let nodes = crate::template_parse::parse_template_to_ast(tpl).expect("template parses");
        let fields: Vec<String> = fields.iter().map(|f| f.to_string()).collect();
        let collected = collect_resolver_keys(&nodes, &methods(script), &fields, mode);
        (collected.keys, collected.warnings)
    }

    /// The live read. The synthetic `:value` bind is created at emit time, so before
    /// this it was invisible to key collection and every `v-model` read resolved to
    /// the empty string.
    #[test]
    fn a_v_model_on_an_accessor_is_registered_as_a_resolver_key() {
        for mode in [RenderMode::State, RenderMode::Resolve] {
            let (keys, warnings) =
                r2_keys_and_warnings(r#"<input v-model="draft"/>"#, R2_SCRIPT, &[], mode);
            assert_eq!(
                keys,
                vec!["draft".to_string()],
                "a v-model on a `State` accessor must get a resolver arm, in {mode:?}"
            );
            if matches!(mode, RenderMode::State) {
                assert!(warnings.is_empty(), "{:?}", warnings);
            }
        }
    }

    /// A dotted `v-model` is answered by the same chain an interpolation of the same
    /// path reads, and dropped under the same conditions.
    #[test]
    fn a_dotted_v_model_is_answered_by_the_same_chain_as_an_interpolation() {
        let (keys, warnings) = r2_keys_and_warnings(
            r#"<input v-model="user.name"/>"#,
            R2_SCRIPT,
            &[],
            RenderMode::State,
        );
        assert_eq!(keys, vec!["user.name".to_string()], "{:?}", keys);
        assert!(warnings.is_empty(), "{:?}", warnings);

        // Without the accessor the read cannot be answered, so it is reported rather
        // than left to the empty-string fallback.
        let (keys, warnings) = r2_keys_and_warnings(
            r#"<input v-model="form.name"/>"#,
            R2_SCRIPT,
            &["form"],
            RenderMode::State,
        );
        assert_eq!(keys, Vec::<String>::new(), "{:?}", keys);
        assert_eq!(warnings.len(), 1, "{:?}", warnings);
        assert!(
            warnings[0].contains(r#"v-model="form.name""#),
            "{}",
            warnings[0]
        );
        assert!(
            warnings[0].contains("is a `State` field"),
            "{}",
            warnings[0]
        );
        assert!(warnings[0].contains("renders empty"), "{}", warnings[0]);
    }

    /// E-1. A `v-model` on a loop item reads the item's value and writes nothing,
    /// because the dispatcher resolves `on:input` against `State` and `State` has no
    /// handle on the loop item.
    #[test]
    fn a_v_model_on_a_loop_item_reads_it_and_writes_nothing() {
        for mode in [RenderMode::State, RenderMode::Resolve] {
            let (keys, warnings) = r2_keys_and_warnings(
                r#"<ul><li v-for="item in items"><input v-model="item.name"/></li></ul>"#,
                R2_SCRIPT,
                &[],
                mode,
            );
            assert_eq!(
                keys,
                Vec::<String>::new(),
                "a loop-rooted v-model must not register a resolver key, in {mode:?}"
            );
            // State reports the write, because the loop renders there and the write
            // is the part that cannot happen. Resolve mode reports the loop instead,
            // which is the stronger fact: nothing in the body renders at all, so a
            // second warning about its write would only repeat it.
            if matches!(mode, RenderMode::State) {
                assert_eq!(warnings.len(), 1, "{:?}", warnings);
                assert!(
                    warnings[0].contains(r#"v-model="item.name""#)
                        && warnings[0].contains("cannot be written"),
                    "{}",
                    warnings[0]
                );
            } else {
                assert_eq!(warnings.len(), 1, "{:?}", warnings);
                assert!(
                    warnings[0].contains("`v-for` over `items` cannot render"),
                    "{}",
                    warnings[0]
                );
            }
        }
    }

    /// Ruling 1: Resolve mode grows no write path, so it reports instead. The
    /// diagnostic is in R-1's family and says which renderer does support it.
    #[test]
    fn a_v_model_in_resolve_mode_is_reported() {
        let (keys, warnings) = r2_keys_and_warnings(
            r#"<input v-model="draft"/>"#,
            R2_SCRIPT,
            &[],
            RenderMode::Resolve,
        );
        assert_eq!(keys, vec!["draft".to_string()], "the read still resolves");
        assert_eq!(warnings.len(), 1, "{:?}", warnings);
        assert!(
            warnings[0].contains(r#"v-model="draft""#),
            "{}",
            warnings[0]
        );
        assert!(
            warnings[0].contains("cannot be written in Resolve mode"),
            "{}",
            warnings[0]
        );
        assert!(
            warnings[0].contains("render_with_state"),
            "the report names the renderer that does support it: {}",
            warnings[0]
        );
    }

    /// One name, from one place: the dotted translation is a hardcoded literal here,
    /// not a call into the function under test, and the loop-rooted case is skipped by
    /// both collectors.
    #[test]
    fn the_v_model_collectors_agree_on_one_hardcoded_setter_name() {
        for (tpl, expected_expressions, expected_handlers) in [
            (
                r#"<input v-model="draft"/>"#,
                vec!["draft"],
                vec!["__vmodel_set_draft"],
            ),
            (
                r#"<input v-model="user.name"/>"#,
                vec!["user.name"],
                vec!["__vmodel_set_user_name"],
            ),
            (
                r#"<ul><li v-for="item in items"><input v-model="item.name"/></li></ul>"#,
                vec![],
                vec![],
            ),
        ] {
            let nodes = crate::template_parse::parse_template_to_ast(tpl).expect("template parses");
            let collected = collect_vmodel_expressions(&nodes);
            let expressions: Vec<String> = collected.iter().map(|(expr, _)| expr.clone()).collect();
            assert_eq!(expressions, expected_expressions, "expressions for {tpl}");

            // The handler the setter is generated under is the one the dispatcher
            // carries, and the dotted translation lives in one place.
            let handlers: Vec<String> = collected
                .iter()
                .map(|(_, handler)| handler.clone())
                .collect();
            assert_eq!(handlers, expected_handlers, "handlers for {tpl}");

            let dispatcher: Vec<String> = collect_handlers(&nodes)
                .into_iter()
                .filter(|h| h.starts_with("__vmodel_set_"))
                .collect();
            assert_eq!(
                dispatcher, expected_handlers,
                "the dispatcher must carry an arm for every non-loop-rooted v-model, under \
                 the same name, and none for a loop-rooted one, in {tpl}"
            );
        }
    }
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
