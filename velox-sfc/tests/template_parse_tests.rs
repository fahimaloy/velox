use velox_sfc::{AttrKind, Node, parse_template_to_ast};

#[test]
fn parse_element_with_text() {
    let ast = parse_template_to_ast("<div>hi</div>").unwrap();
    assert_eq!(ast.len(), 1);
    match &ast[0] {
        Node::Element { tag, children, .. } => {
            assert_eq!(tag, "div");
            assert_eq!(children.len(), 1);
            assert!(matches!(children[0], Node::Text(_)));
        }
        _ => panic!("expected element"),
    }
}

#[test]
fn parse_text_and_interpolation() {
    let ast = parse_template_to_ast("<p>Hello {{name}}</p>").unwrap();
    match &ast[0] {
        Node::Element { children, .. } => {
            assert!(matches!(children[0], Node::Text(_)));
            assert!(matches!(children[1], Node::Interpolation(_)));
        }
        _ => panic!("expected element"),
    }
}

#[test]
fn parse_attrs_static_bind_event() {
    let ast =
        parse_template_to_ast(r#"<input class="x" :value="count" @input="onInput"/>"#).unwrap();
    match &ast[0] {
        Node::Element {
            attrs,
            self_closing,
            ..
        } => {
            assert!(*self_closing);
            assert_eq!(attrs.len(), 3);
            assert!(
                attrs
                    .iter()
                    .any(|a| a.kind == AttrKind::Static && a.name == "class")
            );
            assert!(
                attrs
                    .iter()
                    .any(|a| a.kind == AttrKind::Bind && a.name == "value")
            );
            assert!(
                attrs
                    .iter()
                    .any(|a| a.kind == AttrKind::On && a.name == "input")
            );
        }
        _ => panic!("expected element"),
    }
}
