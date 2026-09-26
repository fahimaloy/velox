//! Headless render proof for the counter example.
//!
//! `render_vnode_to_raster_png_with_scale` rasterizes the compiled `App.vx` tree on the
//! CPU (no window, no compositor), so this test is the repeatable way to prove
//! that the example paints real content at real viewport sizes. Proof PNGs are
//! written to `target/velox-render-proof/counter-<W>x<H>.png`.

use std::sync::Arc;

use velox_dom::layout::{LayoutNode, Rect, compute_layout};
use velox_renderer::{render_vnode_to_raster_png_with_scale, render_vnode_to_rgba};
use velox_style::{Stylesheet, apply_with_cascade};

include!(concat!(env!("OUT_DIR"), "/app.rs"));

/// Colors declared in `src/App.vx`, used as pixel probes.
const COUNT_FG: [u8; 3] = [56, 189, 248]; // .count
const INC_BG: [u8; 3] = [74, 222, 128]; // .btn.inc
const DEC_BG: [u8; 3] = [251, 191, 36]; // .btn.dec
const RESET_BG: [u8; 3] = [148, 163, 184]; // .btn.reset
const CARD_BG: [u8; 3] = [30, 41, 59]; // .card

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

fn styled_tree(state: &Arc<app::script_rs::State>) -> velox_dom::VNode {
    let raw = app::render_with_state(Arc::clone(state), app::make_resolve(Arc::clone(state)));
    apply_with_cascade(&raw, &Stylesheet::parse(app::STYLE))
}

fn find_layout_rect(
    layout: &LayoutNode,
    vnode: &velox_dom::VNode,
    tag: &str,
    class: &str,
) -> Option<Rect> {
    if let velox_dom::VNode::Element {
        tag: node_tag,
        props,
        ..
    } = vnode
        && node_tag == tag
        && props.attrs.get("class").map(String::as_str) == Some(class)
    {
        return Some(layout.rect);
    }
    if let velox_dom::VNode::Element { children, .. } = vnode {
        for child_layout in &layout.children {
            let Some(source_index) = child_layout.source_index else {
                continue;
            };
            if let Some(child) = children.get(source_index)
                && let Some(rect) = find_layout_rect(child_layout, child, tag, class)
            {
                return Some(rect);
            }
        }
    }
    None
}

/// The `value` and `on:input` props of the first `<input>` carrying `class`,
/// read out of a real build of the generated module.
fn input_props(vnode: &velox_dom::VNode, class: &str) -> Option<(String, String)> {
    if let velox_dom::VNode::Element { tag, props, .. } = vnode
        && tag == "input"
        && props.attrs.get("class").map(String::as_str) == Some(class)
    {
        let value = props
            .attrs
            .get("value")
            .cloned()
            .expect("the input carries a value prop");
        let handler = props
            .attrs
            .get("on:input")
            .cloned()
            .expect("the input carries an on:input handler name");
        return Some((value, handler));
    }
    if let velox_dom::VNode::Element { children, .. } = vnode {
        for child in children {
            if let Some(found) = input_props(child, class) {
                return Some(found);
            }
        }
    }
    None
}

fn pixels_near_in_rect(
    rgba: &[u8],
    rect: Rect,
    width: usize,
    height: usize,
    color: [u8; 3],
    tolerance: i32,
) -> usize {
    let left = rect.x.max(0) as usize;
    let top = rect.y.max(0) as usize;
    let right = ((rect.x + rect.w).max(0) as usize).min(width);
    let bottom = ((rect.y + rect.h).max(0) as usize).min(height);
    (top..bottom)
        .flat_map(|y| (left..right).map(move |x| (x, y)))
        .filter(|(x, y)| {
            let offset = (*y * width + *x) * 4;
            (rgba[offset] as i32 - color[0] as i32).abs() <= tolerance
                && (rgba[offset + 1] as i32 - color[1] as i32).abs() <= tolerance
                && (rgba[offset + 2] as i32 - color[2] as i32).abs() <= tolerance
        })
        .count()
}

fn write_proof(name: &str, width: i32, height: i32, png: &[u8]) -> std::path::PathBuf {
    let path = proof_dir().join(format!("{name}-{width}x{height}.png"));
    std::fs::write(&path, png).expect("write proof png");
    path
}

/// Number of pixels close to `color` (tolerates antialiasing edges).
fn pixels_near(rgba: &[u8], color: [u8; 3], tolerance: i32) -> usize {
    rgba.chunks_exact(4)
        .filter(|px| {
            (px[0] as i32 - color[0] as i32).abs() <= tolerance
                && (px[1] as i32 - color[1] as i32).abs() <= tolerance
                && (px[2] as i32 - color[2] as i32).abs() <= tolerance
        })
        .count()
}

fn distinct_colors(rgba: &[u8]) -> usize {
    let mut seen = std::collections::HashSet::new();
    for px in rgba.chunks_exact(4) {
        seen.insert((px[0] / 16, px[1] / 16, px[2] / 16));
    }
    seen.len()
}

/// Scanline index with the most pixels close to `color`, and that pixel count.
fn busiest_row(
    rgba: &[u8],
    width: usize,
    height: usize,
    color: [u8; 3],
    tolerance: i32,
) -> (usize, usize) {
    let mut best = (0usize, 0usize);
    for y in 0..height {
        let start = y * width * 4;
        let count = pixels_near(&rgba[start..start + width * 4], color, tolerance);
        if count > best.1 {
            best = (y, count);
        }
    }
    best
}

/// Last scanline that contains a pixel close to `color`.
fn last_row_with_color(
    rgba: &[u8],
    width: usize,
    height: usize,
    color: [u8; 3],
    tolerance: i32,
) -> usize {
    (0..height)
        .rev()
        .find(|y| {
            let start = y * width * 4;
            pixels_near(&rgba[start..start + width * 4], color, tolerance) > 0
        })
        .expect("color never painted")
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

#[test]
fn renders_large_viewport_proof_png() {
    let state = Arc::new(app::script_rs::State::new());
    let (png, rgba) = render(&state, LARGE.0, LARGE.1);
    assert_non_trivial("counter-large", &png, &rgba);
    // Card, count glyphs and the three buttons are all painted.
    assert!(pixels_near(&rgba, CARD_BG, 8) > 1000, "card background");
    assert!(pixels_near(&rgba, COUNT_FG, 40) > 200, "count text");
    assert!(pixels_near(&rgba, INC_BG, 12) > 200, "+1 button");
    assert!(pixels_near(&rgba, DEC_BG, 12) > 200, "-1 button");
    assert!(pixels_near(&rgba, RESET_BG, 12) > 200, "reset button");
    let path = write_proof("counter", LARGE.0, LARGE.1, &png);
    assert!(path.exists(), "proof png missing: {}", path.display());
}

#[test]
fn small_viewport_keeps_count_status_and_all_buttons_visible() {
    let state = Arc::new(app::script_rs::State::new());
    let (png, rgba) = render(&state, SMALL.0, SMALL.1);
    assert_non_trivial("counter-small", &png, &rgba);
    write_proof("counter", SMALL.0, SMALL.1, &png);

    let width = SMALL.0 as usize;
    let height = SMALL.1 as usize;
    assert!(
        pixels_near(&rgba, COUNT_FG, 40) > 200,
        "count text missing at 480x360"
    );

    // The status line is derived from the count, so incrementing must change
    // the rendered pixels ("not positive" -> "positive"). Check the status
    // element region independently of the count glyph.
    let before_tree = styled_tree(&state);
    let before_layout = compute_layout(&before_tree, SMALL.0, SMALL.1);
    let status_rect =
        find_layout_rect(&before_layout, &before_tree, "p", "status").expect("status layout");
    let before_status_pixels =
        pixels_near_in_rect(&rgba, status_rect, width, height, [148, 163, 184], 24);
    assert!(
        before_status_pixels > 5,
        "status text missing before increment"
    );

    state.increment();
    let (_, after) = render(&state, SMALL.0, SMALL.1);
    assert_ne!(rgba, after, "status label did not react to increment()");
    let after_tree = styled_tree(&state);
    let after_layout = compute_layout(&after_tree, SMALL.0, SMALL.1);
    let after_status_rect = find_layout_rect(&after_layout, &after_tree, "p", "status")
        .expect("status layout after increment");
    let after_status_pixels = pixels_near_in_rect(
        &after,
        after_status_rect,
        width,
        height,
        [148, 163, 184],
        24,
    );
    assert!(
        after_status_pixels > 5,
        "status text missing after increment"
    );

    // All three buttons survive the small viewport in one flex row.
    let mut button_rows = Vec::new();
    for (label, color) in [("+1", INC_BG), ("-1", DEC_BG), ("reset", RESET_BG)] {
        let (row, pixels) = busiest_row(&rgba, width, height, color, 12);
        assert!(pixels > 50, "{label} button missing at 480x360");
        button_rows.push(row);
    }
    assert!(
        button_rows
            .windows(2)
            .all(|rows| rows[0].abs_diff(rows[1]) <= 4),
        "flex action buttons are not on one row: {button_rows:?}"
    );

    // The row is not clipped by the bottom edge.
    let last_reset_row = last_row_with_color(&rgba, width, height, RESET_BG, 12);
    assert!(
        last_reset_row + 4 < height,
        "reset button clipped at the bottom of 480x360 (row {last_reset_row})"
    );
}

/// The `v-model` round trip, end to end, on the OBSERVED values.
///
/// Nothing here reads generated text. The handler name comes out of the
/// `on:input` prop, exactly as `dispatch_input_to_focused` resolves it; the
/// payload is built the way the renderer builds it (the displayed value plus
/// the typed character), because that is what the real input path sends; the
/// dispatch is the module's own `make_on_event`; and the two things asserted
/// afterwards are the value the `State` holds and the pixels the input paints.
/// So this test fails if the generated setter is missing, misnamed, or if the
/// read renders empty.
#[test]
fn v_model_input_round_trips_through_the_real_dispatcher() {
    let state = Arc::new(app::script_rs::State::new());
    let build = |s: &Arc<app::script_rs::State>| {
        app::render_with_state(Arc::clone(s), app::make_resolve(Arc::clone(s)))
    };

    // READ, before anything is typed: the `value` prop is what the input
    // shows, and it is the signal's value, not the empty-string fallback.
    let (value, handler) =
        input_props(&build(&state), "label-input").expect("the label input rendered");
    assert_eq!(
        value,
        state.label(),
        "the input must display the value the state holds"
    );
    assert!(
        !value.is_empty(),
        "the example's initial value is not empty"
    );

    // The layout rect, so the pixel assertions below look only at this input.
    let tree = styled_tree(&state);
    let layout = compute_layout(&tree, LARGE.0, LARGE.1);
    let rect = find_layout_rect(&layout, &tree, "input", "label-input")
        .expect("the label input has a layout rect");
    assert!(
        rect.w > 40 && rect.h > 10,
        "the input is too small: {rect:?}"
    );

    let width = LARGE.0 as usize;
    let height = LARGE.1 as usize;
    // The renderer paints an input's value as black text on white, so the
    // count of near-black pixels inside its rect is the text it shows.
    const TEXT: [u8; 3] = [0, 0, 0];
    let text_pixels = |s: &Arc<app::script_rs::State>| {
        let (_, rgba) = render(s, LARGE.0, LARGE.1);
        pixels_near_in_rect(&rgba, rect, width, height, TEXT, 40)
    };

    let before_pixels = text_pixels(&state);
    assert!(
        before_pixels > 0,
        "the input's own text is not painted, so the pixel proof below would be vacuous"
    );

    // WRITE, twice, the way the renderer sends it: the displayed value plus the
    // typed character, to the handler name the input carries. The second
    // keystroke re-reads the name from a fresh build, so a rebuild that dropped
    // or renamed the handler would show up here too.
    let (shown, _) = input_props(&build(&state), "label-input").expect("the label input rendered");
    app::make_on_event(Arc::clone(&state))(&handler, Some(&format!("{shown}R")));
    assert_eq!(
        state.label(),
        "counterR",
        "the generated setter must land 'R' in the state"
    );
    let (shown, name) =
        input_props(&build(&state), "label-input").expect("the label input rendered");
    app::make_on_event(Arc::clone(&state))(&name, Some(&format!("{shown}2")));
    assert_eq!(
        state.label(),
        "counterR2",
        "the generated setter must land '2' in the state"
    );

    // READ BACK, through a real re-render: the new build shows the new value.
    let (after, _) = input_props(&build(&state), "label-input").expect("the label input rendered");
    assert_eq!(after, "counterR2", "a new build shows the written value");

    // And the input really repaints the longer value: more black text pixels.
    let after_pixels = text_pixels(&state);
    assert!(
        after_pixels > before_pixels,
        "typing did not repaint the input: {before_pixels} -> {after_pixels} black pixels"
    );
}
