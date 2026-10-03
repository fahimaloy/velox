//! Wrapped text goes through the real paint pipeline, and every line lands.
//!
//! ## What this file used to be
//!
//! A checksum: it rendered `"Hello from the Velox renderer"` into an 80px box,
//! hashed the PNG and compared the hash to `0x6146e6d8`, a value recorded from
//! whatever the renderer produced at the time. That is a golden of the defect.
//! The renderer resolved the text `VNode` once per line box and always drew line
//! 0, so the output it pinned was a paragraph whose first line is repeated on
//! every row and whose remaining lines are missing — and hashing it proves
//! nothing about which of those it is. A hash also cannot fail informatively:
//! any pixel anywhere changes it, so a failure says "something moved" and never
//! says what.
//!
//! ## What it is now
//!
//! The same paragraph, through the pipeline that actually lays text out, with a
//! geometric assertion: one band of ink per layout line box, each band on its
//! own rows, and the widths of the bands unequal. Under the defect every band
//! had line 0's glyphs, so the bands were identical and the later lines had
//! none of their own text.
//!
//! `render_vnode_to_rgba` is the entry point, not `render_vnode_to_raster_png`:
//! the latter is a proof-of-concept path that draws without calling
//! `compute_layout`, so on that path a text node's box is whatever the
//! proof-of-concept defaults to and this assertion would be measuring nothing.

#![cfg(all(feature = "skia-native", unix))]

use velox_dom::{Props, h, text};
use velox_renderer::render_vnode_to_rgba;
use velox_style::Stylesheet;

const W: i32 = 96;
const H: i32 = 96;

/// The white box, in the surface's own pixels.
const BOX_W: i32 = 80;
const PAGE: [u8; 4] = [255, 255, 255, 255];

#[test]
fn every_wrapped_line_of_a_paragraph_is_painted_once() {
    let vnode = h(
        "div",
        Props::new().set(
            "style",
            "background:#ffffff;color:#000000;width:80px;height:96px",
        ),
        vec![text("Hello from the Velox renderer")],
    );
    let buf = render_vnode_to_rgba(&vnode, &Stylesheet::default(), W, H).expect("render to rgba");
    // Lay out after rendering, and of the CASCADED tree: that is what the paint
    // followed. The render installs the real font measurer globally, so a layout
    // taken before it would be a different face's advances than the ones the
    // glyphs were drawn at; and `prepare_frame` cascades before it lays out
    // (`skia_render.rs:1433-1436`), and the UA sheet's `box-sizing: border-box`
    // is what decides where a box ends.
    let styled = velox_style::apply_with_cascade(&vnode, &Stylesheet::default());
    let laid = velox_dom::layout::compute_layout(&styled, W, H);
    let lines: Vec<_> = laid
        .children
        .iter()
        .filter(|c| c.source_index.is_some())
        .collect();
    assert!(
        lines.len() >= 2,
        "an 80px box must wrap this sentence; layout produced {} line box(es): {lines:?}",
        lines.len()
    );

    // Ink bands, top to bottom, grouped across a one-row gap.
    //
    // Only the box's own columns are scanned. The surface is wider than the box,
    // and the root has no background, so everything to the right of column 80 is
    // transparent black — `[0,0,0,0]`, which is not `PAGE` and would otherwise
    // read as a solid band of ink.
    let mut rows: Vec<(i32, i32, i32, usize)> = Vec::new();
    for y in 0..H {
        let (mut left, mut right, mut ink) = (None, None, 0usize);
        for x in 0..BOX_W {
            let i = ((y * W + x) * 4) as usize;
            let p = [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]];
            if p[3] == 255 && p != PAGE {
                left.get_or_insert(x);
                right = Some(x);
                ink += 1;
            }
        }
        if let (Some(l), Some(r)) = (left, right) {
            rows.push((y, l, r, ink));
        }
    }
    let mut bands: Vec<(i32, i32, i32, i32, usize)> = Vec::new();
    for (y, l, r, ink) in rows {
        match bands.last_mut() {
            Some(last) if last.1 + 1 == y => {
                last.1 = y;
                last.2 = last.2.min(l);
                last.3 = last.3.max(r);
                last.4 += ink;
            }
            _ => bands.push((y, y, l, r, ink)),
        }
    }

    assert_eq!(
        bands.len(),
        lines.len(),
        "one band of ink per line box: {lines:?} vs bands {bands:?} — a line box with \
         no ink is a line of text that was never drawn"
    );
    for (i, (b, l)) in bands.iter().zip(&lines).enumerate() {
        assert!(
            b.0 >= l.rect.y - 2 && b.1 <= l.rect.y + l.rect.h - 1 + 2,
            "band {i} is on rows {}..{} but line box {i} is rows {}..{}",
            b.0,
            b.1,
            l.rect.y,
            l.rect.y + l.rect.h - 1
        );
        assert!(b.4 > 0, "band {i} is empty");
    }
    for (i, w) in bands.windows(2).enumerate() {
        assert_ne!(
            (w[0].2, w[0].3),
            (w[1].2, w[1].3),
            "bands {i} and {} cover the same columns: the same glyphs have been \
             painted on two lines, which is what drawing line 0 once per line box \
             looks like. Bands: {bands:?}",
            i + 1
        );
    }
}
