use velox_dom::{h, text, Props, VNode};
use velox_style::{Stylesheet, apply_styles_with_hover};

#[test]
fn hover_selector_applies_conditionally() {
    let css = r#"
div:hover { color: blue; }
.btn:hover { background: yellow; }
"#;
    let ss = Stylesheet::parse(css);
    let vnode = h("div", Props::new().set("class", "btn"), vec![text("x")]);

    // Not hovered
    let styled = apply_styles_with_hover(&vnode, &ss, &|_, _| false);
    if let VNode::Element { props, .. } = styled { assert!(props.attrs.get("style").is_none()); }

    // Hovered
    let styled2 = apply_styles_with_hover(&vnode, &ss, &|tag, _| tag == "div");
    if let VNode::Element { props, .. } = styled2 {
        let style = props.attrs.get("style").unwrap();
        assert!(style.contains("color: blue;"));
        assert!(style.contains("background: yellow;"));
    } else { panic!("expected element"); }
}

