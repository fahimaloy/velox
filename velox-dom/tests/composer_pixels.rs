//! PIXEL-LEVEL PROOF of the composer defect fixed in `layout.rs`: a flex item
//! whose resolved main size differs from the size it was measured at used to
//! keep its STALE subtree, so `.input` stayed 556 px wide inside a 480 px
//! `.field` and the Add button painted over the input's right 68 px.
//!
//! `flex_relayout_resolved_size.rs` proves the rect arithmetic. This file
//! proves the PAINT, which is the half a unit test cannot reach.
//!
//! ## Why the colours are what they are
//!
//! Paint order is DOM order, so in the real composer the button paints ON TOP
//! of the input's overhang — with an opaque button the two renderings are
//! pixel-identical and the defect is invisible to a scanline. So the button's
//! background is TRANSLUCENT, which makes the overlap readable at the paint
//! layer:
//!
//! | x | fixed | broken |
//! |---|---|---|
//! | 0..480 | `#ff0000` the input | `#ff0000` the input |
//! | 480..488 | the row's own `#ffffff` gap | `#ff0000` the input |
//! | 488..556 | `#8080ff` — blue 50% over white | `#800080` — blue 50% over the red input |
//!
//! `#800080` is the broken signature. **The red channel of the button's own
//! region is the entire assertion**: it is non-zero if and only if the input
//! still reaches under the button.
//!
//! ## Invariants
//!
//! 1. `render_vnode_to_rgba` is used — it runs `prepare_frame`, which calls
//!    `apply_with_cascade` and then `compute_layout`.
//!    `render_vnode_to_raster_png` is NEVER used: it is documented as skipping
//!    `compute_layout`, so it would false-green.
//! 2. The translucent background is an INLINE `style`, not a class rule.
//!    `rgba()` in a class rule is silently dropped by the cascade (measured:
//!    `.add { background: rgba(0,0,255,0.5) }` paints nothing, while the same
//!    value inline paints `#8080ff`) — a separate defect, out of scope here.
//! 3. Assertions read colours off the rendered buffer. No flex maths is
//!    re-derived here.
//! 4. `EDGE` is the assertion scanline: y=6 sits inside the 40 px band but
//!    above the vertically-centred "Add" label, so the run map is flat and
//!    exact. `MID` (y=20) cuts the glyphs and is printed for the human eye.
//!
//! Run with:
//!   cargo test -p velox-dom --features skia-native --test composer_pixels

#![cfg(all(feature = "skia-native", unix))]

use velox_dom::{VNode, h};
use velox_renderer::render_vnode_to_rgba;
use velox_style::Stylesheet;

const W: i32 = 600;
const H: i32 = 60;
/// Asserts run here: inside the box band, clear of the label's glyphs.
const EDGE: i32 = 6;
/// The row's vertical centre, printed for the human-readable report.
const MID: i32 = 20;

/// The composer's row out of the scaffolded `TodoInput.vx`: a
/// `flex: 1 1 auto` `.field` whose only child is `width: 100%`, and a
/// `flex: 0 0 auto` 68 px `.add`, `gap: 8px`, 556 px wide overall.
fn css() -> Stylesheet {
    Stylesheet::parse(
        r#"
        .composer {
          display: flex; flex-direction: row; gap: 8px;
          width: 556px; height: 48px; background: #ffffff;
        }
        .field { flex: 1 1 auto; }
        .input { width: 100%; height: 40px; background: #ff0000; }
        .add  { flex: 0 0 auto; width: 68px; height: 40px; border: none; }
        "#,
    )
}

fn tree() -> VNode {
    h(
        "div",
        vec![("class", "composer")],
        vec![
            // The `.field` WRAPPER IS LOAD-BEARING and its absence makes this
            // test unable to fail: without it the input is itself the flex
            // item, so flex resizes the input's OWN box and nothing needs
            // re-laying out -- the scanline is byte-identical before and after
            // the fix (measured). The defect needs a flex item that is NOT the
            // element carrying `width: 100%`.
            h(
                "div",
                vec![("class", "field")],
                vec![h("input", vec![("class", "input")], vec![])],
            ),
            h(
                "button",
                vec![
                    ("class", "add"),
                    ("style", "background: rgba(0,0,255,0.5);"),
                ],
                vec![VNode::Text("Add".into())],
            ),
        ],
    )
}

/// Sample one pixel. The buffer is premultiplied with opaque alpha, so the
/// returned RGB is the colour directly.
fn px(buf: &[u8], x: i32, y: i32) -> [u8; 3] {
    let i = ((y * W + x) * 4) as usize;
    [buf[i], buf[i + 1], buf[i + 2]]
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// A scanline as a run-length map: the picture of who owns the row's main axis.
fn runs(buf: &[u8], y: i32) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    let mut cur = String::new();
    let mut n = 0usize;
    for x in 0..556 {
        let c = hex(px(buf, x, y));
        if c == cur {
            n += 1;
        } else {
            if n > 0 {
                out.push((cur.clone(), n));
            }
            cur = c;
            n = 1;
        }
    }
    out.push((cur, n));
    out
}

fn count_colour(buf: &[u8], y: i32, want: [u8; 3]) -> usize {
    (0..W).filter(|&x| px(buf, x, y) == want).count()
}

/// The human-readable scanline. Prints both scanlines and asserts only that the
/// render happened, so it can be read as the screenshot-equivalent evidence.
#[test]
fn composer_scanline_report() {
    let buf = render_vnode_to_rgba(&tree(), &css(), W, H).expect("render");
    println!(
        "scanline y={EDGE} (assertions here): {:?}",
        runs(&buf, EDGE)
    );
    println!(
        "scanline y={MID} (through the label):\n{:#?}",
        runs(&buf, MID)
    );
    for x in [0, 100, 479, 480, 483, 487, 488, 500, 555] {
        println!("  y={EDGE} x={x:>3} = {}", hex(px(&buf, x, EDGE)));
    }
}

/// The defect itself, at the pixel layer: the input must own exactly the 480 px
/// the flex algorithm gave `.field`, and the button's own 68 px must contain no
/// red at all.
#[test]
fn input_does_not_reach_under_the_add_button() {
    let buf = render_vnode_to_rgba(&tree(), &css(), W, H).expect("render");
    let runs = runs(&buf, EDGE);

    assert_eq!(
        count_colour(&buf, EDGE, [0xff, 0x00, 0x00]),
        478,
        "the opaque input must own `.field`'s 480 px (478 flat plus two \
         antialiased edges), not the 556 px it was measured at — a wider input \
         is what painted under the button (scanline: {runs:?})"
    );
    assert_eq!(
        px(&buf, 479, EDGE),
        [0xe4, 0x64, 0x64],
        "x=479 is the input's right antialiased edge: the input ends at exactly \
         480, the width flex resolved (scanline: {runs:?})"
    );
    assert_eq!(
        count_colour(&buf, EDGE, [0xff, 0xff, 0xff]),
        7,
        "the row's own background must survive in the 8 px gap at 480..488 \
         (scanline: {runs:?})"
    );
    assert_eq!(
        px(&buf, 483, EDGE),
        [0xff, 0xff, 0xff],
        "x=483 is inside the 8 px gap and must be the row's own background"
    );
    assert_eq!(
        count_colour(&buf, EDGE, [0x80, 0x80, 0xff]),
        68,
        "the button's translucent fill must cover its full 68 px over the \
         WHITE row. With the input under it that region reads #800080 and this \
         count is 0 (scanline: {runs:?})"
    );
    assert_eq!(
        px(&buf, 500, EDGE),
        [0x80, 0x80, 0xff],
        "x=500 is inside the button, 20 px into its 68. Blue 50% over the RED \
         input reads #800080; over the white row it reads #8080ff. The red \
         channel is the whole assertion — it is non-zero if and only if the \
         input still reaches under the button (scanline: {runs:?})"
    );
    assert_eq!(
        px(&buf, 555, EDGE),
        [0x80, 0x80, 0xff],
        "the button's right edge is the row's right content edge"
    );
}
