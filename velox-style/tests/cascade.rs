use velox_dom::layout::INLINE_BY_DEFAULT_TAGS;
use velox_dom::{Props, VNode, h};
use velox_style::{Stylesheet, apply_with_cascade, ua::ua_sheet};

fn get_style(node: &VNode) -> Option<String> {
    if let VNode::Element { props, .. } = node {
        props.attrs.get("style").cloned()
    } else {
        None
    }
}

/// Extract a single property value from a styled node's `style` attribute.
/// Returns `None` when the node has no style attribute or no declaration
/// for `key`.
fn style_value(node: &VNode, key: &str) -> Option<String> {
    let style = get_style(node)?;
    style.split(';').find_map(|decl| {
        let d = decl.trim();
        d.split_once(':')
            .and_then(|(k, v)| (k.trim() == key).then(|| v.trim().to_string()))
    })
}

/// First child of an element node; panics on shape mismatch.
fn first_child(node: &VNode) -> &VNode {
    let VNode::Element { children, .. } = node else {
        panic!("expected element, got: {node:?}");
    };
    &children[0]
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

#[test]
fn font_family_inherits() {
    let author = Stylesheet::parse(".app{ font-family: monospace }");
    let child = h("span", Props::new(), vec![]);
    let app = h("div", Props::from_class("app"), vec![child]);
    let styled = velox_style::apply_with_cascade(&app, &author);
    assert_eq!(
        style_value(first_child(&styled), "font-family"),
        Some("monospace".to_string())
    );
}

#[test]
fn text_align_inherits() {
    let author = Stylesheet::parse(".app{ text-align: center }");
    let child = h("span", Props::new(), vec![]);
    let app = h("div", Props::from_class("app"), vec![child]);
    let styled = velox_style::apply_with_cascade(&app, &author);
    assert_eq!(
        style_value(first_child(&styled), "text-align"),
        Some("center".to_string())
    );
}

/// Pins the full browser-parity inheritable set: every property from the
/// expansion (font-family, font-style, letter-spacing, text-align,
/// visibility, cursor) plus the pre-existing members (color, font-size,
/// font-weight, text-decoration, line-height) must propagate to children.
/// A non-inheritable property (background) must NOT leak downward.
#[test]
fn full_inheritable_set_propagates_to_children() {
    let author = Stylesheet::parse(concat!(
        ".app{ color: red; font-size: 16px; font-family: monospace; ",
        "font-weight: bold; font-style: italic; line-height: 1.5; ",
        "letter-spacing: 2px; text-align: center; visibility: hidden; ",
        "cursor: pointer; text-decoration: underline; background: #123456 }"
    ));
    let child = h("span", Props::new(), vec![]);
    let app = h("div", Props::from_class("app"), vec![child]);
    let styled = velox_style::apply_with_cascade(&app, &author);
    let span = first_child(&styled);
    let expected = [
        ("color", "red"),
        ("font-size", "16px"),
        ("font-family", "monospace"),
        ("font-weight", "bold"),
        ("font-style", "italic"),
        ("line-height", "1.5"),
        ("letter-spacing", "2px"),
        ("text-align", "center"),
        ("visibility", "hidden"),
        ("cursor", "pointer"),
        ("text-decoration", "underline"),
    ];
    for (k, v) in expected {
        assert_eq!(
            style_value(span, k),
            Some(v.to_string()),
            "{k} should inherit to child span"
        );
    }
    // Non-inheritable properties must not leak into children.
    assert_eq!(
        style_value(span, "background"),
        None,
        "background must not inherit"
    );
}

/// `font-style` was being dropped TWICE: `velox-style` listed it in
/// `INHERITABLE` (so the cascade propagated it) but `ComputedStyle::set_property`
/// had no arm for it, so the declaration never reached the computed style. The
/// inheritance half of the round trip is asserted by
/// `full_inheritable_set_propagates_to_children` above; this pins the other
/// half — the declaration actually parses into `ComputedStyle::font_style`
/// rather than being silently discarded.
#[test]
fn font_style_reaches_the_computed_style_it_is_inherited_into() {
    let author = Stylesheet::parse(".app{ font-style: italic }");
    let child = h("span", Props::new(), vec![]);
    let app = h("div", Props::from_class("app"), vec![child]);
    let styled = velox_style::apply_with_cascade(&app, &author);
    let span = first_child(&styled);

    // The cascade emits the declaration ...
    assert_eq!(
        style_value(span, "font-style"),
        Some("italic".to_string()),
        "font-style must be inherited by the cascade"
    );

    // ... and the DOM must actually parse it, which is the half that was
    // missing: with no `set_property` arm this stayed `Normal` while the
    // string above still said `italic`.
    let mut cs = velox_dom::style::ComputedStyle::default();
    for decl in get_style(span).unwrap_or_default().split(';') {
        if let Some((k, v)) = decl.trim().split_once(':') {
            cs.set_property(k.trim(), v.trim());
        }
    }
    assert_eq!(
        cs.font_style,
        velox_dom::style::FontStyle::Italic,
        "set_property must arm font-style: it is in INHERITABLE, so the cascade \
         hands it over, and dropping it here made the declaration a no-op"
    );
}

/// The arm must cover every keyword CSS defines, and must not corrupt the
/// field when given something it does not recognise.
#[test]
fn font_style_keywords_and_garbage() {
    use velox_dom::style::{ComputedStyle, FontStyle};

    for (value, expected) in [
        ("normal", FontStyle::Normal),
        ("italic", FontStyle::Italic),
        ("oblique", FontStyle::Oblique),
        ("ITALIC", FontStyle::Italic),
        ("  Oblique  ", FontStyle::Oblique),
    ] {
        let mut cs = ComputedStyle::default();
        cs.set_property("font-style", value);
        assert_eq!(cs.font_style, expected, "font-style: {value:?}");
    }

    // An unparsable value must leave the field at its initial value rather
    // than half-applying, matching how `font-weight` already behaves.
    let mut cs = ComputedStyle::default();
    cs.set_property("font-style", "diagonal");
    assert_eq!(
        cs.font_style,
        FontStyle::Normal,
        "an unknown font-style value must not change the field"
    );
}

// ===== UA `display: inline` defaults =====
//
// `apply_with_cascade` merges the UA sheet under the author sheet and writes
// the winning declarations into each element's `style` attribute, which is
// exactly what `compute_layout` reads. These tests therefore assert the
// *computed* result of the cascade, not the text of `ua.css`: a typo, a
// missing tag or an unparsable rule all show up as a missing declaration here.

/// Cascade one styled tree with an empty author sheet and return the
/// `display` that was computed for the single element child.
fn computed_display_with_ua_only(child: &VNode) -> Option<String> {
    let root = h("div", Props::default(), vec![child.clone()]);
    let styled = apply_with_cascade(&root, &Stylesheet::default());
    style_value(first_child(&styled), "display")
}

#[test]
fn ua_gives_phrasing_elements_display_inline() {
    for tag in ["span", "a", "strong", "em"] {
        let el = h(tag, Props::default(), vec![]);
        assert_eq!(
            computed_display_with_ua_only(&el).as_deref(),
            Some("inline"),
            "UA sheet must give <{tag}> display: inline"
        );
    }
}

#[test]
fn ua_does_not_make_block_elements_inline() {
    for tag in ["p", "h1", "div", "section", "li", "ul", "pre", "blockquote"] {
        let el = h(tag, Props::default(), vec![]);
        let display = computed_display_with_ua_only(&el);
        assert_ne!(
            display.as_deref(),
            Some("inline"),
            "block-level <{tag}> must never resolve to display: inline"
        );
    }
    // A UA `display: block` rule is deliberately *not* added just to make this
    // assertable, so the honest assertion is about the tag, not about `p`:
    // `p` does carry an explicit UA display, and it must be `block`.
    let p = h("p", Props::default(), vec![]);
    assert_eq!(
        computed_display_with_ua_only(&p).as_deref(),
        Some("block"),
        "the UA rule for p declares display: block"
    );
    // `div` is block by initial-value fallback with no UA declaration at all.
    let div = h("div", Props::default(), vec![]);
    assert_eq!(
        computed_display_with_ua_only(&div),
        None,
        "div gets no UA display declaration; it is block by layout fallback"
    );
}

#[test]
fn author_display_block_overrides_ua_display_inline() {
    let author = Stylesheet::parse("span { display: block; }");
    let root = h(
        "div",
        Props::default(),
        vec![h("span", Props::default(), vec![])],
    );
    let styled = apply_with_cascade(&root, &author);
    assert_eq!(
        style_value(first_child(&styled), "display").as_deref(),
        Some("block"),
        "an author display: block must win over the UA display: inline default"
    );
}

#[test]
fn inline_style_display_wins_over_author_block() {
    let author = Stylesheet::parse("span { display: block; }");
    let root = h(
        "div",
        Props::default(),
        vec![h("span", props_with_display_inline(), vec![])],
    );
    let styled = apply_with_cascade(&root, &author);
    assert_eq!(
        style_value(first_child(&styled), "display").as_deref(),
        Some("inline"),
        "inline style must beat the author rule and the UA default"
    );
}

#[test]
fn ua_gives_label_inline_but_leaves_inline_block_elements_blocked() {
    let label = h("label", Props::default(), vec![]);
    assert_eq!(
        computed_display_with_ua_only(&label).as_deref(),
        Some("inline"),
        "label is display: inline in browsers and must get the same default"
    );
    // `button` and `input` are `display: inline-block` in browsers. Velox has
    // no inline-block layout, so claiming `display: inline` for them would be
    // a false parity claim; they must be left to the block fallback.
    for tag in ["button", "input", "select", "textarea"] {
        let el = h(tag, Props::default(), vec![]);
        assert_ne!(
            computed_display_with_ua_only(&el).as_deref(),
            Some("inline"),
            "<{tag}> is inline-block in browsers; Velox must not claim display: inline"
        );
    }
}

/// `Props::attrs` has no public builder, so build the inline style attribute
/// the way a compiled `.vx` template does.
fn props_with_display_inline() -> Props {
    let mut props = Props::default();
    props
        .attrs
        .insert("style".to_string(), "display: inline".to_string());
    props
}

// ===== Drift guard: ua.css and the velox-dom layout fallback =====
//
// `ua.css` is the cascade-side home of the `display: inline` defaults and
// `INLINE_BY_DEFAULT_TAGS` is the layout engine's fallback for elements that
// never went through the cascade. velox-dom cannot depend on velox-style, so
// the list has to exist twice; this test is what keeps the copies honest.

/// The tag names of every UA rule that declares `display: inline` and nothing
/// else, i.e. the selector set of the ua.css phrasing-content rule.
fn ua_inline_display_tags() -> Vec<String> {
    let mut tags: Vec<String> = ua_sheet()
        .rules
        .iter()
        .filter(|rule| {
            rule.decls.len() == 1
                && rule
                    .decls
                    .get("display")
                    .is_some_and(|value| value.trim() == "inline")
        })
        .flat_map(|rule| {
            rule.selector
                .parts
                .iter()
                .map(|part| part.tag.clone())
                .collect::<Vec<_>>()
        })
        .collect();
    tags.sort();
    tags
}

#[test]
fn ua_inline_tag_list_matches_the_layout_fallback_table() {
    let from_ua = ua_inline_display_tags();
    let from_layout: Vec<String> = INLINE_BY_DEFAULT_TAGS
        .iter()
        .map(|tag| tag.to_string())
        .collect();

    // Sanity: the rule under test really is there, otherwise both sides are
    // empty and the comparison below would pass vacuously.
    assert!(
        from_ua.len() >= 30,
        "expected the ua.css phrasing rule, found {} tags",
        from_ua.len()
    );

    let only_in_ua: Vec<String> = from_ua
        .iter()
        .filter(|t| !from_layout.contains(t))
        .cloned()
        .collect();
    let only_in_layout: Vec<String> = from_layout
        .iter()
        .filter(|t| !from_ua.contains(t))
        .cloned()
        .collect();
    assert_eq!(
        only_in_ua, only_in_layout,
        "ua.css display:inline tags and velox-dom INLINE_BY_DEFAULT_TAGS must be the same set"
    );
    assert_eq!(from_ua.len(), from_layout.len(), "tag counts must match");

    // Duplicates in either list are a silent-drift failure mode: the guard
    // above compares sets, so a repeated tag would hide a missing one.
    let mut deduped = from_ua.clone();
    deduped.dedup();
    assert_eq!(deduped, from_ua, "ua.css inline rule lists a tag twice");
    let mut deduped_layout = from_layout.clone();
    deduped_layout.dedup();
    assert_eq!(
        deduped_layout, from_layout,
        "INLINE_BY_DEFAULT_TAGS lists a tag twice"
    );
}
