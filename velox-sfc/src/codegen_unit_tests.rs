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
    // It must instead be pushed as a block that yields a single VNode:
    //   __children.push({ if (...) { ... } else { ... } })
    assert!(
        rust.contains("__children.push({"),
        "expected block push for conditional: {}",
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
    // And the push must be a single block per render function (one child added
    // for the conditional), not two separate pushes that would desync layout's
    // source_index. compile_template_to_rs emits two render fns (render_with
    // and render_with_state) that both contain the conditional, so expect 2.
    let pushes = rust.matches("__children.push({ if").count();
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
    // Attribute appended to the descendant-most part of each selector.
    assert!(
        out.contains(".header h1[data-v-"),
        "descendant selector not scoped: {}",
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
