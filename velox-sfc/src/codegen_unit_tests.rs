use crate::template_ast::{AttrKind, Node, TemplateAttr};

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
    let rust = crate::compile_template_to_rs(tpl, "app", None)
        .expect("compile should succeed");
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
