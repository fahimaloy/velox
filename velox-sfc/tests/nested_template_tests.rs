use velox_sfc::{Node, compile_template_to_rs, parse_template_to_ast};

// =============================================================================
// Nested template parsing tests
// =============================================================================

#[test]
fn parse_deeply_nested_elements() {
    let ast = parse_template_to_ast(
        r#"<div><section><article><p><span>deep</span></p></article></section></div>"#,
    )
    .unwrap();
    assert_eq!(ast.len(), 1);
    match &ast[0] {
        Node::Element { tag, children, .. } => {
            assert_eq!(tag, "div");
            assert_eq!(children.len(), 1);
            // Walk down to "deep" text
            let mut node = &children[0];
            for expected in &["section", "article", "p", "span"] {
                match node {
                    Node::Element {
                        tag: t,
                        children: ch,
                        ..
                    } => {
                        assert_eq!(t, expected);
                        assert_eq!(ch.len(), 1);
                        node = &ch[0];
                    }
                    _ => panic!("expected element {}", expected),
                }
            }
            assert!(matches!(node, Node::Text(t) if t == "deep"));
        }
        _ => panic!("expected element"),
    }
}

#[test]
fn parse_sibling_elements() {
    let ast =
        parse_template_to_ast(r#"<div><span>a</span><span>b</span><span>c</span></div>"#).unwrap();
    match &ast[0] {
        Node::Element { children, .. } => {
            assert_eq!(children.len(), 3);
            for (i, child) in children.iter().enumerate() {
                match child {
                    Node::Element {
                        tag, children: ch, ..
                    } => {
                        assert_eq!(tag, "span");
                        assert_eq!(ch.len(), 1);
                        assert!(matches!(&ch[0], Node::Text(t) if t == ["a", "b", "c"][i]));
                    }
                    _ => panic!("expected span"),
                }
            }
        }
        _ => panic!("expected element"),
    }
}

#[test]
fn parse_mixed_text_and_elements() {
    let ast =
        parse_template_to_ast(r#"<p>Hello <strong>world</strong> and <em>more</em>!</p>"#).unwrap();
    match &ast[0] {
        Node::Element { children, .. } => {
            // "Hello ", <strong>world</strong>, " and ", <em>more</em>, "!"
            assert_eq!(children.len(), 5);
            assert!(matches!(&children[0], Node::Text(t) if t.trim() == "Hello"));
            assert!(matches!(&children[1], Node::Element { tag, .. } if tag == "strong"));
            assert!(matches!(&children[3], Node::Element { tag, .. } if tag == "em"));
        }
        _ => panic!("expected element"),
    }
}

#[test]
fn parse_interpolation_in_nested_elements() {
    let ast = parse_template_to_ast(r#"<div><p>{{ title }}</p><ul><li>{{ item }}</li></ul></div>"#)
        .unwrap();
    match &ast[0] {
        Node::Element { children, .. } => {
            assert_eq!(children.len(), 2);
            // First child: <p>{{ title }}</p>
            match &children[0] {
                Node::Element {
                    children: p_children,
                    ..
                } => {
                    assert_eq!(p_children.len(), 1);
                    assert!(matches!(&p_children[0], Node::Interpolation(e) if e == "title"));
                }
                _ => panic!("expected p element"),
            }
            // Second child: <ul><li>{{ item }}</li></ul>
            match &children[1] {
                Node::Element {
                    children: ul_children,
                    ..
                } => {
                    assert_eq!(ul_children.len(), 1);
                    match &ul_children[0] {
                        Node::Element {
                            children: li_children,
                            ..
                        } => {
                            assert_eq!(li_children.len(), 1);
                            assert!(
                                matches!(&li_children[0], Node::Interpolation(e) if e == "item")
                            );
                        }
                        _ => panic!("expected li element"),
                    }
                }
                _ => panic!("expected ul element"),
            }
        }
        _ => panic!("expected root element"),
    }
}

#[test]
fn parse_self_closing_in_nested() {
    let ast = parse_template_to_ast(r#"<div><input/><br/><img src="x"/></div>"#).unwrap();
    match &ast[0] {
        Node::Element { children, .. } => {
            assert_eq!(children.len(), 3);
            for (i, expected) in ["input", "br", "img"].iter().enumerate() {
                match &children[i] {
                    Node::Element {
                        tag, self_closing, ..
                    } => {
                        assert_eq!(tag, expected);
                        assert!(*self_closing);
                    }
                    _ => panic!("expected self-closing element"),
                }
            }
        }
        _ => panic!("expected element"),
    }
}

// =============================================================================
// Nested template codegen tests
// =============================================================================

#[test]
fn codegen_nested_elements_with_interpolations() {
    let rs = compile_template_to_rs(
        r#"<div><h1>{{ title }}</h1><p>{{ body }}</p></div>"#,
        "App",
        None,
    )
    .unwrap();
    assert!(rs.contains(r#"h("div""#));
    assert!(rs.contains(r#"h("h1""#));
    assert!(rs.contains(r#"h("p""#));
    assert!(rs.contains(r#"resolve("title")"#));
    assert!(rs.contains(r#"resolve("body")"#));
}

#[test]
fn codegen_deep_nesting_preserves_structure() {
    let rs = compile_template_to_rs(
        r#"<div><section><article><p>deep text</p></article></section></div>"#,
        "App",
        None,
    )
    .unwrap();
    assert!(rs.contains(r#"h("div""#));
    assert!(rs.contains(r#"h("section""#));
    assert!(rs.contains(r#"h("article""#));
    assert!(rs.contains(r#"h("p""#));
    assert!(rs.contains(r#"text("deep text")"#));
}

#[test]
fn codegen_nested_with_attrs() {
    let rs = compile_template_to_rs(
        r#"<div class="outer"><span class="inner" :data-x="val">text</span></div>"#,
        "App",
        None,
    )
    .unwrap();
    assert!(rs.contains(r#".set("class", "outer")"#));
    assert!(rs.contains(r#".set("class", "inner")"#));
    assert!(rs.contains(r#".set("data-x""#));
}

#[test]
fn codegen_multiple_interpolations_in_single_element() {
    let rs = compile_template_to_rs(r#"<p>{{ first }} {{ middle }} {{ last }}</p>"#, "App", None)
        .unwrap();
    assert!(rs.contains(r#"resolve("first")"#));
    assert!(rs.contains(r#"resolve("middle")"#));
    assert!(rs.contains(r#"resolve("last")"#));
}

// =============================================================================
// Error/edge case tests for templates
// =============================================================================

#[test]
fn parse_empty_template() {
    let ast = parse_template_to_ast("").unwrap();
    assert!(ast.is_empty());
}

#[test]
fn parse_whitespace_only_template() {
    let ast = parse_template_to_ast("   \n\n   ").unwrap();
    // Whitespace-only text nodes are filtered out
    assert!(ast.is_empty());
}

#[test]
fn parse_single_text_node() {
    let ast = parse_template_to_ast("hello world").unwrap();
    assert_eq!(ast.len(), 1);
    assert!(matches!(&ast[0], Node::Text(t) if t.contains("hello")));
}

#[test]
fn parse_multiple_root_elements() {
    let ast = parse_template_to_ast(r#"<div>a</div><span>b</span>"#).unwrap();
    assert_eq!(ast.len(), 2);
    assert!(matches!(&ast[0], Node::Element { tag, .. } if tag == "div"));
    assert!(matches!(&ast[1], Node::Element { tag, .. } if tag == "span"));
}

#[test]
fn codegen_empty_template() {
    let rs = compile_template_to_rs("", "App", None).unwrap();
    assert!(rs.contains(r#"text("")"#));
}

#[test]
fn codegen_text_only_template() {
    let rs = compile_template_to_rs("just text", "App", None).unwrap();
    assert!(rs.contains(r#"text("just text")"#) || rs.contains(r#"text(" just text")"#));
}

#[test]
fn parse_multiple_interpolations_adjacent() {
    let ast = parse_template_to_ast(r#"<p>{{a}}{{b}}{{c}}</p>"#).unwrap();
    match &ast[0] {
        Node::Element { children, .. } => {
            assert_eq!(children.len(), 3);
            assert!(matches!(&children[0], Node::Interpolation(e) if e == "a"));
            assert!(matches!(&children[1], Node::Interpolation(e) if e == "b"));
            assert!(matches!(&children[2], Node::Interpolation(e) if e == "c"));
        }
        _ => panic!("expected element"),
    }
}

#[test]
fn parse_special_characters_in_text() {
    let ast = parse_template_to_ast(r#"<div>Price: $100 & tax</div>"#).unwrap();
    match &ast[0] {
        Node::Element { children, .. } => {
            assert_eq!(children.len(), 1);
            assert!(matches!(&children[0], Node::Text(t) if t.contains("Price")));
        }
        _ => panic!("expected element"),
    }
}

#[test]
fn codegen_nested_v_if() {
    let rs = compile_template_to_rs(
        r#"<div><p v-if="showA">A</p><span v-if="showB">B</span></div>"#,
        "App",
        None,
    )
    .unwrap();
    // Should have two separate if blocks
    assert!(rs.contains("if "));
}
