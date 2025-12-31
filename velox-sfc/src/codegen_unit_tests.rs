use crate::template_ast::{Node, TemplateAttr, AttrKind};

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
    let mut attrs: Vec<TemplateAttr> = Vec::new();
    attrs.push(TemplateAttr { name: "class".into(), value: Some("btn".into()), kind: AttrKind::Static });
    attrs.push(TemplateAttr { name: "value".into(), value: Some("42".into()), kind: AttrKind::Bind });
    attrs.push(TemplateAttr { name: "click".into(), value: Some("inc".into()), kind: AttrKind::On });

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
