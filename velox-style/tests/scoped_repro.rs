use velox_dom::{Props, VNode, h, text};
use velox_style::{Stylesheet, apply_styles};

#[test]
fn scoped_btn_data_v_matches() {
    let css = ".btn[data-v-abc] { color: red; }";
    let ss = Stylesheet::parse(css);
    assert_eq!(ss.rules.len(), 1, "scoped rule should parse");
    // With class btn + data-v-abc="" should match
    let node = h(
        "div",
        Props::new().set("class", "btn").set("data-v-abc", ""),
        vec![text("hi")],
    );
    let styled = apply_styles(&node, &ss);
    if let VNode::Element { props, .. } = &styled {
        let s = props.attrs.get("style").expect("style should be present");
        assert!(s.contains("color: red"), "expected scoped match, got: {}", s);
    } else {
        panic!("expected element");
    }
    // Without data-v-abc should NOT match
    let node2 = h("div", Props::new().set("class", "btn"), vec![]);
    let styled2 = apply_styles(&node2, &ss);
    if let VNode::Element { props, .. } = &styled2 {
        let no_match = props.attrs.get("style").is_none()
            || !props.attrs.get("style").unwrap().contains("color: red");
        assert!(no_match, "should not match without data attr, got {:?}", props.attrs.get("style"));
    }
}

#[test]
fn scoped_media_prelude_not_scoped_inner_only() {
    let css = "@media (max-width: 600px) { .a { color: blue; } } .b { color: green; }";
    let ss = Stylesheet::parse(css);
    assert_eq!(ss.rules.len(), 2, "media inner + top-level =2, got {}", ss.rules.len());
    let node_a = h("div", Props::new().set("class", "a"), vec![]);
    let styled_a = apply_styles(&node_a, &ss);
    if let VNode::Element { props, .. } = styled_a {
        let s = props.attrs.get("style").unwrap();
        assert!(s.contains("color: blue"), "media inner should apply, got {}", s);
    }
}

#[test]
fn scoped_attr_value_and_universal() {
    let css = r#".btn[data-v-abc=""] { background: red; } [data-v-xyz] { color: green; }"#;
    let ss = Stylesheet::parse(css);
    assert_eq!(ss.rules.len(), 2);
    let node = h("div", Props::new().set("class", "btn").set("data-v-abc", ""), vec![]);
    let styled = apply_styles(&node, &ss);
    if let VNode::Element { props, .. } = &styled {
        assert!(props.attrs.get("style").unwrap().contains("background: red"));
    }
    let node2 = h("span", Props::new().set("data-v-xyz", ""), vec![]);
    let styled2 = apply_styles(&node2, &ss);
    if let VNode::Element { props, .. } = &styled2 {
        assert!(props.attrs.get("style").unwrap().contains("color: green"));
    }
}
