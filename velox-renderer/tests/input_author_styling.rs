//! A4: the author's own `background`, `color`, `border`, `border-radius` and
//! `padding` must reach the `<input>` paint lane.
//!
//! ## The bug
//!
//! The input lane was special-cased and hardcoded three things the author had
//! already declared: an opaque white fill (painted OVER the author's
//! `background`), a `#c8c8c8` border colour, and near-black value text. The
//! element box pass above it had already painted `background` and `border`
//! correctly, so the white fill was not merely a default — it was an override.
//! The boilerplate's `.input`
//! (`background:#16213e; color:#e6edf3; border:1px solid #3a3a5c;
//! border-radius:8px`) rendered as a white box with black text.
//!
//! Additionally the border was drawn with `draw_rect`, so `border-radius` was
//! defeated on the outline even when the fill honoured it.
//!
//! ## What is asserted
//!
//! Each test is a pixel reading of the field's own box, obtained from
//! `compute_layout` — the same box the renderer consumes. No test re-derives
//! the painter's arithmetic; where an inset is involved the test asks
//! `input_metrics::input_text_metrics`, the painter's own authority.
//!
//! The page behind the field is pure black and every authored colour is chosen
//! far from it, so "the author's colour is not on screen" cannot be confused
//! with a subtle blend.

#![cfg(feature = "skia-native")]

use velox_dom::{Props, VNode, h};
use velox_renderer::render_vnode_to_rgba;
use velox_style::Stylesheet;

const W: i32 = 300;
const H: i32 = 120;

/// Pure black, so anything painted is unmistakable and nothing is accidental.
const PAGE: &str = "background:#000000";

fn px(buf: &[u8], x: i32, y: i32) -> [u8; 4] {
    let i = ((y * W + x) * 4) as usize;
    [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
}

fn render(v: &VNode) -> Vec<u8> {
    render_vnode_to_rgba(v, &Stylesheet::default(), W, H).expect("render to rgba")
}

/// A page whose only content is one `<input>` with the given style and value.
fn scene(style: &str, value: &str) -> VNode {
    let props = Props::new()
        .set("type", "text")
        .set("style", style)
        .set("value", value);
    h(
        "div",
        Props::new().set("style", PAGE),
        vec![h("input", props, vec![])],
    )
}

/// The field's border box, from the layout the renderer itself consumes.
fn field_box(v: &VNode) -> velox_dom::layout::Rect {
    let styled = velox_style::apply_with_cascade(v, &Stylesheet::default());
    let layout = velox_dom::layout::compute_layout(&styled, W, H);
    layout.children[0].rect
}

/// The CASCADED style string the painter received for the field in `v`.
///
/// The UA sheet contributes `padding` and `min-height` to every `<input>`, so
/// the authored string is not what the painter read.
fn cascaded_style(v: &VNode) -> String {
    fn walk(node: &VNode) -> Option<String> {
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
                children.iter().find_map(walk)
            }
        }
    }
    let styled = velox_style::apply_with_cascade(v, &Stylesheet::default());
    walk(&styled).expect("the field must carry a style")
}

fn metrics_for(v: &VNode) -> velox_renderer::input_metrics::InputTextMetrics {
    let box_ = field_box(v);
    velox_renderer::input_metrics::input_text_metrics(
        Some(&cascaded_style(v)),
        box_,
        (W as f32, H as f32),
        velox_dom::layout::DEFAULT_ROOT_FONT_SIZE,
    )
}

/// The colour at the CENTRE of a box, which is inside the fill and outside both
/// the border and any radius.
fn centre(buf: &[u8], b: &velox_dom::layout::Rect) -> [u8; 4] {
    px(buf, b.x + b.w / 2, b.y + b.h / 2)
}

// ── background ──────────────────────────────────────────────────────────

// The headline bug: the author's dark field was painted white. Chosen far from
// white so a white fill cannot pass.
#[test]
fn the_authors_background_reaches_the_field_instead_of_the_white_fallback() {
    let v = scene("width:200px;height:60px;background:#16213e", "");
    let buf = render(&v);
    let got = centre(&buf, &field_box(&v));
    assert_eq!(
        (got[0], got[1], got[2]),
        (0x16, 0x21, 0x3e),
        "the field's fill is {got:?}: the hardcoded opaque white is winning over \
         the author's `background`"
    );
}

// A translucent author background must stay translucent — painting it opaque is
// the same class of defect as painting it white.
//
// The page behind is opaque black, so what a 50%-alpha red composites to is
// ~127, not 255. Asserting on the CHANNEL rather than on the output alpha is
// deliberate: the composited result is correctly opaque, because the page under
// it is. A 255 red here means the input lane painted the colour twice (or
// dropped the alpha entirely), which is exactly what re-filling the box the
// element pass already filled used to do.
#[test]
fn a_translucent_author_background_is_not_painted_twice() {
    let v = scene("width:200px;height:60px;background:rgba(255,0,0,0.5)", "");
    let buf = render(&v);
    let got = centre(&buf, &field_box(&v));
    assert!(
        got[0] < 200,
        "50% red over a black page composites to ~127; got {got:?}. A value near \
         255 means the fill was composited more than once"
    );
    assert!(
        got[0] > 60,
        "50% red over a black page composites to ~127; got {got:?}. A value near \
         0 means the alpha was discarded"
    );
}

// The hardcoded white survives only as the fallback, so an author who specifies
// nothing still gets something that reads as a field.
#[test]
fn an_undeclared_background_still_falls_back_to_white() {
    let v = scene("width:200px;height:60px", "");
    let buf = render(&v);
    let got = centre(&buf, &field_box(&v));
    assert!(
        got[0] > 200 && got[1] > 200 && got[2] > 200 && got[3] == 255,
        "an input with no declared background must still read as a light field, \
         got {got:?}"
    );
}

// ── border ──────────────────────────────────────────────────────────────

// The other hardcoded literal: `#c8c8c8` painted over the author's border.
#[test]
fn the_authors_border_colour_reaches_the_outline() {
    let v = scene(
        "width:200px;height:60px;background:#000000;border:4px solid #ff0000",
        "",
    );
    let buf = render(&v);
    let b = field_box(&v);
    // The border band lives on the border box's edge, half the stroke inside.
    let got = px(&buf, b.x + 2, b.y + b.h / 2);
    assert!(
        got[0] > 180 && got[1] < 60 && got[2] < 60,
        "the border band reads {got:?}: the author's red `border` did not reach \
         the outline (the `#c8c8c8` fallback is winning)"
    );
}

// The width the author asked for must be visible, not clipped. A 4px border is
// read at its centre row; if the outline were drawn inside the padding box it
// would be clipped to nothing.
#[test]
fn the_authors_border_width_is_fully_visible() {
    let thin = render(&scene(
        "width:200px;height:60px;background:#000000;border:1px solid #ff0000",
        "",
    ));
    let thick = render(&scene(
        "width:200px;height:60px;background:#000000;border:6px solid #ff0000",
        "",
    ));
    let thin_b = field_box(&scene(
        "width:200px;height:60px;background:#000000;border:1px solid #ff0000",
        "",
    ));
    let thick_b = field_box(&scene(
        "width:200px;height:60px;background:#000000;border:6px solid #ff0000",
        "",
    ));
    // Count red pixels along the horizontal centre line: a 6px border is drawn
    // half inside and half outside the box, so the band is roughly twice as
    // wide as the 1px one plus its AA fringe.
    let red_run = |buf: &[u8], b: &velox_dom::layout::Rect| -> i32 {
        (0..b.w)
            .filter(|i| px(buf, b.x + i, b.y + b.h / 2)[0] > 150)
            .count() as i32
    };
    let a = red_run(&thin, &thin_b);
    let c = red_run(&thick, &thick_b);
    assert!(a >= 1, "a 1px red border drew nothing at all ({a}px)");
    assert!(
        c >= a + 4,
        "a 6px border drew {c}px of red against {a}px for 1px: the requested \
         width is not reaching the canvas"
    );
}

#[test]
fn the_default_border_colour_survives_as_the_fallback() {
    let v = scene("width:200px;height:60px;background:#ffffff", "");
    let buf = render(&v);
    let b = field_box(&v);
    let got = px(&buf, b.x, b.y + b.h / 2);
    assert!(
        got[0] > 150 && got[1] > 150 && got[2] > 150,
        "an input with no declared border must still be outlined, got {got:?}"
    );
}

// ── border-radius ───────────────────────────────────────────────────────

// The border was drawn with `draw_rect`, so an author's `border-radius` was
// defeated on the outline. A rounded field's extreme corner pixel must be the
// PAGE, not the field's fill — that is the signature of a radius.
#[test]
fn a_border_radius_reaches_the_outline_not_just_the_fill() {
    let rounded = scene(
        "width:200px;height:60px;background:#16213e;border:4px solid #ff0000;border-radius:20px",
        "",
    );
    let square = scene(
        "width:200px;height:60px;background:#16213e;border:4px solid #ff0000;border-radius:0",
        "",
    );
    let rb = field_box(&rounded);
    let sb = field_box(&square);
    let corner = |buf: &[u8], b: &velox_dom::layout::Rect| px(buf, b.x + 1, b.y + 1);

    let rc = corner(&render(&rounded), &rb);
    let sc = corner(&render(&square), &sb);
    assert_eq!(
        sc[0], 255,
        "control: a square-cornered field must paint its very corner, got {sc:?}"
    );
    assert_ne!(
        rc, sc,
        "a 20px radius field painted its extreme corner pixel {rc:?} exactly as \
         the square one did: `border-radius` is not reaching the border"
    );
    assert!(
        rc[0] < 40 && rc[1] < 40 && rc[2] < 40,
        "the rounded corner should be the black page showing through, got {rc:?}"
    );
}

// The fallback outline must not reintroduce the square corner: a field with a
// radius but no declared border still needs a rounded outline.
#[test]
fn the_fallback_outline_honours_the_radius_too() {
    let v = scene(
        "width:200px;height:60px;background:#16213e;border-radius:20px",
        "",
    );
    let b = field_box(&v);
    let corner = px(&render(&v), b.x + 1, b.y + 1);
    assert!(
        corner[0] < 40 && corner[1] < 40 && corner[2] < 40,
        "the no-author-border fallback outline must still be rounded; its corner \
         reads {corner:?}"
    );
}

// ── value colour ────────────────────────────────────────────────────────

// The value text was painted hardcoded near-black regardless of `color`, which
// on the boilerplate's dark field meant black-on-dark.
#[test]
fn the_value_text_uses_the_authors_colour() {
    let v = scene(
        "width:200px;height:60px;background:#000000;color:#e6edf3;font-size:20px",
        "todo",
    );
    let buf = render(&v);
    // Black text on a black field would produce no light pixel anywhere; the
    // author's `#e6edf3` must.
    let b = field_box(&v);
    let found_light = (b.x..b.x + b.w).any(|x| {
        (b.y..b.y + b.h).any(|y| {
            let p = px(&buf, x, y);
            p[0] > 120 && p[1] > 120 && p[2] > 120
        })
    });
    assert!(
        found_light,
        "no light pixel anywhere in the field: `color:#e6edf3` did not reach the \
         value text, which is still painted near-black"
    );
}

// The complementary control: with a DARK `color` on a LIGHT field there must be
// dark ink, i.e. the colour is actually read rather than the field happening to
// contain light pixels from something else.
#[test]
fn a_dark_author_colour_paints_dark_ink_and_not_the_hardcoded_black_forever() {
    let v = scene(
        "width:200px;height:60px;background:#ffffff;color:#ff0000;font-size:20px",
        "todo",
    );
    let buf = render(&v);
    let b = field_box(&v);
    let reddish = (b.x..b.x + b.w).any(|x| {
        (b.y..b.y + b.h).any(|y| {
            let p = px(&buf, x, y);
            p[0] > 120 && p[1] < 90 && p[2] < 90
        })
    });
    assert!(
        reddish,
        "a red value colour on a white field produced no reddish pixel: the value \
         ink is still a hardcoded literal"
    );
}

// ── padding (A3's cross-check) ──────────────────────────────────────────

// The three spellings must place the first glyph identically. This is the
// shorthand bug the whole `input_metrics` module exists to fix: the hit-test
// lane looked up the literal key `"padding-left"`, which the cascade never
// writes for an authored `padding`.
#[test]
fn the_shorthand_the_longhand_and_an_inline_length_agree() {
    let by_authored = |s: &str| metrics_for(&scene(s, "x")).text_left;
    let shorthand = by_authored("width:200px;height:60px;padding:10px 12px");
    let longhand = by_authored("width:200px;height:60px;padding-left:12px");
    let inline = by_authored("width:200px;height:60px;padding-left:12px;padding-top:10px");
    assert_eq!(
        shorthand, longhand,
        "`padding:10px 12px` and `padding-left:12px` put the text at different x"
    );
    assert_eq!(shorthand, inline);
    // The field sits at x=0 with no border, so `padding-left:12px` puts the text
    // origin at exactly 12. The point is not the number — it is that all three
    // spellings land on the SAME number, which the old cascade could not do
    // because it stored the shorthand unexpanded under the key `padding` while the
    // hit-test lane looked up `padding-left`.
    assert_eq!(
        shorthand, 12.0,
        "text_left must be exactly the authored 12px padding from the box edge"
    );
}

// And the pixel consequence: widening the left padding must move the ink right.
#[test]
fn a_wider_padding_moves_the_glyphs_right_on_screen() {
    let narrow_v = scene(
        "width:200px;height:60px;background:#000000;color:#ffffff;font-size:20px;padding-left:6px",
        "todo",
    );
    let wide_v = scene(
        "width:200px;height:60px;background:#000000;color:#ffffff;font-size:20px;padding-left:40px",
        "todo",
    );
    let (narrow, wide) = (render(&narrow_v), render(&wide_v));
    // Scan INSIDE the field's own box, skipping the 4px outer ring: the fallback
    // `#c8c8c8` outline sits there when no border is declared, and it is light
    // enough that scanning the whole page would find it first on every render.
    let first_light = |buf: &[u8], b: &velox_dom::layout::Rect| -> i32 {
        (b.y + 4..b.y + b.h - 4)
            .find_map(|y| (b.x + 4..b.x + b.w - 4).find(|x| px(buf, *x, y)[0] > 120))
            .unwrap_or(-1)
    };
    let a = first_light(&narrow, &field_box(&narrow_v));
    let c = first_light(&wide, &field_box(&wide_v));
    assert!(
        a >= 0 && c >= 0,
        "no light glyph pixels found in one of the fields"
    );
    assert!(
        c > a + 20,
        "widening the left padding by 34px moved the first glyph from x={a} to \
         x={c}: the padding is not reaching the paint"
    );
}

// An explicit `padding: 0` must be honoured rather than tripping the
// historical 4px fallback — a fallback that cannot be turned off is not a
// fallback.
#[test]
fn an_explicit_zero_padding_is_not_replaced_by_a_fallback() {
    let zero = metrics_for(&scene("width:200px;height:60px;padding:0", "x"));
    let none = metrics_for(&scene("width:200px;height:60px", "x"));
    assert_eq!(
        zero.padding_left, 0.0,
        "`padding:0` must produce zero padding, got {}",
        zero.padding_left
    );
    assert!(
        none.padding_left > 0.0,
        "an input with no padding declared must still get the UA/legacy inset"
    );
}
