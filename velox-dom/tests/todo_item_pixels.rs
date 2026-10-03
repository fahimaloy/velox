//! PIXEL-LEVEL PROOF of the `.todo-item` row flex defect.
//!
//! This is the owner-reported bug, promoted out of a scratch crate so it lives
//! in git and protects the fix.
//!
//! Invariant 4: `render_vnode_to_rgba` IS used -- it calls `prepare_frame`,
//! which cascades and runs `compute_layout`. `render_vnode_to_raster_png` is
//! NEVER used: it skips `compute_layout` and would false-green.
//!
//! Invariant 5: nothing here re-derives flex maths. Every assertion reads a
//! colour off the rendered buffer.
//!
//! Run with:  cargo test -p velox-dom --features skia-native --test todo_item_pixels

#![cfg(all(feature = "skia-native", unix))]

use velox_dom::{VNode, h};
use velox_renderer::render_vnode_to_rgba;
use velox_style::Stylesheet;

const W: i32 = 400;
const H: i32 = 60;

/// Sample one pixel. `render_vnode_to_rgba` is premultiplied with opaque alpha,
/// so the returned RGB is the colour directly.
fn px(buf: &[u8], w: i32, x: i32, y: i32) -> [u8; 3] {
    let i = ((y * w + x) * 4) as usize;
    [buf[i], buf[i + 1], buf[i + 2]]
}

fn is_close(got: [u8; 3], want: [u8; 3]) -> bool {
    got.iter()
        .zip(want.iter())
        .all(|(g, w)| (*g as i32 - *w as i32).abs() <= 4)
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// A scanline across the middle of the row as a run-length map: the
/// human-readable picture of who owns the row's main axis.
fn runs(buf: &[u8], y: i32) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    let mut cur = String::new();
    let mut n = 0usize;
    for x in 0..W {
        let c = hex(px(buf, W, x, y));
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

/// How many pixels of the scanline are exactly this colour.
fn count_colour(buf: &[u8], y: i32, want: [u8; 3]) -> usize {
    (0..W).filter(|&x| px(buf, W, x, y) == want).count()
}

/// `.todo-item` from `velox-cli/templates/project/src/components/TodoItem.vx`,
/// with the `flex: 1` text span given a solid green background so the space it
/// actually receives is directly readable off the pixel buffer.
///
/// The template's `input.checkbox` is NOT included here: the defect is
/// orthogonal to it (the checkbox carries an explicit `width`, so it never
/// touched the content-size path), and leaving it out keeps this file's
/// before/after scanline directly comparable with the numbers recorded in
/// `flex_content_basis.rs`. The checkbox-inclusive shape is asserted at rect
/// level in that file.
fn css() -> Stylesheet {
    Stylesheet::parse(
        r#"
        .todo-item {
          display: flex; align-items: center; gap: 10px;
          padding: 12px 16px; background: #16213e; border-radius: 10px;
        }
        .todo-text { flex: 1; font-size: 15px; background: #00ff00; }
        .btn { padding: 6px 12px; font-size: 14px; border: none;
               border-radius: 6px; cursor: pointer; color: #ffffff; }
        .btn-danger { background: #ef4444; }
        .btn-small { padding: 4px 10px; }
        "#,
    )
}

fn tree() -> VNode {
    h(
        "div",
        vec![("class", "todo-item")],
        vec![
            h(
                "span",
                vec![("class", "todo-text")],
                vec![VNode::Text("Buy milk".into())],
            ),
            h(
                "button",
                vec![("class", "btn btn-danger btn-small")],
                vec![VNode::Text("×".into())],
            ),
        ],
    )
}

#[test]
fn todo_row_scanline_report() {
    let buf = render_vnode_to_rgba(&tree(), &css(), W, H).expect("render");
    println!("scanline y={} runs: {:#?}", H / 2, runs(&buf, H / 2));
    println!(
        "  x=395 (right edge of row) = {}",
        hex(px(&buf, W, 395, H / 2))
    );
    println!(
        "  x=200 (mid row)           = {}",
        hex(px(&buf, W, 200, H / 2))
    );
    println!(
        "  x=4   (left of row)        = {}",
        hex(px(&buf, W, 4, H / 2))
    );
}

/// The regression assertion. Under a correct flex implementation the `x`
/// button is sized by its CONTENT, so the right-hand end of the row is the
/// row's own background, not button red -- and the `flex: 1` sibling owns the
/// freed middle.
///
/// Before the fix this scanline was `#16213e 19px` then `#ef4444 374px`:
/// the button absorbed 374 of 400px and the `flex: 1` span was painted ZERO
/// pixels.
#[test]
fn non_grow_flex_item_must_not_absorb_the_row() {
    let buf = render_vnode_to_rgba(&tree(), &css(), W, H).expect("render");
    let row_bg = [0x16, 0x21, 0x3e];
    let btn_red = [0xef, 0x44, 0x44];
    let text_green = [0x00, 0xff, 0x00];

    let right = px(&buf, W, W - 5, H / 2);
    println!("right edge = {}", hex(right));
    assert!(
        !is_close(right, btn_red),
        "non-grow `.btn` painted red at x={} -- it absorbed the row's main axis",
        W - 5
    );
    assert!(
        is_close(right, row_bg),
        "right edge should be row background"
    );

    let mid = px(&buf, W, 200, H / 2);
    println!("mid row = {}", hex(mid));
    assert!(
        is_close(mid, text_green),
        "the `flex: 1` `.todo-text` should own the freed middle of the row"
    );

    // The button must still be painted SOMEWHERE, narrow.
    let red = count_colour(&buf, H / 2, btn_red);
    println!("red px = {red} of {W}");
    assert!(red > 0, "the button must still be painted");
    assert!(red < 80, "non-grow button took {red}px of a {W}px row");
}

/// The `i32::MAX` width leak, proved on pixels.
///
/// A row-flex child with a definite `flex-basis` but no explicit `width` used
/// to be measured against the `UNCONSTRAINED_CROSS_SIZE` sentinel
/// (`i32::MAX as f32`) and come out 2147483647px wide, taking the whole row.
///
/// Measured before the fix: the green band below covered the entire 400px
/// scanline. Correct behaviour: `flex: 0 0 20px` is exactly 20px, and the rest
/// of the row is the row's own background.
#[test]
fn a_definite_flex_basis_is_not_widened_by_the_unconstrained_sentinel() {
    let css = Stylesheet::parse(
        r#"
        .row { display: flex; width: 400px; height: 40px;
               background: #16213e; align-items: center; }
        .pinned { flex: 0 0 20px; height: 20px; background: #00ff00; }
        "#,
    );
    // No text child: a glyph would paint over the band and make the exact-colour
    // count below mean "band minus glyph" rather than "band".
    let tree = h(
        "div",
        vec![("class", "row")],
        vec![h("div", vec![("class", "pinned")], vec![])],
    );
    let buf = render_vnode_to_rgba(&tree, &css, W, H).expect("render");

    let green = count_colour(&buf, 20, [0x00, 0xff, 0x00]);
    let row_bg = count_colour(&buf, 20, [0x16, 0x21, 0x3e]);
    println!("green = {green}px, row bg = {row_bg}px of {W}px");
    println!("  x=5  = {}", hex(px(&buf, W, 5, 20)));
    println!("  x=25 = {}", hex(px(&buf, W, 25, 20)));
    println!("  x=200= {}", hex(px(&buf, W, 200, 20)));

    // Inside the green band -> it is sized near 20px, not 400 and not 2^31.
    assert!(
        is_close(px(&buf, W, 5, 20), [0x00, 0xff, 0x00]),
        "x=5 should be green"
    );
    // Just past 20px -> the row's own background. Under the i32::MAX leak this
    // was green all the way across.
    assert!(
        is_close(px(&buf, W, 25, 20), [0x16, 0x21, 0x3e]),
        "x=25 should be row background -- a `flex: 0 0 20px` item leaked the i32::MAX sentinel"
    );
    assert!(
        is_close(px(&buf, W, 200, 20), [0x16, 0x21, 0x3e]),
        "x=200 should be row background"
    );
    assert!(
        green >= 20 && green <= 22,
        "expected ~20px of green, got {green}"
    );
    assert!(
        row_bg >= 378,
        "expected the rest of the row to be background, got {row_bg}px"
    );
}
