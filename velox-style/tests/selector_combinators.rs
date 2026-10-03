//! Selector-matching regressions and child-combinator behavior.
//!
//! Guards the contract consumed by Task 9: descendant selectors keep matching
//! any ancestor, `>` matches exactly one level up, and existing selector
//! shapes (tag, class, `:hover`, `[data-v-*]` attributes) keep matching after
//! the combinator-aware rewrite of `parse_selector_list`/`matches_selector`.

use velox_dom::{Props, VNode, h, text};
use velox_style::{Stylesheet, apply_styles, apply_styles_with_hover};

fn style_of(node: &VNode) -> Option<String> {
    match node {
        VNode::Element { props, .. } => props.attrs.get("style").cloned(),
        _ => None,
    }
}

fn styled_has(node: &VNode, needle: &str) -> bool {
    style_of(node).is_some_and(|s| s.contains(needle))
}

// ---------------------------------------------------------------------------
// Regressions: pre-existing selector shapes must keep matching
// ---------------------------------------------------------------------------

#[test]
fn descendant_matches_any_ancestor() {
    let ss = Stylesheet::parse(".header h1 { color: red }");
    // h1 two levels under .header must still match (walk-all-ancestors).
    let tree = h(
        "div",
        Props::new().set("class", "header"),
        vec![h(
            "section",
            Props::new(),
            vec![h("h1", Props::new(), vec![text("t")])],
        )],
    );
    let styled = apply_styles(&tree, &ss);
    let VNode::Element { children, .. } = &styled else {
        panic!("expected element");
    };
    let VNode::Element { children, .. } = &children[0] else {
        panic!("expected section");
    };
    assert!(
        styled_has(&children[0], "color: red"),
        "descendant selector should match at depth 2: {:?}",
        style_of(&children[0])
    );
}

#[test]
fn tag_and_class_rules_still_match() {
    let ss = Stylesheet::parse("div { color: blue } .btn { font-weight: bold }");
    let node = h("div", Props::new().set("class", "btn"), vec![]);
    let styled = apply_styles(&node, &ss);
    let style = style_of(&styled).expect("style attr");
    assert!(
        style.contains("color: blue") && style.contains("font-weight: bold"),
        "tag and class rules should both apply: {style}"
    );
}

#[test]
fn hover_rule_still_applies_conditionally() {
    let ss = Stylesheet::parse("button:hover { background: yellow }");
    let node = h("button", Props::new(), vec![]);

    let idle = apply_styles_with_hover(&node, &ss, &|_, _| false);
    assert!(
        !styled_has(&idle, "background: yellow"),
        "hover rule must not apply when not hovered"
    );

    let hovered = apply_styles_with_hover(&node, &ss, &|tag, _| tag == "button");
    assert!(
        styled_has(&hovered, "background: yellow"),
        "hover rule must apply when hovered"
    );
}

#[test]
fn scoped_attribute_selector_still_matches() {
    let ss = Stylesheet::parse(".btn[data-v-abc] { color: red }");
    let node = h(
        "div",
        Props::new().set("class", "btn").set("data-v-abc", ""),
        vec![],
    );
    let styled = apply_styles(&node, &ss);
    assert!(styled_has(&styled, "color: red"));

    let untagged = h("div", Props::new().set("class", "btn"), vec![]);
    let styled2 = apply_styles(&untagged, &ss);
    assert!(!styled_has(&styled2, "color: red"));
}

// ---------------------------------------------------------------------------
// Child combinator: exactly one level up
// ---------------------------------------------------------------------------

#[test]
fn child_combinator_matches_direct_child_only() {
    let ss = Stylesheet::parse("div > .card { color: red }");

    // Direct child: matches.
    let direct = h(
        "div",
        Props::new(),
        vec![h("span", Props::new().set("class", "card"), vec![])],
    );
    let styled = apply_styles(&direct, &ss);
    let VNode::Element { children, .. } = &styled else {
        panic!("expected element");
    };
    assert!(
        styled_has(&children[0], "color: red"),
        "direct child must match `div > .card`"
    );

    // Grandchild: must NOT match.
    let grandchild = h(
        "div",
        Props::new(),
        vec![h(
            "section",
            Props::new(),
            vec![h("span", Props::new().set("class", "card"), vec![])],
        )],
    );
    let styled2 = apply_styles(&grandchild, &ss);
    let VNode::Element { children, .. } = &styled2 else {
        panic!("expected element");
    };
    let VNode::Element { children, .. } = &children[0] else {
        panic!("expected section");
    };
    assert!(
        !styled_has(&children[0], "color: red"),
        "element two levels down must NOT match `div > .card`"
    );
}

#[test]
fn unspaced_child_combinator_matches_direct_child_only() {
    // `div>.card` (no spaces) must parse to the same child relationship.
    let ss = Stylesheet::parse("div>.card { color: red }");
    let direct = h(
        "div",
        Props::new(),
        vec![h("span", Props::new().set("class", "card"), vec![])],
    );
    let styled = apply_styles(&direct, &ss);
    let VNode::Element { children, .. } = &styled else {
        panic!("expected element");
    };
    assert!(
        styled_has(&children[0], "color: red"),
        "direct child must match `div>.card`"
    );
}

#[test]
fn scoped_child_selector_output_matches_end_to_end() {
    // The exact selector text that velox-sfc's scope_css now produces must
    // match the corresponding scoped tree.
    let css = "div[data-v-abc] > .card[data-v-abc] { color: red }";
    let ss = Stylesheet::parse(css);

    let scoped_tree = h(
        "div",
        Props::new().set("data-v-abc", ""),
        vec![h(
            "span",
            Props::new().set("class", "card").set("data-v-abc", ""),
            vec![],
        )],
    );
    let styled = apply_styles(&scoped_tree, &ss);
    let VNode::Element { children, .. } = &styled else {
        panic!("expected element");
    };
    assert!(
        styled_has(&children[0], "color: red"),
        "scoped child selector must match the scoped direct child"
    );

    // Same tree with an extra level between div and .card: no match.
    let deep = h(
        "div",
        Props::new().set("data-v-abc", ""),
        vec![h(
            "section",
            Props::new().set("data-v-abc", ""),
            vec![h(
                "span",
                Props::new().set("class", "card").set("data-v-abc", ""),
                vec![],
            )],
        )],
    );
    let styled2 = apply_styles(&deep, &ss);
    let VNode::Element { children, .. } = &styled2 else {
        panic!("expected element");
    };
    let VNode::Element { children, .. } = &children[0] else {
        panic!("expected section");
    };
    assert!(
        !styled_has(&children[0], "color: red"),
        "scoped child selector must not match past one level"
    );
}

// ---------------------------------------------------------------------------
// Chains: descendant walks all ancestors, child pins exactly one level
// ---------------------------------------------------------------------------

#[test]
fn three_part_descendant_chain_matches_in_order() {
    // `.a .b .c` — the leftmost part must match an ancestor of the middle
    // part's match (the previous farthest-first walker got this wrong).
    let ss = Stylesheet::parse("div .mid .leaf { color: green }");
    let tree = h(
        "div",
        Props::new(),
        vec![h(
            "p",
            Props::new().set("class", "mid"),
            vec![h("span", Props::new().set("class", "leaf"), vec![])],
        )],
    );
    let styled = apply_styles(&tree, &ss);
    let VNode::Element { children, .. } = &styled else {
        panic!("expected element");
    };
    let VNode::Element { children, .. } = &children[0] else {
        panic!("expected p.mid");
    };
    assert!(
        styled_has(&children[0], "color: green"),
        "three-part descendant chain must match: {:?}",
        style_of(&children[0])
    );
}

#[test]
fn child_then_descendant_chain_matches() {
    // `div > .mid .leaf`: .leaf anywhere under .mid, .mid exactly one level
    // under div.
    let ss = Stylesheet::parse("div > .mid .leaf { color: green }");

    let mid_is_direct_child = h(
        "div",
        Props::new(),
        vec![h(
            "p",
            Props::new().set("class", "mid"),
            vec![h(
                "section",
                Props::new(),
                vec![h("span", Props::new().set("class", "leaf"), vec![])],
            )],
        )],
    );
    let styled = apply_styles(&mid_is_direct_child, &ss);
    let VNode::Element { children, .. } = &styled else {
        panic!("expected element");
    };
    let VNode::Element { children, .. } = &children[0] else {
        panic!("expected p.mid");
    };
    let VNode::Element { children, .. } = &children[0] else {
        panic!("expected section");
    };
    assert!(
        styled_has(&children[0], "color: green"),
        ".leaf under direct-child .mid must match"
    );

    // .mid two levels under div: the child combinator must reject it.
    let mid_too_deep = h(
        "div",
        Props::new(),
        vec![h(
            "section",
            Props::new(),
            vec![h(
                "p",
                Props::new().set("class", "mid"),
                vec![h("span", Props::new().set("class", "leaf"), vec![])],
            )],
        )],
    );
    let styled2 = apply_styles(&mid_too_deep, &ss);
    let VNode::Element { children, .. } = &styled2 else {
        panic!("expected element");
    };
    let VNode::Element { children, .. } = &children[0] else {
        panic!("expected section");
    };
    let VNode::Element { children, .. } = &children[0] else {
        panic!("expected p.mid");
    };
    assert!(
        !styled_has(&children[0], "color: green"),
        ".leaf under non-direct-child .mid must NOT match"
    );
}

#[test]
fn leading_child_combinator_is_dropped_as_invalid() {
    // `> .card` has no left compound: the rule is invalid CSS and must not
    // silently degrade to a bare `.card` rule.
    let ss = Stylesheet::parse("> .card { color: red }");
    assert!(
        ss.rules.is_empty(),
        "invalid leading-child selector must drop the rule, got {:?}",
        ss.rules
    );
}
