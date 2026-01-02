use velox_sfc::{parse_template_to_ast, AttrKind, Node};

#[test]
fn parse_v_else_variants() {
    let tpl = r#"<div><p v-if="x">A</p><p v-else-if="y">B</p><p v-elseif="z">C</p><p v-else>D</p></div>"#;
    let ast = parse_template_to_ast(tpl).unwrap();
    // root div
    assert_eq!(ast.len(), 1);
    if let Node::Element { children, .. } = &ast[0] {
        // expect 4 child elements
        assert_eq!(children.len(), 4);
        // inspect attrs of each child
        for (i, ch) in children.iter().enumerate() {
            match ch {
                Node::Element { attrs, .. } => {
                    if i == 0 {
                        assert!(attrs.iter().any(|a| a.kind == AttrKind::Directive && a.name == "if"));
                    } else if i == 1 {
                        assert!(attrs.iter().any(|a| a.kind == AttrKind::Directive && a.name == "else-if"));
                    } else if i == 2 {
                        assert!(attrs.iter().any(|a| a.kind == AttrKind::Directive && a.name == "elseif" || a.name == "else-if"));
                    } else if i == 3 {
                        assert!(attrs.iter().any(|a| a.kind == AttrKind::Directive && a.name == "else"));
                    }
                }
                _ => panic!("expected element child"),
            }
        }
    } else { panic!("expected root element"); }
}
