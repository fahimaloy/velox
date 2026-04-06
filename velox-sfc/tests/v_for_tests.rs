use velox_sfc::{AttrKind, Node, parse_template_to_ast};

// =============================================================================
// v-for directive parsing tests
// =============================================================================

#[test]
fn parse_v_for_simple_collection() {
    let ast = parse_template_to_ast(r#"<li v-for="item in items">{{ item }}</li>"#).unwrap();
    assert_eq!(ast.len(), 1);
    match &ast[0] {
        Node::Element { attrs, .. } => {
            assert!(
                attrs
                    .iter()
                    .any(|a| a.kind == AttrKind::Directive && a.name == "for")
            );
            let for_attr = attrs.iter().find(|a| a.name == "for").unwrap();
            assert_eq!(for_attr.value.as_deref(), Some("item in items"));
        }
        _ => panic!("expected element"),
    }
}

#[test]
fn parse_v_for_with_index() {
    let ast = parse_template_to_ast(
        r#"<li v-for="(item, index) in items">{{ item }} - {{ index }}</li>"#,
    )
    .unwrap();
    match &ast[0] {
        Node::Element { attrs, .. } => {
            let for_attr = attrs.iter().find(|a| a.name == "for").unwrap();
            assert_eq!(for_attr.value.as_deref(), Some("(item, index) in items"));
        }
        _ => panic!("expected element"),
    }
}

#[test]
fn parse_v_for_with_key_attr() {
    let ast = parse_template_to_ast(
        r#"<div v-for="item in items" :key="item.id"><span>{{ item }}</span></div>"#,
    )
    .unwrap();
    match &ast[0] {
        Node::Element { attrs, .. } => {
            assert!(
                attrs
                    .iter()
                    .any(|a| a.kind == AttrKind::Directive && a.name == "for")
            );
            assert!(
                attrs
                    .iter()
                    .any(|a| a.kind == AttrKind::Bind && a.name == "key")
            );
            let key_attr = attrs.iter().find(|a| a.name == "key").unwrap();
            assert_eq!(key_attr.value.as_deref(), Some("item.id"));
        }
        _ => panic!("expected element"),
    }
}

#[test]
fn parse_v_for_nested() {
    let ast = parse_template_to_ast(
        r#"<div v-for="row in rows"><span v-for="col in row.cols">{{ col }}</span></div>"#,
    )
    .unwrap();
    match &ast[0] {
        Node::Element { children, .. } => {
            assert_eq!(children.len(), 1);
            match &children[0] {
                Node::Element { attrs, .. } => {
                    assert!(
                        attrs
                            .iter()
                            .any(|a| a.kind == AttrKind::Directive && a.name == "for")
                    );
                }
                _ => panic!("expected nested element"),
            }
        }
        _ => panic!("expected element"),
    }
}

#[test]
fn parse_v_for_with_numeric_count() {
    let ast = parse_template_to_ast(r#"<span v-for="i in 5">{{ i }}</span>"#).unwrap();
    match &ast[0] {
        Node::Element { attrs, .. } => {
            let for_attr = attrs.iter().find(|a| a.name == "for").unwrap();
            assert_eq!(for_attr.value.as_deref(), Some("i in 5"));
        }
        _ => panic!("expected element"),
    }
}

#[test]
fn parse_v_for_with_text_content() {
    let ast = parse_template_to_ast(r#"<p v-for="name in names">Hello {{ name }}!</p>"#).unwrap();
    match &ast[0] {
        Node::Element { children, .. } => {
            // Should have Text("Hello "), Interpolation("name"), Text("!")
            assert_eq!(children.len(), 3);
            assert!(matches!(&children[0], Node::Text(t) if t == "Hello "));
            assert!(matches!(&children[1], Node::Interpolation(e) if e == "name"));
            assert!(matches!(&children[2], Node::Text(t) if t == "!"));
        }
        _ => panic!("expected element"),
    }
}

#[test]
fn parse_v_for_on_different_tags() {
    // v-for should work on any element type
    let tags = ["ul", "table", "select", "option", "tr", "td", "svg", "path"];
    for tag in tags {
        let src = format!(r#"<{tag} v-for="item in items">{{{{ item }}}}</{tag}>"#);
        let ast = parse_template_to_ast(&src).unwrap();
        assert_eq!(ast.len(), 1);
        match &ast[0] {
            Node::Element { tag: t, attrs, .. } => {
                assert_eq!(t, tag);
                assert!(
                    attrs
                        .iter()
                        .any(|a| a.kind == AttrKind::Directive && a.name == "for")
                );
            }
            _ => panic!("expected element for tag {}", tag),
        }
    }
}

// =============================================================================
// v-for codegen tests
// =============================================================================

use velox_sfc::compile_template_to_rs;

#[test]
fn codegen_v_for_generates_loop() {
    let rs = compile_template_to_rs(
        r#"<div v-for="item in items"><span>{{ item }}</span></div>"#,
        "App",
        None,
    )
    .unwrap();
    assert!(rs.contains("__for_count"));
    assert!(rs.contains("0..__for_count"));
    assert!(rs.contains("resolve(\"items\")"));
}

#[test]
fn codegen_v_for_with_key_removes_key_from_props() {
    let rs = compile_template_to_rs(
        r#"<div v-for="item in items" :key="item.id">{{ item.name }}</div>"#,
        "App",
        None,
    )
    .unwrap();
    // :key should NOT be emitted as a .set("key", ...) call
    assert!(!rs.contains(r#".set("key""#));
    // But the loop structure should still be there
    assert!(rs.contains("__for_count"));
}

#[test]
fn codegen_v_for_with_destructuring_uses_index_name() {
    let rs = compile_template_to_rs(
        r#"<div v-for="(item, idx) in items">{{ idx }}: {{ item }}</div>"#,
        "App",
        None,
    )
    .unwrap();
    assert!(rs.contains("for idx in 0..__for_count"));
}

#[test]
fn codegen_v_for_numeric_count_generates_loop() {
    let rs =
        compile_template_to_rs(r#"<div v-for="i in 10">Item {{ i }}</div>"#, "App", None).unwrap();
    assert!(rs.contains("__for_count"));
    assert!(rs.contains("0..__for_count"));
}

#[test]
fn codegen_v_for_nested_loops() {
    let rs = compile_template_to_rs(
        r#"<div v-for="row in rows"><span v-for="col in row">{{ col }}</span></div>"#,
        "App",
        None,
    )
    .unwrap();
    // Should have two loop structures
    let for_count = rs.matches("__for_count").count();
    assert!(
        for_count >= 2,
        "expected at least 2 __for_count occurrences, found {}",
        for_count
    );
}

#[test]
fn codegen_v_for_with_v_if_combination() {
    let rs = compile_template_to_rs(
        r#"<div v-for="item in items" v-if="item.active">{{ item.name }}</div>"#,
        "App",
        None,
    )
    .unwrap();
    assert!(rs.contains("__for_count"));
    assert!(rs.contains("if ("));
}

#[test]
fn codegen_v_for_with_attrs() {
    let rs = compile_template_to_rs(
        r#"<li v-for="item in items" class="list-item" :data-id="item.id">{{ item }}</li>"#,
        "App",
        None,
    )
    .unwrap();
    assert!(rs.contains(r#".set("class""#));
    assert!(rs.contains("__for_count"));
}

// =============================================================================
// v-for error/edge cases
// =============================================================================

#[test]
fn parse_v_for_missing_in_keyword() {
    // Should still parse but directive value won't have "in"
    let ast = parse_template_to_ast(r#"<div v-for="items">test</div>"#);
    // Parser should succeed; the codegen handles missing "in" gracefully
    assert!(ast.is_ok());
}

#[test]
fn parse_v_for_empty_value() {
    let ast = parse_template_to_ast(r#"<div v-for="">test</div>"#);
    assert!(ast.is_ok());
}

#[test]
fn codegen_v_for_standalone_template() {
    // v-for as the only content in template
    let rs = compile_template_to_rs(r#"<span v-for="n in numbers">{{ n }}</span>"#, "App", None)
        .unwrap();
    assert!(rs.contains("__for_count"));
}

#[test]
fn codegen_v_for_with_dot_notation_in_interpolation() {
    let rs = compile_template_to_rs(
        r#"<div v-for="user in users"><p>{{ user.name }}: {{ user.email }}</p></div>"#,
        "App",
        None,
    )
    .unwrap();
    // Should use indexed access pattern
    assert!(rs.contains("users[{}].name") || rs.contains("users[{}]"));
}
