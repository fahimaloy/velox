use velox_sfc::{AttrKind, Node, parse_template_to_ast};

#[test]
fn parse_v_if_directive() {
    let ast = parse_template_to_ast(r#"<div v-if="show">Hello</div>"#).unwrap();
    assert_eq!(ast.len(), 1);
    match &ast[0] {
        Node::Element { attrs, children, .. } => {
            assert_eq!(children.len(), 1);
            assert!(attrs.iter().any(|a| a.kind == AttrKind::Directive && a.name == "if"));
        }
        _ => panic!("expected element"),
    }
}
