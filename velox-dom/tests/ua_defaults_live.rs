//! Task 4.2a — proof that the UA stylesheet's added declarations actually REACH
//! the screen.
//!
//! # Why this test file exists
//!
//! `ua.css` is not a stylesheet the engine interprets. `apply_with_cascade`
//! merges it into each element's `style` **attribute string**
//! (`velox-style/src/lib.rs`: `merge_styles` -> `new_props.set("style", ...)`).
//! The paint path then re-parses that string in
//! `velox-renderer/src/skia_render.rs::parse_text_style`, which accepts exactly
//! NINE keys, and `velox-dom/src/layout.rs` re-parses it again for geometry.
//!
//! That means a declaration in `ua.css` can be perfectly valid CSS, match a
//! selector, and be merged into the style string — and still be dropped by
//! every reader, so it never affects a single pixel. Asserting on the *text* of
//! `ua.css` would pass in exactly that situation, which is the bug class this
//! file exists to close.
//!
//! So every assertion below goes through the REAL path:
//! `velox_style::apply_with_cascade` (which composes the UA sheet) and then
//! `velox_dom::layout::compute_layout`, and asserts on computed values —
//! geometry where `velox-dom` consumes the property, and the post-cascade style
//! string where only the renderer's `parse_text_style` consumes it.
//!
//! # The invariant these tests police
//!
//! A declaration may only be added to `ua.css` if it
//!   1. has a `set_property` arm in `velox-dom/src/style.rs`, AND
//!   2. is consumed by `parse_text_style` or by `velox-dom/src/layout.rs`.
//!
//! # Why some properties are asserted on the style string, not on geometry
//!
//! `font-weight` and `white-space` reach the screen through
//! `parse_text_style`, a private fn inside the renderer's `skia-native` feature
//! that `velox-dom` cannot call. For those, the post-cascade style string is
//! the honest observable: it is byte-for-byte the input `parse_text_style`
//! receives. Geometry is used for everything `velox-dom` itself consumes, and
//! every geometry assertion here FAILS on the pre-change tree.

use velox_dom::layout::{LayoutNode, compute_layout};
use velox_dom::{Props, VNode, h, text};
use velox_style::{Stylesheet, apply_with_cascade};

// ===== harness ==============================================================

/// Run the real 3-layer cascade (UA < author < inline) over a tree.
fn cascade(root: &VNode) -> VNode {
    apply_with_cascade(root, &Stylesheet::default())
}

/// Lay a tree out after the cascade, at a viewport wide enough that nothing
/// is clipped for reasons unrelated to the property under test.
fn cascade_layout(root: &VNode) -> LayoutNode {
    compute_layout(&cascade(root), 800, 600)
}

/// The computed style string the cascade produced for the first element with
/// `tag`, parsed into `(property, value)` pairs. This is the exact string
/// `parse_text_style` receives.
fn computed_style(tree: &VNode, tag: &str) -> Vec<(String, String)> {
    fn walk(node: &VNode, tag: &str) -> Option<String> {
        match node {
            VNode::Text(_) => None,
            VNode::Element {
                tag: t,
                props,
                children,
            } => {
                if t == tag {
                    return props.attrs.get("style").cloned();
                }
                children.iter().find_map(|c| walk(c, tag))
            }
        }
    }
    let raw = walk(tree, tag).unwrap_or_default();
    raw.split(';')
        .filter_map(|d| {
            let d = d.trim();
            if d.is_empty() {
                return None;
            }
            d.split_once(':')
                .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        })
        .collect()
}

/// The single value of `prop` in the computed style of `tag`.
fn computed(tree: &VNode, tag: &str, prop: &str) -> Option<String> {
    computed_style(tree, tag)
        .into_iter()
        .find(|(k, _)| k == prop)
        .map(|(_, v)| v)
}

/// The vertical gap between two consecutive siblings, i.e. the distance from
/// the bottom of `a` to the top of `b`. For two siblings with equal margins
/// this is the collapsed value, which is what CSS specifies.
fn sibling_gap(parent: &LayoutNode, a: usize, b: usize) -> i32 {
    parent.children[b].rect.y - (parent.children[a].rect.y + parent.children[a].rect.h)
}

// ===== B: `ul, ol` padding-left (was `padding-inline-start`) ==================
//
// `padding-inline-start` has NO `set_property` arm and no reader anywhere in
// the workspace, so the 40px indent the UA sheet claimed for `ul`/`ol` never
// existed in the layout. `padding-left` has an arm and IS read by layout, so
// the child's x offset becomes observable.

#[test]
fn list_items_are_indented_forty_px_by_the_ua_sheet() {
    let root = h(
        "div",
        Props::new(),
        vec![h(
            "ul",
            Props::new(),
            vec![h("li", Props::new(), vec![text("x")])],
        )],
    );
    let laid = cascade_layout(&root);
    let list = &laid.children[0];
    let item = &list.children[0];
    assert_eq!(
        item.rect.x - list.rect.x,
        40,
        "ua.css `ul, ol {{ padding-left: 40px }}` must indent the list item by 40px; \
         `padding-inline-start` has no set_property arm and was silently dropped"
    );
}

#[test]
fn ordered_list_items_are_indented_too() {
    let root = h(
        "div",
        Props::new(),
        vec![h(
            "ol",
            Props::new(),
            vec![h("li", Props::new(), vec![text("x")])],
        )],
    );
    let laid = cascade_layout(&root);
    let list = &laid.children[0];
    assert_eq!(list.children[0].rect.x - list.rect.x, 40);
}

// ===== A: `pre` stops wrapping ===============================================

#[test]
fn pre_does_not_wrap_a_line_that_is_wider_than_its_box() {
    // 200px of content box; at the default 16px font the heuristic measurer
    // gives this 39 chars a width of 16 * 0.5 * 39 = 312px, so `normal`
    // white-space MUST wrap it onto two lines. `white-space: pre` must not.
    let long = "aaa bbb ccc ddd eee fff ggg hhh iii jjj";
    let root = h(
        "div",
        Props::from_inline("width:200px"),
        vec![h("pre", Props::new(), vec![text(long)])],
    );
    let laid = cascade_layout(&root);
    let pre = &laid.children[0];

    let control_root = h(
        "div",
        Props::from_inline("width:200px"),
        vec![h("pre", Props::new(), vec![text("aaa")])],
    );
    let control = cascade_layout(&control_root);
    let control_pre = &control.children[0];

    assert_eq!(
        pre.rect.h, control_pre.rect.h,
        "a `pre` line wider than its box must stay on ONE line (height {} == \
         one-line control height {}); the UA sheet's `white-space: pre` did not reach layout",
        pre.rect.h, control_pre.rect.h
    );
}

#[test]
fn pre_still_breaks_on_newlines() {
    // The counterpart: `pre` must not become a single unbroken line, or
    // `white-space: pre` would be honoured as `nowrap`.
    let root = h(
        "div",
        Props::from_inline("width:200px"),
        vec![h("pre", Props::new(), vec![text("aaa\nbbb\nccc")])],
    );
    let laid = cascade_layout(&root);
    let three_lines = laid.children[0].rect.h;

    let one_root = h(
        "div",
        Props::from_inline("width:200px"),
        vec![h("pre", Props::new(), vec![text("aaa")])],
    );
    let one_line = cascade_layout(&one_root).children[0].rect.h;

    assert!(
        three_lines > one_line,
        "three \\n-separated lines ({three_lines}) must be taller than one ({one_line}); \
         `white-space: pre` preserves newlines rather than acting as `nowrap`"
    );
}

#[test]
fn pre_keeps_a_one_em_vertical_margin() {
    // 1em resolves against the parent font size, so 16px at the default.
    let root = h(
        "div",
        Props::new(),
        vec![
            h("pre", Props::new(), vec![text("a")]),
            h("pre", Props::new(), vec![text("b")]),
        ],
    );
    let laid = cascade_layout(&root);
    assert_eq!(
        sibling_gap(&laid, 0, 1),
        16,
        "ua.css `pre {{ margin: 1em 0 }}` must resolve to 16px at the default 16px font; \
         `em` in margin/padding IS live (Length::Em is resolved in layout)"
    );
}

// NOTE: a "pre preserves interior spaces" test was written, measured, and
// DELETED. It could not fail: `compute_layout` measures `"a    b"` at 48px
// under `pre` AND under `div`/`normal`, so interior-space preservation is not
// observable in layout geometry at all (only the newline/width behaviour of
// `white-space: pre` is, which the two tests above cover). Shipping it would
// have been an assertion that passes before and after the change.

// ===== D: tier-1 heading rules ==============================================

/// The `em` margins on `h3`-`h6` ARE live, and their distinct values are what
/// makes the row falsifiable: 1em/1.33em/1.67em/2.33em of 16px round to
/// 16/21/27/37.
#[test]
fn tier_one_heading_margins_resolve_and_differ() {
    for (tag, expected) in [("h3", 16), ("h4", 21), ("h5", 27), ("h6", 37)] {
        let root = h(
            "div",
            Props::new(),
            vec![
                h(tag, Props::new(), vec![text("a")]),
                h(tag, Props::new(), vec![text("b")]),
            ],
        );
        let laid = cascade_layout(&root);
        assert_eq!(
            sibling_gap(&laid, 0, 1),
            expected,
            "ua.css `{tag} {{ margin: ...em 0 }}` must resolve to {expected}px at 16px"
        );
    }
}

#[test]
fn tier_one_headings_are_bold() {
    // `font-weight` is a `parse_text_style` key, not a layout key, so the
    // post-cascade string is the observable.
    let root = h(
        "div",
        Props::new(),
        vec![
            h("h3", Props::new(), vec![text("a")]),
            h("h4", Props::new(), vec![text("a")]),
            h("h5", Props::new(), vec![text("a")]),
            h("h6", Props::new(), vec![text("a")]),
        ],
    );
    let styled = cascade(&root);
    for tag in ["h3", "h4", "h5", "h6"] {
        assert_eq!(
            computed(&styled, tag, "font-weight").as_deref(),
            Some("bold"),
            "ua.css must set `font-weight: bold` on {tag}; `bolder` is not a value \
             `parse_text_style` honours (it only accepts `bold` or a number >= 700)"
        );
    }
}

#[test]
fn tier_one_headings_deliberately_carry_no_font_size() {
    // `em` in `font-size` is a silent no-op at BOTH readers: layout's
    // `inline_font_size` and the renderer's `parse_px_value` each strip only
    // `px`. Adding it would be a new silent lie, so `h3`-`h6` must NOT have it.
    // (Note `h1`/`h2` still do — pre-existing, tracked, out of this task's scope.)
    let root = h(
        "div",
        Props::new(),
        vec![
            h("h3", Props::new(), vec![text("a")]),
            h("h4", Props::new(), vec![text("a")]),
            h("h5", Props::new(), vec![text("a")]),
            h("h6", Props::new(), vec![text("a")]),
        ],
    );
    let styled = cascade(&root);
    for tag in ["h3", "h4", "h5", "h6"] {
        assert_eq!(
            computed(&styled, tag, "font-size"),
            None,
            "{tag} must not carry `font-size`: an `em` font-size is a no-op at both readers"
        );
    }
}

// ===== D: blockquote / figure / dl / dd =====================================

#[test]
fn blockquote_and_figure_take_the_browser_margins() {
    for tag in ["blockquote", "figure"] {
        let root = h(
            "div",
            Props::new(),
            vec![h(tag, Props::new(), vec![text("a")])],
        );
        let laid = cascade_layout(&root);
        let child = &laid.children[0];
        assert_eq!(
            child.rect.x - laid.rect.x,
            40,
            "ua.css `{tag} {{ margin: 1em 40px }}` must indent 40px horizontally"
        );
        assert_eq!(
            child.rect.y - laid.rect.y,
            16,
            "ua.css `{tag} {{ margin: 1em 40px }}` must indent 16px (1em) vertically"
        );
    }
}

#[test]
fn dl_takes_a_one_em_vertical_margin() {
    let root = h(
        "div",
        Props::new(),
        vec![
            h("dl", Props::new(), vec![text("a")]),
            h("dl", Props::new(), vec![text("b")]),
        ],
    );
    let laid = cascade_layout(&root);
    assert_eq!(sibling_gap(&laid, 0, 1), 16);
}

#[test]
fn dd_is_indented_forty_px() {
    let root = h(
        "div",
        Props::new(),
        vec![h(
            "dl",
            Props::new(),
            vec![h("dd", Props::new(), vec![text("a")])],
        )],
    );
    let laid = cascade_layout(&root);
    let list = &laid.children[0];
    let term = &list.children[0];
    assert_eq!(
        term.rect.x - list.rect.x,
        40,
        "ua.css `dd {{ margin-left: 40px }}` must indent the definition 40px"
    );
}

// ===== D: hr ================================================================

#[test]
fn hr_takes_a_half_em_vertical_margin() {
    // 0.5em of 16px = 8px, and the observable is the `hr` box's own y offset.
    //
    // NB: this cannot be written as "the gap between two `hr`s". A bare `hr` is
    // an EMPTY block (h == 0, no children), and layout hoists an empty block's
    // collapsed margins into `last_bottom_margin` without advancing `cur_y`, so
    // two of them share one y and their gap is 0 no matter what the margin is.
    // The single-box y offset is the honest observable.
    let root = h(
        "div",
        Props::new(),
        vec![
            h("hr", Props::new(), vec![]),
            h("div", Props::new(), vec![text("a")]),
        ],
    );
    let laid = cascade_layout(&root);
    let rule = &laid.children[0];
    assert_eq!(
        rule.rect.y - laid.rect.y,
        8,
        "ua.css `hr {{ margin: 0.5em 0 }}` must resolve to 8px at the default 16px font"
    );
}

// ===== D: fieldset ==========================================================

#[test]
fn fieldset_pads_its_content_by_three_quarters_of_an_em() {
    // 0.75em of 16px = 12px. A child block's x offset is the observable,
    // because `LayoutNode` carries no padding field of its own.
    let root = h(
        "div",
        Props::new(),
        vec![h(
            "fieldset",
            Props::new(),
            vec![h("div", Props::from_inline("height:10px"), vec![])],
        )],
    );
    let laid = cascade_layout(&root);
    let fieldset = &laid.children[0];
    let content = &fieldset.children[0];
    assert_eq!(
        content.rect.x - fieldset.rect.x,
        12,
        "ua.css `fieldset {{ padding-left: 0.75em }}` must push content 12px in"
    );
}

// ===== D: center ============================================================

#[test]
fn center_centres_its_inline_content() {
    // `text-align` is read per inline line box (layout.rs, `ctx.text_align`),
    // so the offset lands on the text's own rect, not the container's.
    let body = "short";
    let root = h(
        "div",
        Props::from_inline("width:400px"),
        vec![h("center", Props::new(), vec![text(body)])],
    );
    let laid = cascade_layout(&root);
    let container = &laid.children[0];
    let line = container
        .children
        .first()
        .unwrap_or_else(|| panic!("`center` produced no inline line box to align: {container:?}"));
    assert!(
        line.rect.x > container.rect.x,
        "ua.css `center {{ text-align: center }}` must push the line box right of the \
         container's content edge (line x {} vs container x {})",
        line.rect.x,
        container.rect.x
    );
}

// ===== C: `white-space` is an inherited property ============================

#[test]
fn white_space_reaches_nested_inline_elements() {
    // This is the falsifiable test for the single `velox-style/src/lib.rs`
    // line: `INHERITABLE` must contain `white-space`, or a `white-space`
    // declared on an ancestor is lost at the next element boundary.
    let root = h(
        "pre",
        Props::new(),
        vec![h("span", Props::new(), vec![text("a")])],
    );
    let styled = cascade(&root);
    assert_eq!(
        computed(&styled, "span", "white-space").as_deref(),
        Some("pre"),
        "`white-space` is inherited (CSS 2.1 §16.6); velox-style's INHERITABLE list \
         must carry it or a nested element loses the ancestor's white-space"
    );
}

// ===== D: textarea ==========================================================

#[test]
fn textarea_uses_pre_wrap() {
    let root = h("textarea", Props::new(), vec![text("a")]);
    let styled = cascade(&root);
    assert_eq!(
        computed(&styled, "textarea", "white-space").as_deref(),
        Some("pre-wrap"),
        "ua.css `textarea {{ white-space: pre-wrap }}` must reach the post-cascade \
         style string that `parse_text_style` reads"
    );
}

// NOTE: a matching "textarea preserves interior spaces" layout test was also
// written, measured, and DELETED for the same reason as the `pre` one above:
// the engine measures a preserved run identically to a collapsed one, so the
// assertion would pass before and after. `textarea_uses_pre_wrap` above is
// kept because it fails on the pre-change tree.

// ===== D: strong / b ========================================================

#[test]
fn strong_and_b_are_bold() {
    let root = h(
        "div",
        Props::new(),
        vec![
            h("strong", Props::new(), vec![text("a")]),
            h("b", Props::new(), vec![text("a")]),
        ],
    );
    let styled = cascade(&root);
    for tag in ["strong", "b"] {
        assert_eq!(
            computed(&styled, tag, "font-weight").as_deref(),
            Some("bold"),
            "ua.css must set `font-weight: bold` on {tag}; `parse_text_style` accepts only \
             `bold` or a number >= 700, so `bolder` would render as normal weight"
        );
    }
}
