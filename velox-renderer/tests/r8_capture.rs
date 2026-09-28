//! R-8 byte-identity guard for the render prologue funnel
//! (`skia_render::skia_impl::prepare_frame`).
//!
//! Unifying the prologue is a *potential* visual change: `render_vnode_to_rgba`
//! and `render_vnode_to_raster_png_with_scale` used to run the same four steps
//! in two different orders. The baselines below were captured on unmodified
//! HEAD (`6b00fef`, before the funnel existed) and are pinned here so the
//! refactor cannot silently repaint.
//!
//! # How discriminating is this, honestly?
//!
//! `set_current_scale` / `set_skia_measurer` install PROCESS-GLOBAL state, and
//! `prepare_frame` installs them before every `compute_layout`. The global leaks
//! across calls within one process, so a prologue-ordering bug is only visible
//! to whichever entry point runs FIRST in a fresh process; a later call inherits
//! the measurer the earlier call left behind and masks the difference.
//!
//! Measured, not assumed: with the measurer install deliberately moved to AFTER
//! `compute_layout`, `render_vnode_to_rgba` moved `efe29d78` -> `595f757c`, and a
//! PNG-only run in a *fresh* process moved scale=1.0 `1cb2561b` -> `cfa88f1c`.
//! Run in the same process after an `rgba` call, the PNG cases did not move.
//! Both probes were run as separate `--test` targets precisely to get the
//! fresh-process case; combining them hides it.
//!
//! # Deliberately not used
//!
//! `render_vnode_to_raster_png` is never called here: it runs no `compute_layout`
//! at all, so it would false-green (invariant 4).

#![cfg(all(feature = "skia-native", unix))]

use velox_dom::{VNode, h};
use velox_renderer::{render_vnode_to_raster_png_with_scale, render_vnode_to_rgba};
use velox_style::Stylesheet;

fn fnv1a(bytes: &[u8]) -> u32 {
    let mut hash: u32 = 0x811c9dc5;
    for b in bytes {
        hash ^= *b as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}

/// A deliberately non-trivial tree: nested elements, styled children of
/// differing sizes, and text, so a prologue ORDER change has something to move.
fn tree() -> VNode {
    h(
        "div",
        vec![("style", "width:240px;background:#f0f0f0")],
        vec![
            h(
                "div",
                vec![("style", "width:120px;height:40px;background:#ff0000")],
                vec![VNode::Text("alpha".to_string())],
            ),
            h(
                "span",
                vec![("style", "width:200px;height:17px;background:#00ff00")],
                vec![VNode::Text("beta gamma".to_string())],
            ),
            h(
                "div",
                vec![("style", "width:80px;height:9px;background:#0000ff")],
                vec![],
            ),
        ],
    )
}

/// Baselines captured on unmodified HEAD, before the funnel existed.
const RGBA_LEN: usize = 240_000;
const RGBA_HASH: u32 = 0xefe2_9d78;
const PNG_BASELINES: [(f32, usize, u32); 5] = [
    (1.0, 1981, 0x1cb2_561b),
    (1.5, 3293, 0x1c93_6207),
    (2.0, 4684, 0x2290_2f92),
    // A degenerate scale is substituted with 1.0 by the single rounding
    // authority (`viewport::physical_from_logical`), so 0.0 and NaN render
    // exactly what 1.0 renders. Pinned so that substitution can never silently
    // change.
    (0.0, 1981, 0x1cb2_561b),
    (f32::NAN, 1981, 0x1cb2_561b),
];

#[test]
fn the_funnel_preserves_the_bytes_head_produced() {
    let sheet = Stylesheet::default();
    let v = tree();

    let rgba = render_vnode_to_rgba(&v, &sheet, 300, 200).expect("rgba render");
    assert_eq!(rgba.len(), RGBA_LEN, "rgba buffer length changed");
    assert_eq!(
        fnv1a(&rgba),
        RGBA_HASH,
        "render_vnode_to_rgba no longer produces the bytes HEAD produced"
    );

    for (scale, want_len, want_hash) in PNG_BASELINES {
        let png = render_vnode_to_raster_png_with_scale(&v, &sheet, 300, 200, scale)
            .expect("scaled png render");
        assert_eq!(png.len(), want_len, "png at scale {scale} changed length");
        assert_eq!(
            fnv1a(&png),
            want_hash,
            "render_vnode_to_raster_png_with_scale at scale {scale} no longer \
             produces the bytes HEAD produced"
        );
    }
}
