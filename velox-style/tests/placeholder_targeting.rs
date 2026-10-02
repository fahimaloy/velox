//! `::placeholder` rules must be routed by the selector's SUBJECT, not by
//! "does the last part happen to be a placeholder".
//!
//! # The defect this file pins
//!
//! `::placeholder` declarations reach the screen through a different attribute
//! than the element's own (`PLACEHOLDER_STYLE_ATTR`), because an `<input>`'s
//! own `style` is what the painter reads the VALUE's colour and geometry from.
//! The routing decision therefore has real consequences, and it used to be made
//! by asking a single question: "is the LAST part a placeholder?"
//!
//! That is the wrong question. A pseudo-element is not a node — it is the string
//! drawn inside an empty control — so CSS only ever lets it appear as the
//! selector's SUBJECT. `.field::placeholder .row` therefore matches NOTHING in
//! a browser, because there is no node for `.field::placeholder` to be an
//! ancestor of. Velox, keying off the last part, saw `.row` as the subject,
//! found no placeholder flag on it, and dropped `.field::placeholder`'s
//! declarations into `.row`'s OWN `style` — repainting an element the author
//! never named, in a way that looks like the rule was honoured.
//!
//! # The shapes
//!
//! Accepted — `::placeholder` is the subject:
//!   * `input::placeholder`
//!   * `.field::placeholder`
//!   * `.wrap input::placeholder` (placeholder subject; `.wrap` is a chain link)
//!
//! Rejected — `::placeholder` is a link in the chain, so there is no subject
//! left to attach the declarations to:
//!   * `.field::placeholder .row`
//!   * `.field::placeholder > .row`
//!   * `.a::placeholder .b::placeholder` (placeholder on both a chain link and
//!     the subject — the chain link alone disqualifies it)

use velox_dom::{Props, VNode, h};
use velox_style::{PLACEHOLDER_STYLE_ATTR, Stylesheet, apply_with_cascade};

/// The post-cascade props of the first element carrying `class`.
fn by_class(tree: &VNode, class: &str) -> Props {
    fn walk(node: &VNode, class: &str) -> Option<Props> {
        match node {
            VNode::Text(_) => None,
            VNode::Element {
                props, children, ..
            } => {
                if props.attrs.get("class").is_some_and(|c| c == class) {
                    return Some(props.clone());
                }
                children.iter().find_map(|c| walk(c, class))
            }
        }
    }
    walk(tree, class)
        .unwrap_or_else(|| panic!("no element with class {class:?} in the styled tree"))
}

fn own_style(p: &Props) -> Option<&str> {
    p.attrs.get("style").map(String::as_str)
}

fn placeholder_style(p: &Props) -> Option<&str> {
    p.attrs.get(PLACEHOLDER_STYLE_ATTR).map(String::as_str)
}

/// A tree in which `span.b` is a DESCENDANT of `input.a`, and `input.a` is an
/// `<input>` showing its placeholder — so `input.a::placeholder <chain> .b`
/// really does match `span.b`.
///
/// Nesting matters here: `apply_rec` hands `matches_selector` an ancestor list
/// whose first entry is the element itself, so a sibling is never reachable as
/// a chain link and a flat fixture would silently test nothing.
fn nested() -> VNode {
    h(
        "input",
        Props::new().set("class", "a").set("placeholder", "hint"),
        vec![h("span", Props::new().set("class", "b"), vec![])],
    )
}

// ===== accepted shapes: the placeholder attribute carries them ================

#[test]
fn a_bare_input_placeholder_lands_on_the_placeholder_attribute() {
    let tree = h(
        "div",
        Props::new(),
        vec![h(
            "input",
            Props::new().set("class", "f").set("placeholder", "hint"),
            vec![],
        )],
    );
    let sheet = Stylesheet::parse("input::placeholder { color: #ff0000; }");
    let styled = apply_with_cascade(&tree, &sheet);
    let field = by_class(&styled, "f");
    assert_eq!(placeholder_style(&field), Some("color: #ff0000;"));
    assert!(!own_style(&field).is_some_and(|s| s.contains("ff0000")));
}

#[test]
fn a_placeholder_rule_can_have_a_descendant_chain_before_its_subject() {
    // `.wrap` is a chain LINK; `input::placeholder` is the SUBJECT. This is
    // real CSS — the placeholder of an input inside `.wrap` — so it must stay
    // accepted. A fix that required the placeholder part to be the ONLY part
    // would reject it, and that regression is what this test exists to stop.
    let tree = h(
        "div",
        Props::new().set("class", "wrap"),
        vec![h("input", Props::new().set("placeholder", "hint"), vec![])],
    );
    let sheet = Stylesheet::parse(".wrap input::placeholder { color: #ff0000; }");
    let styled = apply_with_cascade(&tree, &sheet);
    let VNode::Element { children, .. } = &styled else {
        panic!("expected an element")
    };
    let VNode::Element { props, .. } = &children[0] else {
        panic!("expected an element")
    };
    assert_eq!(
        placeholder_style(props),
        Some("color: #ff0000;"),
        "`.wrap input::placeholder` styles the placeholder of an input inside \
         `.wrap`; the chain link must not disqualify the subject"
    );
    assert!(
        !own_style(props).is_some_and(|s| s.contains("ff0000")),
        "the placeholder colour must not land on the input's own `style`, \
         which is the VALUE text's style"
    );
}

// ===== rejected shapes: the declarations reach NOBODY ========================

#[test]
fn a_placeholder_as_a_descendant_link_does_not_repaint_the_descendant() {
    // The load-bearing assertion. Before the fix this landed `color: #ff0000`
    // on `span.b`'s OWN style.
    let sheet = Stylesheet::parse(".a::placeholder .b { color: #ff0000; }");
    let styled = apply_with_cascade(&nested(), &sheet);
    let b = by_class(&styled, "b");
    assert!(
        !own_style(&b).is_some_and(|s| s.contains("ff0000")),
        "`.a::placeholder .b` matched no element in CSS — there is no node for \
         `.a::placeholder` to be an ancestor of — so it must not paint `.b`. \
         Got style {:?}",
        own_style(&b)
    );
}

/// The same selector without the placeholder must still work, so the rejection
/// above is about the `::placeholder` link and not about descendant matching
/// being broken by this file's fixture.
#[test]
fn the_same_chain_without_a_placeholder_still_paints_the_descendant() {
    let sheet = Stylesheet::parse(".a .b { color: #ff0000; }");
    let styled = apply_with_cascade(&nested(), &sheet);
    let b = by_class(&styled, "b");
    assert!(
        own_style(&b).is_some_and(|s| s.contains("ff0000")),
        "control: `.a .b` is ordinary valid CSS and must reach `.b`, otherwise \
         the rejection test above is vacuous. Got style {:?}",
        own_style(&b)
    );
}

#[test]
fn a_placeholder_on_both_a_link_and_the_subject_still_rejects() {
    // Two `::placeholder` parts: one is a chain link, which already
    // disqualifies the whole selector, even though the subject carries one too.
    let tree = h(
        "input",
        Props::new().set("class", "a").set("placeholder", "hint"),
        vec![h(
            "input",
            Props::new().set("class", "b").set("placeholder", "hint"),
            vec![],
        )],
    );
    let sheet = Stylesheet::parse(".a::placeholder .b::placeholder { color: #ff0000; }");
    let styled = apply_with_cascade(&tree, &sheet);
    let b = by_class(&styled, "b");
    assert!(
        placeholder_style(&b).is_none(),
        "a selector with a `::placeholder` CHAIN LINK matches nothing at all, \
         even when its subject also carries `::placeholder`. Got placeholder \
         style {:?}",
        placeholder_style(&b)
    );
    assert!(!own_style(&b).is_some_and(|s| s.contains("ff0000")));
}

#[test]
fn a_rejected_placeholder_chain_does_not_disturb_the_rules_around_it() {
    // The rejection must be scoped to the offending rule: a sibling rule that
    // targets the same element normally still applies.
    let sheet =
        Stylesheet::parse(".a::placeholder .b { color: #ff0000; }\n.b { font-weight: bold; }");
    let styled = apply_with_cascade(&nested(), &sheet);
    let b = by_class(&styled, "b");
    assert!(
        own_style(&b).is_some_and(|s| s.contains("font-weight: bold")),
        "an unrelated `.b` rule must still apply alongside a rejected one; got {:?}",
        own_style(&b)
    );
    assert!(!own_style(&b).is_some_and(|s| s.contains("ff0000")));
}
