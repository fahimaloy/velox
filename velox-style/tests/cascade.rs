use velox_dom::{Props, VNode, h};
use velox_style::{Stylesheet, ua::ua_sheet};

fn get_style(node: &VNode) -> Option<String> {
    if let VNode::Element { props, .. } = node {
        props.attrs.get("style").cloned()
    } else {
        None
    }
}

#[test]
fn ua_body_has_8px_margin_and_h1_has_em_margin() {
    let ua = ua_sheet();
    // body should contribute 8px even with empty author sheet
    let has_body_margin = ua.rules.iter().any(|r| {
        let is_body = r.selector.parts.iter().any(|p| p.tag == "body");
        let has_margin_8px = r
            .decls
            .iter()
            .any(|(k, v)| k == "margin" && v.contains("8px"));
        is_body && has_margin_8px
    });
    assert!(
        has_body_margin,
        "UA sheet should contain body {{ margin: 8px }} rule, got: {:?}",
        ua.rules
    );

    // h1 should have em-based margin
    let has_h1_margin = ua.rules.iter().any(|r| {
        let is_h1 = r.selector.parts.iter().any(|p| p.tag == "h1");
        let has_em_margin = r
            .decls
            .iter()
            .any(|(k, v)| k == "margin" && v.contains("0.67em"));
        is_h1 && has_em_margin
    });
    assert!(
        has_h1_margin,
        "UA sheet should contain h1 margin 0.67em, got: {:?}",
        ua.rules
    );
}

#[test]
fn apply_respects_ua_lt_author_lt_inline() {
    let author = Stylesheet::parse("p{ margin: 2px }");
    // inline margin: 9px should win over author 2px and UA 1em
    let node = h("p", Props::new().set("style", "margin: 9px"), vec![]);
    let out = velox_style::apply_with_cascade(&node, &author);
    let style = get_style(&out).expect("styled node should have style attr");
    // inline wins — should contain 9px, not 2px nor 1em
    assert!(
        style.contains("9px"),
        "inline margin 9px should win, got style: {}",
        style
    );
    // Ensure author value is not present as final value (could be present as substring if not overridden, but 2px should be overridden)
    // Since merge keeps later keys overriding earlier, 9px should be present and 2px should not leak as separate key
    // For "margin" shorthand, UA and author both set "margin", inline overrides with same key.
    assert!(
        !style.contains("margin: 2px"),
        "author margin 2px should be overridden by inline 9px, got: {}",
        style
    );
}

#[test]
fn author_overrides_ua_when_no_inline() {
    let author = Stylesheet::parse("p{ margin: 2px }");
    let node = h("p", Props::new(), vec![]);
    let out = velox_style::apply_with_cascade(&node, &author);
    let style = get_style(&out).expect("style present");
    assert!(
        style.contains("2px"),
        "author 2px should override UA 1em when no inline, got: {}",
        style
    );
}

#[test]
fn ua_applies_body_margin_with_empty_author() {
    let empty = Stylesheet::parse("");
    let node = h("body", Props::new(), vec![]);
    let out = velox_style::apply_with_cascade(&node, &empty);
    let style = get_style(&out).expect("body should have style from UA");
    assert!(
        style.contains("8px"),
        "body should have UA margin 8px, got: {}",
        style
    );
}
