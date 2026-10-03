//! A5: the placeholder paint path and the UA input defaults.
//!
//! ## What was missing
//!
//! Nothing. `grep -i placeholder` across every `src` tree returned only
//! comments and unrelated matches: no `placeholder` arm in
//! `velox-dom/src/style.rs`, no read in the renderer, no `::placeholder`
//! pseudo-element machinery anywhere in `velox-style`, and no placeholder colour
//! token. The attribute reached the VNode generically through `props.attrs` and
//! then stopped. An `<input placeholder="What needs doing?">` rendered as an
//! empty box.
//!
//! ## What is asserted
//!
//! Two layers, deliberately separated:
//!
//!  * **the paint path** — a placeholder appears for an empty field, disappears
//!    the moment there is a value, is truncated to the field's content width,
//!    and takes a colour DERIVED from the field's own `color` rather than a
//!    fixed grey;
//!  * **the styling hook** — `input::placeholder { color: … }` in an author
//!    stylesheet actually reaches the paint. This is the half that makes the
//!    feature real rather than a hardcoded special case, and it is the half a
//!    pixel test alone cannot cover: it needs the cascade to run.
//!
//! Plus the UA input rule: an `<input>`'s auto height is exactly padding +
//! border (it has no children, so content height is zero), which is why the
//! boilerplate's `padding: 10px 12px; border: 1px` produced a 22px field.

#![cfg(feature = "skia-native")]

use velox_dom::{Props, VNode, h};
use velox_renderer::render_vnode_to_rgba;
use velox_style::Stylesheet;

const W: i32 = 320;
const H: i32 = 120;
const PAGE: &str = "background:#000000";

fn px(buf: &[u8], x: i32, y: i32) -> [u8; 4] {
    let i = ((y * W + x) * 4) as usize;
    [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
}

fn render_with(v: &VNode, sheet: &Stylesheet) -> Vec<u8> {
    render_vnode_to_rgba(v, sheet, W, H).expect("render to rgba")
}

fn render(v: &VNode) -> Vec<u8> {
    render_with(v, &Stylesheet::default())
}

/// A dark field with light text — the boilerplate's own palette, so the derived
/// placeholder has to cope with a dark background rather than a white one.
const DARK: &str = "width:200px;height:60px;background:#000000;color:#ffffff;font-size:20px";

fn field(style: &str, extra: &[(&str, &str)]) -> VNode {
    let mut p = Props::new().set("type", "text").set("style", style);
    for (k, v) in extra {
        p = p.set(*k, *v);
    }
    h(
        "div",
        Props::new().set("style", PAGE),
        vec![h("input", p, vec![])],
    )
}

fn field_box(v: &VNode) -> velox_dom::layout::Rect {
    let styled = velox_style::apply_with_cascade(v, &Stylesheet::default());
    velox_dom::layout::compute_layout(&styled, W, H).children[0].rect
}

fn metrics_for(v: &VNode) -> velox_renderer::input_metrics::InputTextMetrics {
    fn style_of(node: &VNode) -> Option<String> {
        match node {
            VNode::Text(_) => None,
            VNode::Element {
                tag,
                props,
                children,
            } => {
                if tag == "input" {
                    return props.attrs.get("style").cloned();
                }
                children.iter().find_map(style_of)
            }
        }
    }
    let styled = velox_style::apply_with_cascade(v, &Stylesheet::default());
    velox_renderer::input_metrics::input_text_metrics(
        style_of(&styled).as_deref(),
        field_box(v),
        (W as f32, H as f32),
        velox_dom::layout::DEFAULT_ROOT_FONT_SIZE,
    )
}

/// Every pixel in the field that is not the page-black, grouped so the count of
/// lit pixels is the observable.
fn lit_pixels(buf: &[u8], b: &velox_dom::layout::Rect) -> usize {
    (b.y..b.y + b.h)
        .flat_map(|y| (b.x..b.x + b.w).map(move |x| px(buf, x, y)))
        .filter(|p| p[0] > 20 || p[1] > 20 || p[2] > 20)
        .count()
}

/// The brightest pixel found inside the field, which for thin anti-aliased text
/// is the glyph core rather than any particular declared value.
///
/// The outer 3px ring is skipped: a field with no declared `border` still gets
/// the `#c8c8c8` fallback outline, and at this font size that outline is
/// BRIGHTER than a dimmed placeholder — measuring the whole box would report
/// the border for every render and prove nothing about the placeholder.
fn peak(buf: &[u8], b: &velox_dom::layout::Rect) -> [u8; 4] {
    (b.y + 3..b.y + b.h - 3)
        .flat_map(|y| (b.x + 3..b.x + b.w - 3).map(move |x| px(buf, x, y)))
        .max_by_key(|p| p[0].max(p[1]).max(p[2]))
        .unwrap_or([0, 0, 0, 0])
}

// ── 1. the placeholder paints at all ────────────────────────────────────

#[test]
fn an_empty_field_with_a_placeholder_paints_something() {
    let v = field(DARK, &[("placeholder", "What needs doing?")]);
    let buf = render(&v);
    let b = field_box(&v);
    assert!(
        lit_pixels(&buf, &b) > 20,
        "an empty field with a placeholder is completely blank ({} lit pixels): \
         the attribute reaches the VNode and is then ignored",
        lit_pixels(&buf, &b)
    );
}

// ── 2. it disappears once there is a value ──────────────────────────────

// The defining behaviour, stated as an EXACT equivalence rather than a pixel
// count: a filled field must render byte-identically whether or not it carries
// a `placeholder` attribute. Anything else is the placeholder still being drawn
// behind the value, which is worse than having no placeholder at all.
#[test]
fn a_placeholder_disappears_once_the_field_has_a_value() {
    let with_placeholder = field(
        DARK,
        &[("placeholder", "What needs doing?"), ("value", "x")],
    );
    let without = field(DARK, &[("value", "x")]);
    assert_eq!(
        render(&with_placeholder),
        render(&without),
        "adding a `placeholder` attribute changed a FILLED field: the \
         placeholder is still being painted behind the value"
    );
}

// An EMPTY placeholder attribute is not a placeholder; painting an empty string
// would still cost a draw call and could trip the ellipsis path.
#[test]
fn an_empty_placeholder_attribute_paints_nothing() {
    let none = field(DARK, &[]);
    let blank = field(DARK, &[("placeholder", "")]);
    let b = field_box(&none);
    assert_eq!(
        lit_pixels(&render(&blank), &b),
        lit_pixels(&render(&none), &b),
        "`placeholder=\"\"` painted something an absent placeholder does not"
    );
}

// ── 3. the colour is derived, not a fixed literal ────────────────────────

// A hardcoded `#999` is invisible on the dark fields this engine's own
// boilerplate uses and invisible on a light one if the theme flips. The
// placeholder must be the field's own ink, held back.
#[test]
fn the_placeholder_keeps_the_fields_hue_but_not_its_strength() {
    let warm = field(
        "width:200px;height:60px;background:#000000;color:#ffffff;font-size:20px",
        &[("placeholder", "aaa")],
    );
    let cool = field(
        "width:200px;height:60px;background:#000000;color:#ff8800;font-size:20px",
        &[("placeholder", "aaa")],
    );
    let b = field_box(&warm);
    let w = peak(&render(&warm), &b);
    let c = peak(&render(&cool), &b);
    assert_ne!(
        w, c,
        "two different `color` values produced the same brightest pixel \
         ({w:?}): the placeholder colour is a fixed literal"
    );
    assert!(
        c[0] > c[1],
        "with `color:#ff8800` the placeholder's peak is {c:?}: it should still \
         be red-dominant, i.e. the same hue as the text it stands in for"
    );
}

// The derivation must hold the value ink BACK — a placeholder at full strength
// is indistinguishable from a value.
#[test]
fn the_placeholder_is_dimmer_than_the_value_it_replaces() {
    let placeholder = field(
        "width:200px;height:60px;background:#000000;color:#ffffff;font-size:20px",
        &[("placeholder", "mmm")],
    );
    let value = field(
        "width:200px;height:60px;background:#000000;color:#ffffff;font-size:20px",
        &[("value", "mmm")],
    );
    let b = field_box(&placeholder);
    let p = peak(&render(&placeholder), &b);
    let v = peak(&render(&value), &b);
    assert!(
        p[0] < v[0],
        "the placeholder peaks at {p:?} and the value at {v:?}: the placeholder \
         must be held back, or a filled field is indistinguishable from an \
         empty one"
    );
    assert!(
        p[0] > 40,
        "the placeholder peaks at {p:?}: dimming it toward the background has \
         made it illegible rather than merely recessive"
    );
}

// ── 4. it is clipped to the field ────────────────────────────────────────

// A placeholder longer than the field must be truncated, or it paints over the
// border and into the page.
//
// Stated as a DIFF against the same field with no placeholder, so the field's
// own border cannot be mistaken for overflow: a 1px outline centred on the box
// edge legitimately paints a column outside the box. Only pixels the
// PLACEHOLDER contributed can fail this.
#[test]
fn a_placeholder_longer_than_the_field_is_truncated_to_its_content_width() {
    let long = "a placeholder far too long to fit inside this narrow field";
    let style = "width:120px;height:60px;background:#000000;color:#ffffff;\
                 font-size:20px;padding-left:4px";
    let with = render(&field(style, &[("placeholder", long)]));
    let without = render(&field(style, &[]));
    let b = field_box(&field(style, &[]));
    let m = metrics_for(&field(style, &[("placeholder", long)]));
    assert!(
        m.text_width() > 0.0,
        "the field's content width is {}: this test cannot say anything",
        m.text_width()
    );
    let (bx, bw, by, bh) = (b.x, b.w, b.y, b.h);
    let spill = (bx + bw + 2..W)
        .flat_map(|x| (by..by + bh).map(move |y| (x, y)))
        .filter(|(x, y)| px(&with, *x, *y) != px(&without, *x, *y))
        .count();
    assert_eq!(
        spill, 0,
        "{spill} pixels the PLACEHOLDER added past the field's right edge: a \
         long placeholder must be truncated to the content width, not allowed to \
         run over the border and out into the page"
    );
}

// ── 5. `::placeholder` is STYLEABLE, not just present ────────────────────

// The whole point of the pseudo-element hook: an author who wants a specific
// placeholder colour gets it, without the renderer growing another literal.
#[test]
fn an_author_placeholder_colour_reaches_the_paint() {
    let sheet = Stylesheet::parse("input::placeholder { color: #ff0000; }");
    let v = field(
        "width:200px;height:60px;background:#000000;color:#ffffff;font-size:20px",
        &[("placeholder", "mmm")],
    );
    let plain = render(&v);
    let styled = render_with(&v, &sheet);
    let b = field_box(&v);
    let p = peak(&plain, &b);
    let s = peak(&styled, &b);
    assert_ne!(
        p, s,
        "`input::placeholder {{ color: #ff0000 }}` changed nothing: the \
         pseudo-element is not reaching the paint"
    );
    assert!(
        s[0] > 150 && s[1] < 90 && s[2] < 90,
        "the styled peak is {s:?}: it should be red-dominant"
    );
}

// And the pseudo-element must NOT be applied to a field that is showing a
// value — the classic failure of a "works or does not work" implementation is a
// rule that matches every input unconditionally and paints the VALUE red.
#[test]
fn the_placeholder_rule_does_not_touch_the_value_text() {
    let sheet = Stylesheet::parse("input::placeholder { color: #ff0000; }");
    let v = field(
        "width:200px;height:60px;background:#000000;color:#ffffff;font-size:20px",
        &[("placeholder", "mmm"), ("value", "mmm")],
    );
    let plain = render_with(&v, &Stylesheet::default());
    let styled = render_with(&v, &sheet);
    assert_eq!(
        plain, styled,
        "an `input::placeholder` rule changed a FILLED field: the \
         pseudo-element is matching inputs that are showing a value, which is \
         how the value text ends up the wrong colour"
    );
}

// A field with no placeholder attribute at all must not match either.
#[test]
fn the_placeholder_rule_ignores_a_field_with_no_placeholder() {
    let sheet = Stylesheet::parse("input::placeholder { color: #ff0000; }");
    let v = field(DARK, &[("value", "mmm")]);
    assert_eq!(
        render_with(&v, &Stylesheet::default()),
        render_with(&v, &sheet),
        "`input::placeholder` styled a field that declares no placeholder"
    );
}

// ── 6. the UA input defaults ─────────────────────────────────────────────

// An `<input>`'s auto height is exactly padding + border: it has no children,
// so its content height is zero. Without a `min-height` the boilerplate's
// `padding: 10px 12px; border: 1px` yields a 22px field, which is the "the
// input looks wrong" report in numeric form.
#[test]
fn an_input_with_no_author_height_still_gets_a_usable_height() {
    let v = h("div", Props::new(), vec![h("input", Props::new(), vec![])]);
    let styled = velox_style::apply_with_cascade(&v, &Stylesheet::default());
    let layout = velox_dom::layout::compute_layout(&styled, W, H);
    let b = layout.children[0].rect;
    assert!(
        b.h >= 24,
        "a bare `<input>` is {}px tall: with zero content height it collapses to \
         padding+border, which is unusable as a text field",
        b.h
    );
}

#[test]
fn an_input_gets_ua_padding_even_with_no_author_style() {
    let v = h("div", Props::new(), vec![h("input", Props::new(), vec![])]);
    let styled = velox_style::apply_with_cascade(&v, &Stylesheet::default());
    let mut found = None;
    fn walk(node: &VNode, out: &mut Option<String>) {
        match node {
            VNode::Text(_) => {}
            VNode::Element {
                tag,
                props,
                children,
            } => {
                if tag == "input" {
                    *out = props.attrs.get("style").cloned();
                }
                for c in children {
                    walk(c, out);
                }
            }
        }
    }
    walk(&styled, &mut found);
    let style = found.expect("the input must carry a cascaded style");
    assert!(
        style.contains("padding"),
        "the UA sheet gave an `<input>` no padding (style was {style:?}), so its \
         text starts hard against its border"
    );
}

// An author's padding must still win over the UA's — the UA rule is a default,
// not an override. (This is the drift guard: a UA rule with a higher priority
// than the author would quietly undo A4.)
#[test]
fn an_author_padding_beats_the_ua_default() {
    let v = field(
        "width:200px;height:40px;padding:10px 12px",
        &[("value", "x")],
    );
    let m = metrics_for(&v);
    assert_eq!(
        m.padding_left, 12.0,
        "the author wrote `padding:10px 12px` but the effective left padding is \
         {}: the UA rule is overriding the author",
        m.padding_left
    );
}

// ── 7. the dark-boilerplate case, end to end ─────────────────────────────

// The actual reported bug, as one assertion: dark field, light value text,
// muted placeholder, no white box anywhere.
#[test]
fn the_boilerplates_dark_input_renders_as_a_dark_field() {
    let v = field(
        "width:240px;height:44px;background:#16213e;color:#e6edf3;\
         border:1px solid #3a3a5c;border-radius:8px;font-size:15px;padding:10px 12px",
        &[("placeholder", "What needs doing?")],
    );
    let buf = render(&v);
    let b = field_box(&v);
    let mid = px(&buf, b.x + b.w - 12, b.y + b.h / 2);
    assert_eq!(
        (mid[0], mid[1], mid[2]),
        (0x16, 0x21, 0x3e),
        "the boilerplate's dark field renders as {mid:?}: the hardcoded white \
         fallback is still winning over `background:#16213e`"
    );
}
