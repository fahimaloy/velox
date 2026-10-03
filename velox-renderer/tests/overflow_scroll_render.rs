//! Headless pixel proofs for the scrollable overflow model (skia raster):
//! scrollbar thumb paint, overflow clipping, and scrolled content rendering.
//! Requires `--features skia-native`.

#![cfg(feature = "skia-native")]

use velox_dom::{Props, VNode, h};
use velox_style::{Stylesheet, apply_styles};

fn props(s: &str) -> Props {
    Props::from_inline(s)
}

fn px(rgba: &[u8], w: i32, x: i32, y: i32) -> (u8, u8, u8) {
    let base = ((y as usize) * (w as usize) + x as usize) * 4;
    (rgba[base], rgba[base + 1], rgba[base + 2])
}

/// 200x100 overflow:auto container over 300px of red content on a 200x300
/// canvas (surface clears to transparent black).
fn scrollable_tree() -> VNode {
    h(
        "div",
        props("width:200px; height:100px; overflow:auto;"),
        vec![h(
            "div",
            props("width:200px; height:300px; background:#FF0000;"),
            vec![],
        )],
    )
}

#[test]
fn scrollbar_thumb_painted_on_scrollable_container() {
    let sheet = Stylesheet::default();
    let styled = apply_styles(&scrollable_tree(), &sheet);
    let rgba =
        velox_renderer::render_vnode_to_rgba(&styled, &sheet, 200, 300).expect("render to rgba");

    // Content is painted inside the container...
    let (r, g, b) = px(&rgba, 200, 100, 50);
    assert!(
        r > 200 && g < 60 && b < 60,
        "content visible inside container, got rgb=({r},{g},{b})"
    );
    // ...and clipped below it (surface clears transparent: 0,0,0).
    let (r, g, b) = px(&rgba, 200, 100, 150);
    assert!(
        r < 60 && g < 60 && b < 60,
        "content clipped below container, got rgb=({r},{g},{b})"
    );
    // Scrollbar thumb: gray (100,100,100) at 47% alpha over red content in
    // the top track region (track_x = 200 - 8 - 2 = 190, thumb at track top).
    let (r, g, b) = px(&rgba, 200, 194, 15);
    assert!(
        r > 120 && (20..200).contains(&g) && g != 255 && b < 100,
        "thumb painted near right edge, got rgb=({r},{g},{b})"
    );
    // Left of the track: pure content, no thumb.
    let (r, g, b) = px(&rgba, 200, 150, 15);
    assert!(
        r > 200 && g < 60,
        "no thumb bleeding into content area, got rgb=({r},{g},{b})"
    );
}

#[test]
fn non_scrollable_container_paints_no_thumb() {
    // Same geometry but overflow:visible (not scrollable): no thumb pixels.
    let vnode = h(
        "div",
        props("width:200px; height:100px;"),
        vec![h(
            "div",
            props("width:200px; height:300px; background:#FF0000;"),
            vec![],
        )],
    );
    let sheet = Stylesheet::default();
    let styled = apply_styles(&vnode, &sheet);
    let rgba =
        velox_renderer::render_vnode_to_rgba(&styled, &sheet, 200, 300).expect("render to rgba");

    let (r, g, b) = px(&rgba, 200, 194, 15);
    assert!(
        r > 200 && g < 60,
        "no scrollbar thumb without overflow, got rgb=({r},{g},{b})"
    );
}

#[test]
fn scrolled_content_renders_shifted_with_thumb_at_bottom() {
    // Deprecated synthetic scroll-top (compat path) shifts children by -100:
    // red (0..100) moves out above, green (100..200) becomes visible, and the
    // thumb moves to the bottom of the track (scroll_y == max_scroll_y).
    let vnode = h(
        "div",
        props("width:200px; height:100px; overflow:auto; scroll-top:100px;"),
        vec![
            h(
                "div",
                props("width:200px; height:100px; background:#FF0000;"),
                vec![],
            ),
            h(
                "div",
                props("width:200px; height:100px; background:#00FF00;"),
                vec![],
            ),
        ],
    );
    let sheet = Stylesheet::default();
    let styled = apply_styles(&vnode, &sheet);
    let rgba =
        velox_renderer::render_vnode_to_rgba(&styled, &sheet, 200, 300).expect("render to rgba");

    // Green content revealed at the top of the container (was red unscrolled).
    let (r, g, b) = px(&rgba, 200, 100, 50);
    assert!(
        g > 200 && r < 60 && b < 60,
        "scrolled content (green) visible, got rgb=({r},{g},{b})"
    );
    // Red moved above the container and is clipped away.
    let (r, g, b) = px(&rgba, 200, 100, 150);
    assert!(
        r < 60 && g < 60,
        "below-container area clipped, got rgb=({r},{g},{b})"
    );
    // Thumb at the bottom of the track over green content:
    // thumb_y = 2 + (100/100)*(96 - 33.33) ~= 64.7, so (194, 70) is thumb.
    let (r, g, b) = px(&rgba, 200, 194, 70);
    assert!(
        r < 100 && g > 120 && b < 100,
        "thumb near bottom over scrolled content, got rgb=({r},{g},{b})"
    );
    // ...and no thumb left at the top of the track.
    let (r, g, b) = px(&rgba, 200, 194, 15);
    assert!(
        g > 200 && r < 60,
        "top track shows content, not thumb, got rgb=({r},{g},{b})"
    );
}
