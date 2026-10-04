//! Headless render proof for the showcase example.
//!
//! Rasterizes the compiled `App.vx` tree on the CPU (no window, no compositor)
//! and writes proof PNGs to `target/velox-render-proof/showcase-<W>x<H>.png`.
//! The assertions double as a parity gallery check: spacing/flex fills, the
//! `margin: 0 auto` centered block, and the scroll container are all painted.

use std::sync::Arc;

use velox_dom::layout::{LayoutNode, Rect, compute_layout};
use velox_renderer::{render_vnode_to_raster_png_with_scale, render_vnode_to_rgba};
use velox_style::{Stylesheet, apply_with_cascade};

include!(concat!(env!("OUT_DIR"), "/app.rs"));

/// Colors declared in `src/App.vx`.
const SECTION_BG: [u8; 3] = [30, 41, 59]; // .section
const CENTERED_BG: [u8; 3] = [167, 139, 250]; // .centered
const PAD_A_BG: [u8; 3] = [56, 189, 248]; // .box-a
const PAD_B_BG: [u8; 3] = [74, 222, 128]; // .box-b
const PAD_C_BG: [u8; 3] = [251, 191, 36]; // .box-c

const LARGE: (i32, i32) = (1280, 800);
const SMALL: (i32, i32) = (480, 360);

fn proof_dir() -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("velox-render-proof");
    std::fs::create_dir_all(&dir).expect("create proof dir");
    dir
}

fn render(state: &Arc<app::script_rs::State>, width: i32, height: i32) -> (Vec<u8>, Vec<u8>) {
    let vnode = app::render_with_state(Arc::clone(state), app::make_resolve(Arc::clone(state)));
    let sheet = Stylesheet::parse(app::STYLE);
    let png = render_vnode_to_raster_png_with_scale(&vnode, &sheet, width, height, 1.0)
        .expect("raster png");
    let rgba = render_vnode_to_rgba(&vnode, &sheet, width, height).expect("raster rgba");
    (png, rgba)
}

fn write_proof(name: &str, width: i32, height: i32, png: &[u8]) -> std::path::PathBuf {
    let path = proof_dir().join(format!("{name}-{width}x{height}.png"));
    std::fs::write(&path, png).expect("write proof png");
    path
}

fn pixels_near(rgba: &[u8], color: [u8; 3], tolerance: i32) -> usize {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .filter(|px| {
            (px[0] as i32 - color[0] as i32).abs() <= tolerance
                && (px[1] as i32 - color[1] as i32).abs() <= tolerance
                && (px[2] as i32 - color[2] as i32).abs() <= tolerance
        })
        .count()
}

fn distinct_colors(rgba: &[u8]) -> usize {
    let mut seen = std::collections::HashSet::new();
    for px in rgba.as_chunks::<4>().0 {
        seen.insert((px[0] / 16, px[1] / 16, px[2] / 16));
    }
    seen.len()
}

fn assert_non_trivial(name: &str, png: &[u8], rgba: &[u8]) {
    assert!(
        png.len() > 1000,
        "{name}: png suspiciously small ({} bytes)",
        png.len()
    );
    assert!(
        distinct_colors(rgba) > 4,
        "{name}: render is flat ({} distinct colors)",
        distinct_colors(rgba)
    );
}

fn collect_layout_matches(
    vnode: &velox_dom::VNode,
    layout: &LayoutNode,
    class: &str,
    matches: &mut Vec<LayoutNode>,
) {
    if let velox_dom::VNode::Element { props, .. } = vnode
        && props
            .attrs
            .get("class")
            .map(|value| value.split_whitespace().any(|item| item == class))
            .unwrap_or(false)
    {
        matches.push(layout.clone());
    }
    if let velox_dom::VNode::Element { children, .. } = vnode {
        for child_layout in &layout.children {
            let Some(source_index) = child_layout.source_index else {
                continue;
            };
            if let Some(child) = children.get(source_index) {
                collect_layout_matches(child, child_layout, class, matches);
            }
        }
    }
}

fn first_layout_match(vnode: &velox_dom::VNode, layout: &LayoutNode, class: &str) -> LayoutNode {
    let mut matches = Vec::new();
    collect_layout_matches(vnode, layout, class, &mut matches);
    matches
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("missing layout class {class}"))
}

fn collect_vnode_matches<'a>(
    vnode: &'a velox_dom::VNode,
    class: &str,
    matches: &mut Vec<&'a velox_dom::VNode>,
) {
    if let velox_dom::VNode::Element {
        props, children, ..
    } = vnode
    {
        if props
            .attrs
            .get("class")
            .map(|value| value.split_whitespace().any(|item| item == class))
            .unwrap_or(false)
        {
            matches.push(vnode);
        }
        for child in children {
            collect_vnode_matches(child, class, matches);
        }
    }
}

fn style_attr(vnode: &velox_dom::VNode) -> Option<&str> {
    if let velox_dom::VNode::Element { props, .. } = vnode {
        props.attrs.get("style").map(String::as_str)
    } else {
        None
    }
}

fn light_ink_bounds(
    rgba: &[u8],
    rect: Rect,
    viewport_width: usize,
    viewport_height: usize,
) -> Option<(usize, usize)> {
    let left = rect.x.max(0) as usize;
    let top = rect.y.max(0) as usize;
    let right = ((rect.x + rect.w).max(0) as usize).min(viewport_width);
    let bottom = ((rect.y + rect.h).max(0) as usize).min(viewport_height);
    let mut min_x = None;
    let mut max_x = None;
    for y in top..bottom {
        for x in left..right {
            let offset = (y * viewport_width + x) * 4;
            if rgba[offset] > 100 && rgba[offset + 1] > 100 && rgba[offset + 2] > 100 {
                min_x = Some(min_x.map_or(x, |value: usize| value.min(x)));
                max_x = Some(max_x.map_or(x, |value: usize| value.max(x)));
            }
        }
    }
    min_x.zip(max_x)
}

fn assert_feature_geometry(state: &Arc<app::script_rs::State>, width: i32, height: i32) {
    let raw = app::render_with_state(Arc::clone(state), app::make_resolve(Arc::clone(state)));
    let vnode = apply_with_cascade(&raw, &Stylesheet::parse(app::STYLE));
    let layout = compute_layout(&vnode, width, height);

    let centered = first_layout_match(&vnode, &layout, "centered");
    let mut sections = Vec::new();
    collect_layout_matches(&vnode, &layout, "section", &mut sections);
    let section = sections
        .into_iter()
        .find(|candidate| {
            candidate.rect.x <= centered.rect.x
                && candidate.rect.x + candidate.rect.w >= centered.rect.x + centered.rect.w
        })
        .expect("centered block should be inside a section");
    assert_eq!(
        centered.rect.w, 320,
        "centered block must keep its fixed width"
    );
    let section_center = section.rect.x + section.rect.w / 2;
    let block_center = centered.rect.x + centered.rect.w / 2;
    assert!(
        (section_center - block_center).abs() <= 10,
        "margin auto is not centering the block: section_center={section_center}, block_center={block_center}"
    );

    let scroll = first_layout_match(&vnode, &layout, "scroll-box");
    assert!(scroll.scrollable, "scroll container should be scrollable");
    assert!(
        scroll.max_scroll_y > 0 && scroll.scroll_height > scroll.rect.h,
        "scroll content does not overflow: scroll_height={}, height={}, max_scroll_y={}",
        scroll.scroll_height,
        scroll.rect.h,
        scroll.max_scroll_y
    );

    let mut wrap_cells = Vec::new();
    collect_layout_matches(&vnode, &layout, "wrap-cell", &mut wrap_cells);
    assert!(wrap_cells.len() >= 4, "wrap demo should contain four cells");
    let first_row = wrap_cells[0].rect.y;
    assert!(wrap_cells[1].rect.y == first_row && wrap_cells[2].rect.y == first_row);
    assert!(
        wrap_cells[3].rect.y > first_row,
        "four 30% flex cells did not wrap onto a second row"
    );
    assert!(
        (wrap_cells[3].rect.x - wrap_cells[0].rect.x).abs() <= 2,
        "wrapped row did not restart at the row's left edge"
    );
    assert_eq!(wrap_cells[0].rect.w, wrap_cells[1].rect.w);
    assert_eq!(wrap_cells[0].rect.w, wrap_cells[2].rect.w);

    let mut lines = Vec::new();
    collect_layout_matches(&vnode, &layout, "line", &mut lines);
    let mut line_nodes = Vec::new();
    collect_vnode_matches(&vnode, "line", &mut line_nodes);
    assert!(
        lines.len() >= 2 && line_nodes.len() >= 2,
        "showcase should have center and right lines"
    );
    let center_style = style_attr(line_nodes[0]).expect("center line style");
    let right_style = style_attr(line_nodes[1]).expect("right line style");
    assert!(center_style.contains("text-align: center"));
    assert!(right_style.contains("text-align: right"));

    if width > 600 {
        let viewport_width = width as usize;
        let viewport_height = height as usize;
        let (_, rgba) = render(state, width, height);
        let centered_ink = light_ink_bounds(&rgba, lines[0].rect, viewport_width, viewport_height)
            .expect("centered line should have visible text");
        let center_offset = ((centered_ink.0 + centered_ink.1) as i64 / 2
            - (lines[0].rect.x + lines[0].rect.w / 2) as i64)
            .abs();
        assert!(
            center_offset <= 40,
            "centered line is not text-align center"
        );

        let right_ink = light_ink_bounds(&rgba, lines[1].rect, viewport_width, viewport_height)
            .expect("right line should have visible text");
        let right_edge = (lines[1].rect.x + lines[1].rect.w) as usize;
        let line_midpoint = (lines[1].rect.x + lines[1].rect.w / 2) as usize;
        assert!(
            right_ink.1 > line_midpoint && right_edge - right_ink.1 <= 40,
            "right line is not text-aligned right"
        );
    }
}

#[test]
fn renders_large_viewport_proof_png() {
    let state = Arc::new(app::script_rs::State::new());
    let (png, rgba) = render(&state, LARGE.0, LARGE.1);
    assert_non_trivial("showcase-large", &png, &rgba);
    assert_feature_geometry(&state, LARGE.0, LARGE.1);
    // The spacing, centered, and padding demonstrations paint their own
    // targeted fills; flex/scroll/wrap behavior is covered by the geometry
    // assertions rather than one pooled cell-color population.
    assert!(pixels_near(&rgba, SECTION_BG, 8) > 2000, "section cards");
    assert!(pixels_near(&rgba, CENTERED_BG, 12) > 500, "centered block");
    assert!(pixels_near(&rgba, PAD_A_BG, 12) > 200, "padding card A");
    assert!(pixels_near(&rgba, PAD_B_BG, 12) > 200, "padding card B");
    assert!(pixels_near(&rgba, PAD_C_BG, 12) > 100, "padding card C");
    let path = write_proof("showcase", LARGE.0, LARGE.1, &png);
    assert!(path.exists(), "proof png missing: {}", path.display());
}

#[test]
fn small_viewport_renders_visible_gallery_sections() {
    let state = Arc::new(app::script_rs::State::new());
    let (png, rgba) = render(&state, SMALL.0, SMALL.1);
    assert_non_trivial("showcase-small", &png, &rgba);
    assert_feature_geometry(&state, SMALL.0, SMALL.1);

    // Prove that the later sections are still present in layout and are
    // placed below the first viewport, rather than inferring that from pixels
    // that happen to be in the first few sections.
    let raw = app::render_with_state(Arc::clone(&state), app::make_resolve(Arc::clone(&state)));
    let vnode = apply_with_cascade(&raw, &Stylesheet::parse(app::STYLE));
    let layout = compute_layout(&vnode, SMALL.0, SMALL.1);
    let centered = first_layout_match(&vnode, &layout, "centered");
    let scroll = first_layout_match(&vnode, &layout, "scroll-box");
    let wrap = first_layout_match(&vnode, &layout, "wrap-row");
    let mut sections = Vec::new();
    collect_layout_matches(&vnode, &layout, "section", &mut sections);
    assert!(
        sections.len() >= 7,
        "all seven showcase sections must exist"
    );
    let centered_section = sections
        .iter()
        .find(|candidate| {
            candidate.rect.x <= centered.rect.x
                && candidate.rect.x + candidate.rect.w >= centered.rect.x + centered.rect.w
        })
        .expect("centered section should be present");
    let scroll_section = sections
        .iter()
        .find(|candidate| {
            candidate.rect.y <= scroll.rect.y
                && candidate.rect.y + candidate.rect.h >= scroll.rect.y
        })
        .expect("scroll section should be present");
    let wrap_section = sections
        .iter()
        .find(|candidate| {
            candidate.rect.y <= wrap.rect.y && candidate.rect.y + candidate.rect.h >= wrap.rect.y
        })
        .expect("wrap section should be present");
    assert!(
        scroll_section.rect.y > centered_section.rect.y,
        "scroll section should be placed after the centered section"
    );
    assert!(
        wrap_section.rect.y > scroll_section.rect.y,
        "wrap section should be placed after the scroll section"
    );
    assert!(
        scroll_section.rect.y + scroll_section.rect.h > SMALL.1,
        "scroll section should extend beyond the 480x360 viewport"
    );

    write_proof("showcase", SMALL.0, SMALL.1, &png);

    // The first-viewport sections are rendered, centered by auto margins, and
    // do not overflow the viewport; later sections continue below it.
    let width = SMALL.0 as usize;
    let centered = pixels_near(&rgba, CENTERED_BG, 12);
    assert!(centered > 200, "centered block missing at 480x360");
    let rows_with_centered = (0..SMALL.1 as usize)
        .filter(|y| {
            let start = y * width * 4;
            pixels_near(&rgba[start..start + width * 4], CENTERED_BG, 12) > 0
        })
        .count();
    assert!(rows_with_centered > 10, "centered block is squashed");
    assert!(
        rows_with_centered < SMALL.1 as usize,
        "centered block overflows the viewport"
    );
}
