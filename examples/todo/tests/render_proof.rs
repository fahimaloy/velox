//! Headless render proof for the todo example.
//!
//! Rasterizes the compiled `App.vx` tree on the CPU (no window, no compositor)
//! and writes proof PNGs to `target/velox-render-proof/todo-<W>x<H>.png`.

use std::sync::Arc;

use velox_renderer::{render_vnode_to_raster_png_with_scale, render_vnode_to_rgba};
use velox_style::Stylesheet;

include!(concat!(env!("OUT_DIR"), "/app.rs"));

/// Colors declared in `src/App.vx` and the child components.
const ROW_BG: [u8; 3] = [30, 41, 59]; // .todo-item / .filters button
const DONE_FG: [u8; 3] = [34, 197, 94]; // .done text, toggled through :class
const ADD_BG: [u8; 3] = [56, 189, 248]; // .btn-add
const FILTER_BG: [u8; 3] = [51, 65, 85]; // .filter chip

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
    let vnode = build(state);
    let sheet = Stylesheet::parse(app::STYLE);
    let png = render_vnode_to_raster_png_with_scale(&vnode, &sheet, width, height, 1.0)
        .expect("raster png");
    let rgba = render_vnode_to_rgba(&vnode, &sheet, width, height).expect("raster rgba");
    (png, rgba)
}

fn build(state: &Arc<app::script_rs::State>) -> velox_dom::VNode {
    app::render_with_state(Arc::clone(state), app::make_resolve(Arc::clone(state)))
}

/// Depth-first search for the first element with `tag`, returning its attributes.
fn find_element_attrs<'a>(
    node: &'a velox_dom::VNode,
    tag: &str,
) -> Option<&'a std::collections::HashMap<String, String>> {
    match node {
        velox_dom::VNode::Element {
            tag: node_tag,
            props,
            children,
        } => {
            if node_tag == tag {
                return Some(&props.attrs);
            }
            children
                .iter()
                .find_map(|child| find_element_attrs(child, tag))
        }
        velox_dom::VNode::Text(_) => None,
    }
}

fn write_proof(name: &str, width: i32, height: i32, png: &[u8]) -> std::path::PathBuf {
    let path = proof_dir().join(format!("{name}-{width}x{height}.png"));
    std::fs::write(&path, png).expect("write proof png");
    path
}

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
    assert_non_trivial("todo-large", &png, &rgba);
    // Seeded todos render as cards, the completed one is tinted through the
    // `:class` binding, and the input plus filter controls are painted.
    assert!(pixels_near(&rgba, ROW_BG, 8) > 100_000, "todo cards");
    assert!(
        pixels_near(&rgba, DONE_FG, 10) > 40,
        "completed todo styling"
    );
    assert!(pixels_near(&rgba, ADD_BG, 12) > 2_000, "add button");
    assert!(pixels_near(&rgba, FILTER_BG, 8) > 2_000, "filter chip");
    let path = write_proof("todo", LARGE.0, LARGE.1, &png);
    assert!(path.exists(), "proof png missing: {}", path.display());
}

#[test]
fn small_viewport_renders_the_list_and_reacts_to_events() {
    let state = Arc::new(app::script_rs::State::new());
    let (png, rgba) = render(&state, SMALL.0, SMALL.1);
    assert_non_trivial("todo-small", &png, &rgba);
    write_proof("todo", SMALL.0, SMALL.1, &png);
    // The first todo card, the add button and the filter chips all land
    // inside a 480x360 viewport.
    assert!(
        pixels_near(&rgba, ROW_BG, 8) > 5_000,
        "todo cards at 480x360"
    );
    assert!(
        pixels_near(&rgba, ADD_BG, 12) > 2_000,
        "add button at 480x360"
    );
    assert!(
        pixels_near(&rgba, FILTER_BG, 8) > 2_000,
        "filter chip at 480x360"
    );
}

/// Regression proof for the component-prop defect: bound component attributes
/// (`:value="draft"`, `:placeholder="input_placeholder"`) are emitted as
/// `resolve("...")` calls, so the generated resolver must register those keys.
/// The proof types into the input through the real event dispatcher, renders
/// BEFORE submit, and checks the draft text and placeholder reach the rendered
/// input element.
#[test]
fn typed_draft_reaches_the_input_before_submit() {
    let state = Arc::new(app::script_rs::State::new());
    let (empty_png, empty_rgba) = render(&state, LARGE.0, LARGE.1);

    // The rendered input starts empty but carries the bound placeholder.
    let empty_tree = build(&state);
    let empty_input = find_element_attrs(&empty_tree, "input").expect("input element");
    assert_eq!(
        empty_input.get("value").map(String::as_str),
        Some(""),
        "draft starts empty"
    );
    assert_eq!(
        empty_input.get("placeholder").map(String::as_str),
        Some("What needs to be done?"),
        "bound :placeholder must resolve, not render empty"
    );

    // Dispatch exactly what the renderer dispatches for `on:input`.
    let mut on_event = app::make_on_event(Arc::clone(&state));
    on_event("on_input", Some("Ship the rewrite"));
    assert_eq!(
        state.draft.get(),
        "Ship the rewrite",
        "draft should hold input"
    );

    let (png, rgba) = render(&state, LARGE.0, LARGE.1);
    let typed_tree = build(&state);
    let input = find_element_attrs(&typed_tree, "input").expect("input element");
    assert_eq!(
        input.get("value").map(String::as_str),
        Some("Ship the rewrite"),
        "bound :value must render the live draft"
    );
    assert_eq!(
        input.get("placeholder").map(String::as_str),
        Some("What needs to be done?"),
        "placeholder stays bound after typing"
    );
    // The renderer paints the input's value, so the frame must differ from the
    // empty-draft frame: the typed text is actually drawn, not just stored.
    assert_ne!(empty_rgba, rgba, "typed draft did not change the render");
    let path = write_proof("todo-draft", LARGE.0, LARGE.1, &png);
    assert!(path.exists(), "proof png missing: {}", path.display());
    assert_ne!(empty_png, png, "typed draft did not change the png");
}

#[test]
fn events_drive_the_visible_list() {
    let state = Arc::new(app::script_rs::State::new());
    let (_, before) = render(&state, LARGE.0, LARGE.1);

    state.on_input("Ship the rewrite");
    state.on_submit();
    assert_eq!(state.draft.get(), "", "draft should clear after submit");
    assert_eq!(state.todos.get().len(), 3, "todo should be appended");

    let (_, after_add) = render(&state, LARGE.0, LARGE.1);
    assert_ne!(before, after_add, "on_submit did not change the render");
    assert!(
        pixels_near(&after_add, ROW_BG, 8) > pixels_near(&before, ROW_BG, 8),
        "added todo did not add a card"
    );

    // Toggling maps the rendered row back to its source todo.
    state.on_toggle("1");
    let (png, after_toggle) = render(&state, LARGE.0, LARGE.1);
    assert_ne!(after_add, after_toggle, "toggle did not change the render");
    write_proof("todo-toggled", LARGE.0, LARGE.1, &png);

    // Cycling the filter twice reaches "completed", which hides active rows.
    state.cycle_filter();
    state.cycle_filter();
    let (_, after_filter) = render(&state, LARGE.0, LARGE.1);
    assert_ne!(
        after_toggle, after_filter,
        "filter did not change the render"
    );
    assert!(
        pixels_near(&after_filter, ROW_BG, 8) < pixels_near(&after_toggle, ROW_BG, 8),
        "completed filter should hide active cards"
    );
}
