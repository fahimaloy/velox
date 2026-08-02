use velox_dom::{Props, VNode, h, text};
use velox_style::{Stylesheet, apply_styles};

#[test]
fn class_rule_applies_background() {
    let css = ".app { background: #1a1a2e; color: #e6edf3; }";
    let ss = Stylesheet::parse(css);
    assert_eq!(ss.rules.len(), 1, "expected one rule for .app");
    let node = h("div", Props::new().set("class", "app"), vec![]);
    let styled = apply_styles(&node, &ss);
    if let VNode::Element { props, .. } = &styled {
        let s = props.attrs.get("style").expect("style attr applied");
        assert!(
            s.contains("background") && s.contains("#1a1a2e"),
            "background not applied: {}",
            s
        );
    } else {
        panic!("expected element");
    }
}

#[test]
fn applies_tag_and_class_rules() {
    let css = r#"
div { color: red; }
.btn { font-weight: bold; }
"#;
    let ss = Stylesheet::parse(css);

    let vnode = h(
        "div",
        Props::new().set("class", "btn container"),
        vec![text("Click")],
    );

    let styled = apply_styles(&vnode, &ss);
    match styled {
        VNode::Element { props, .. } => {
            let style = props.attrs.get("style").expect("style present");
            // Both declarations applied
            assert!(style.contains("color: red;"));
            assert!(style.contains("font-weight: bold;"));
        }
        _ => panic!("expected element"),
    }
}

#[test]
fn class_overrides_tag_for_same_prop() {
    let css = r#"
div { color: blue; }
.btn { color: red; }
"#;
    let ss = Stylesheet::parse(css);
    let vnode = h("div", Props::new().set("class", "btn"), vec![]);
    let styled = apply_styles(&vnode, &ss);
    if let VNode::Element { props, .. } = styled {
        let style = props.attrs.get("style").unwrap();
        // Our simple merger creates deterministic order by key; just assert final value
        assert!(style.contains("color: red;"));
    } else {
        panic!("expected element");
    }
}

#[test]
fn children_receive_styles_recursively() {
    let css = r#"span { color: green; }"#;
    let ss = Stylesheet::parse(css);
    let vnode = h("div", Props::new(), vec![h("span", Props::new(), vec![])]);
    let styled = apply_styles(&vnode, &ss);
    if let VNode::Element { children, .. } = styled {
        if let VNode::Element { props, .. } = &children[0] {
            let style = props.attrs.get("style").unwrap();
            assert!(style.contains("color: green;"));
        } else {
            panic!("expected span element");
        }
    } else {
        panic!("expected div element");
    }
}
