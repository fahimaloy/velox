use crate::template_ast::{AttrKind, Node, TemplateAttr};

#[test]
fn script_setup_lifecycle_hook_macros_pass_through() {
    // A `<script setup>` that registers lifecycle hooks through the fully-qualified
    // velox-core macros must be emitted verbatim into the generated module so the
    // hooks are wired when the SFC is compiled by the CLI/example build.rs.
    let sfc_src = r#"<template>
  <div><span>{{ title }}</span></div>
</template>
<script setup>
use velox_core::ref_value;

pub struct State { pub title: String }
impl State { pub fn new() -> Self { Self { title: String::from("Lifecycle") } } }
</script>
<style>
.app { padding: 12px; }
</style>"#;
    // Simulate injecting the hook registrations at the top of script_setup,
    // as a user would write them, and confirm codegen carries them through.
    let with_hooks = sfc_src.replace(
        "<script setup>\n",
        "<script setup>\nvelox_core::on_mounted! { { /* setup */ } }\nvelox_core::on_updated! { { /* update */ } }\nvelox_core::on_unmounted! { { /* teardown */ } }\n",
    );
    let sfc = crate::parse_sfc(&with_hooks).expect("parse sfc");
    let stub = crate::to_stub_rs(&sfc, "app");

    for expected in [
        "velox_core::on_mounted!",
        "velox_core::on_updated!",
        "velox_core::on_unmounted!",
    ] {
        assert!(
            stub.contains(expected),
            "generated module should contain `{expected}` verbatim"
        );
    }
    // User code lands inside the `script_rs` module.
    assert!(stub.contains("pub mod script_rs"));
}

#[test]
fn emit_node_text_and_interpolation() {
    let t = Node::Text("hello".to_string());
    let out = crate::template_codegen::emit_node(&t);
    assert_eq!(out, r#"text("hello")"#);

    let i = Node::Interpolation("count".to_string());
    let out2 = crate::template_codegen::emit_node(&i);
    // interpolation uses resolve in other helpers; here emit_node returns text(&resolve("count"))
    assert!(out2.contains("resolve"));
}

#[test]
fn emit_props_varieties() {
    let attrs: Vec<TemplateAttr> = vec![
        TemplateAttr {
            name: "class".into(),
            value: Some("btn".into()),
            kind: AttrKind::Static,
        },
        TemplateAttr {
            name: "on:click".into(),
            value: Some("increment".into()),
            kind: AttrKind::On,
        },
        TemplateAttr {
            name: "click".into(),
            value: Some("inc".into()),
            kind: AttrKind::On,
        },
    ];

    let out = crate::template_codegen::emit_props(&attrs);
    assert!(out.contains("set(\"class\", \"btn\")") || out.contains("class"));
    assert!(out.contains("on:click"));
}

#[test]
fn emit_children_simple() {
    let children = vec![Node::Text("a".into()), Node::Text("b".into())];
    let out = crate::template_codegen::emit_children(&children);
    assert!(out.starts_with("vec!["));
    assert!(out.contains("text(\"a\")"));
}

#[test]
fn rewrite_if_expr_logical_and() {
    let out = crate::template_codegen::rewrite_if_expr("is_visible && is_active");
    assert!(out.contains("&&"));
    assert!(out.contains("resolve(\"is_visible\")"));
    assert!(out.contains("resolve(\"is_active\")"));
}

#[test]
fn rewrite_if_expr_logical_or() {
    let out = crate::template_codegen::rewrite_if_expr("is_visible || is_active");
    assert!(out.contains("||"));
    assert!(out.contains("resolve(\"is_visible\")"));
    assert!(out.contains("resolve(\"is_active\")"));
}

#[test]
fn rewrite_if_expr_negation() {
    let out = crate::template_codegen::rewrite_if_expr("!is_hidden");
    assert!(out.starts_with("!("));
    assert!(out.contains("resolve(\"is_hidden\")"));
}

#[test]
fn rewrite_if_expr_negation_true() {
    let out = crate::template_codegen::rewrite_if_expr("!true");
    assert!(out.contains("!(true)") || out.contains("!true"));
}

#[test]
fn rewrite_if_expr_combined_logic() {
    let out = crate::template_codegen::rewrite_if_expr("is_visible && !is_hidden");
    assert!(out.contains("&&"));
    assert!(out.contains("!"));
    assert!(out.contains("resolve(\"is_visible\")"));
    assert!(out.contains("resolve(\"is_hidden\")"));
}

#[test]
fn rewrite_if_expr_comparison_preserved() {
    let out = crate::template_codegen::rewrite_if_expr("count > 0");
    assert!(out.contains(">"));
    assert!(out.contains("resolve(\"count\")"));
    assert!(out.contains("parse::<f64>()"));
}

#[test]
fn rewrite_if_expr_comparison_with_logic() {
    let out = crate::template_codegen::rewrite_if_expr("count > 0 && is_visible");
    assert!(out.contains(">"));
    assert!(out.contains("&&"));
    assert!(out.contains("resolve(\"count\")"));
    assert!(out.contains("parse::<f64>()"));
}

#[test]
fn v_if_else_emits_block_push() {
    let tpl = r#"<template>
      <div class="app">
        <p class="a">{{ x }}</p>
        <p v-if="ok" class="b">yes</p>
        <p v-else class="c">no</p>
        <p class="d">{{ y }}</p>
      </div>
    </template>"#;
    let rust = crate::compile_template_to_rs(tpl, "app", None).expect("compile should succeed");
    // The conditional must NOT be pushed as a parenthesized value (the bug):
    //   __children.push((if (...) { ... } else { ... }))
    assert!(
        !rust.contains("__children.push((if "),
        "v-if/v-else must not be pushed as a parenthesized value: {}",
        rust
    );
    // It must instead be pushed as a BARE conditional expression — no parens
    // (the bug above) and no outer block either:
    //   __children.push(if (...) { ... } else { ... })
    // The braces that remain are the `if` arm's own, which is where a component's
    // `let __props` / `let __callbacks` bindings live. An outer `{ … }` around the
    // whole expression is what emitted `unused_braces` in every scaffolded app's
    // generated `app.rs` — four warnings, at the `v-if` sites whose body rustc can
    // see on a single line (`UnusedBraces` is suppressed across multiple lines).
    assert!(
        !rust.contains("__children.push({"),
        "the conditional must not be wrapped in a redundant outer block: {}",
        rust
    );
    assert!(
        rust.contains("__children.push(if "),
        "expected a bare conditional push: {}",
        rust
    );
    // Unconditional siblings must still be emitted:
    assert!(
        rust.contains("\"a\"") && rust.contains("\"d\""),
        "siblings dropped by conditional codegen: {}",
        rust
    );
}

#[test]
fn v_if_else_resolves_to_single_branch() {
    // Compiles the same template and renders via render_with_state with a
    // resolver, confirming only one branch's text is present in the tree.
    let tpl = r#"<template>
      <div class="app">
        <p v-if="ok" class="b">yes</p>
        <p v-else class="c">no</p>
      </div>
    </template>"#;
    let rust = crate::compile_template_to_rs(tpl, "app", None).expect("compile");
    // The generated code must contain both branch text literals (so the
    // conditional selects at runtime, not at compile time).
    assert!(
        rust.contains("\"yes\"") && rust.contains("\"no\""),
        "branches missing: {}",
        rust
    );
    // And the push must be a single expression per render function (one child
    // added for the conditional), not two separate pushes that would desync
    // layout's source_index. compile_template_to_rs emits two render fns
    // (render_with and render_with_state) that both contain the conditional, so
    // expect 2. The count is the invariant; the shape it counts is pinned by
    // `v_if_else_emits_block_push`.
    let pushes = rust.matches("__children.push(if ").count();
    assert_eq!(
        pushes, 2,
        "expected one conditional push per render fn: {}",
        rust
    );
}

#[test]
fn scoped_style_prefixes_selectors_with_scope_id() {
    use crate::codegen::to_stub_rs;
    use crate::sfc::{Attr, Sfc, StyleBlock};

    // Scoped style: selectors must be prefixed and a non-empty SCOPE_ID emitted.
    let scoped = Sfc {
        style: Some(StyleBlock {
            attrs: vec![Attr {
                name: "scoped".into(),
                value: None,
            }],
            content: ".header h1 { color: red; }\nh1, .btn { margin: 0; }\n".into(),
        }),
        ..Default::default()
    };
    let out = to_stub_rs(&scoped, "Counter");
    assert!(
        out.contains("pub const SCOPE_ID: &str = \"data-v-"),
        "expected a non-empty scope id: {}",
        out
    );
    // Attribute appended to every compound of each selector.
    assert!(
        out.contains(".header[data-v-") && out.contains("] h1[data-v-"),
        "descendant selector compounds not scoped: {}",
        out
    );
    assert!(
        out.contains("h1[data-v-") && out.contains(".btn[data-v-"),
        "selector list members not scoped: {}",
        out
    );

    // Unscoped style passes through unchanged with an empty scope id.
    let unscoped = Sfc {
        style: Some(StyleBlock {
            attrs: vec![],
            content: ".header { color: red; }".into(),
        }),
        ..Default::default()
    };
    let plain = to_stub_rs(&unscoped, "Counter");
    assert!(
        plain.contains("pub const SCOPE_ID: &str = \"\";"),
        "unscoped component should emit empty scope id: {}",
        plain
    );
    assert!(
        plain.contains(".header { color: red; }") && !plain.contains("[data-v-"),
        "unscoped style must pass through unchanged: {}",
        plain
    );
}

// ---------------------------------------------------------------------------
// Named slots: the three pure AST transforms behind `slots_map_expr`
// ---------------------------------------------------------------------------
//
// These are asserted on the NODE rather than on generated Rust, and that is the
// whole point of putting them here. `strip_slot_binding` removes an
// `AttrKind::Directive` from the content, and `emit_props_in_loop` emits only
// `AttrKind::Static` and `AttrKind::Bind` — so whether the `v-slot:` attribute
// was stripped is not observable in the generated source, and an integration
// assertion about it holds whether the strip happens or not. A test that cannot
// fail is a test that reports nothing.

use crate::template_codegen::{slot_binding, slot_content, strip_slot_binding};

/// The element a `v-slot:` / `#` fragment is written as in these tests.
fn slot_bound(tag: &str, directive: &str, children: Vec<Node>) -> Node {
    Node::Element {
        tag: tag.to_string(),
        attrs: vec![
            TemplateAttr {
                name: directive.to_string(),
                value: None,
                kind: AttrKind::Directive,
            },
            TemplateAttr {
                name: "class".to_string(),
                value: Some("own".to_string()),
                kind: AttrKind::Static,
            },
        ],
        children,
        self_closing: false,
    }
}

fn text_node(t: &str) -> Node {
    Node::Text(t.to_string())
}

/// A `<template v-slot:x>` contributes its CHILDREN, not itself.
///
/// `template` is not an element the renderer knows, so a fragment bound through
/// one has to be flattened or it renders as an inert `<template>` node with the
/// caller's content inside it.
#[test]
fn a_template_fragment_contributes_its_children_not_itself() {
    let node = slot_bound(
        "template",
        "slot:header",
        vec![text_node("a"), text_node("b")],
    );
    let content = slot_content(&node);
    assert_eq!(
        content.len(),
        2,
        "the `<template>` wrapper must be flattened away, leaving its two children \
         as the slot's content. Got {content:?}"
    );
    assert!(matches!(&content[0], Node::Text(t) if t == "a"));
    assert!(matches!(&content[1], Node::Text(t) if t == "b"));
}

/// A PLAIN element is its own content, and keeps its own attributes.
///
/// This is the half of `slot_content` that flattening is not, and without it
/// `<h1 v-slot:header class="lead">` would lose its `class` — the caller wrote
/// that markup, and the child is not entitled to drop it.
#[test]
fn a_plain_element_is_its_own_content_and_keeps_its_own_attributes() {
    let node = slot_bound("h1", "slot:header", vec![text_node("title")]);
    let content = slot_content(&node);
    assert_eq!(
        content.len(),
        1,
        "expected the element itself, got {content:?}"
    );
    let Node::Element { tag, attrs, .. } = &content[0] else {
        panic!("expected an element, got {:?}", content[0]);
    };
    assert_eq!(tag, "h1");
    assert!(
        attrs
            .iter()
            .any(|a| a.name == "class" && a.value.as_deref() == Some("own")),
        "the element's own `class` must survive the slot pass. Got {attrs:?}"
    );
}

/// The `v-slot:` binding comes OFF the content it named, and only it does.
///
/// It named the slot; it is not an attribute of the content. It is removed by
/// name-prefix, so this also pins that the filter is not "drop the first
/// directive" — the `class` is the first attribute here and has to stay.
#[test]
fn the_slot_binding_is_stripped_and_nothing_else_is() {
    let node = slot_bound("p", "slot:footer", vec![text_node("x")]);
    let stripped = strip_slot_binding(&node);
    let Node::Element { attrs, .. } = &stripped else {
        panic!("expected an element, got {stripped:?}");
    };
    assert!(
        !attrs
            .iter()
            .any(|a| a.kind == AttrKind::Directive && a.name.starts_with("slot:")),
        "the `v-slot:` binding must not survive onto the content. Got {attrs:?}"
    );
    assert_eq!(
        attrs.len(),
        1,
        "only the `slot:` directive goes; every other attribute is markup the \
         caller wrote. Got {attrs:?}"
    );
}

/// The name is read off the folded directive, for both spellings.
///
/// The parser folds `v-slot:footerBar` and `#footerBar` onto `slot:footer-bar`
/// before either arrives here, so this reads the same string for both. The
/// assertion is that no OTHER fold is attempted on top: the name in the map key
/// is exactly what the attribute says.
#[test]
fn the_slot_name_is_read_off_the_directive_unchanged() {
    let bound = slot_bound("p", "slot:footer-bar", vec![]);
    assert_eq!(
        slot_binding(&bound).as_deref(),
        Some("footer-bar"),
        "the name is taken from the attribute as it arrived, not re-folded. The \
         parser already folded it."
    );
    let unnamed = Node::Element {
        tag: "p".to_string(),
        attrs: vec![],
        children: vec![],
        self_closing: false,
    };
    assert_eq!(
        slot_binding(&unnamed),
        None,
        "a child that names no slot is the default slot, which is `None` here and \
         becomes the key `\"default\"` in `slots_map_expr`. Not the string \
         `\"default\"`: a child that names a slot called `default` would then be \
         indistinguishable from one that names none."
    );
}
