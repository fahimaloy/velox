//! Headless render proof for the todo example.
//!
//! Rasterizes the compiled `App.vx` tree on the CPU (no window, no compositor)
//! and writes proof PNGs to `target/velox-render-proof/todo-<W>x<H>.png`.
use std::sync::Arc;

use velox_dom::layout::{LayoutNode, Rect, compute_layout};
use velox_renderer::{render_vnode_to_raster_png_with_scale, render_vnode_to_rgba};
use velox_style::{Stylesheet, apply_with_cascade};

include!(concat!(env!("OUT_DIR"), "/app.rs"));

/// Colors declared in `src/App.vx` and the child components.
const ROW_BG: [u8; 3] = [30, 41, 59]; // .todo-item
const DONE_FG: [u8; 3] = [34, 197, 94]; // .done text, toggled through :class
const ADD_BG: [u8; 3] = [56, 189, 248]; // .add
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

fn styled_build(state: &Arc<app::script_rs::State>) -> velox_dom::VNode {
    apply_with_cascade(&build(state), &Stylesheet::parse(app::STYLE))
}

fn find_layout_rect<'a>(
    layout: &'a LayoutNode,
    vnode: &velox_dom::VNode,
    tag: &str,
) -> Option<Rect> {
    if let velox_dom::VNode::Element { tag: node_tag, .. } = vnode
        && node_tag == tag
    {
        return Some(layout.rect);
    }
    if let velox_dom::VNode::Element { children, .. } = vnode {
        for child_layout in &layout.children {
            let Some(source_index) = child_layout.source_index else {
                continue;
            };
            if let Some(child) = children.get(source_index)
                && let Some(rect) = find_layout_rect(child_layout, child, tag)
            {
                return Some(rect);
            }
        }
    }
    None
}

fn input_rect(state: &Arc<app::script_rs::State>, width: i32, height: i32) -> Rect {
    let vnode = styled_build(state);
    let layout = compute_layout(&vnode, width, height);
    find_layout_rect(&layout, &vnode, "input").expect("input layout")
}

fn dark_pixels_in(rgba: &[u8], rect: Rect, width: i32, height: i32) -> usize {
    let left = rect.x.saturating_add(4).clamp(0, width) as usize;
    let top = rect.y.saturating_add(4).clamp(0, height) as usize;
    let right = rect
        .x
        .saturating_add(rect.w)
        .saturating_sub(4)
        .clamp(0, width) as usize;
    let bottom = rect
        .y
        .saturating_add(rect.h)
        .saturating_sub(4)
        .clamp(0, height) as usize;
    (top..bottom)
        .flat_map(|y| (left..right).map(move |x| (x, y)))
        .filter(|(x, y)| {
            let offset = (*y * width as usize + *x) * 4;
            rgba[offset] < 120 && rgba[offset + 1] < 120 && rgba[offset + 2] < 120
        })
        .count()
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

fn vnode_contains_text(node: &velox_dom::VNode, needle: &str) -> bool {
    match node {
        velox_dom::VNode::Text(text) => text.contains(needle),
        velox_dom::VNode::Element { children, .. } => children
            .iter()
            .any(|child| vnode_contains_text(child, needle)),
    }
}

/// The `key` attribute of every rendered todo row, in document order. A row
/// without one is reported as `<missing>` so a failure names the gap instead of
/// silently comparing an empty string.
fn todo_row_keys(node: &velox_dom::VNode, out: &mut Vec<String>) {
    if let velox_dom::VNode::Element {
        tag,
        props,
        children,
        ..
    } = node
    {
        if props.attrs.get("class").map(String::as_str) == Some("todo-item") {
            out.push(
                props
                    .attrs
                    .get("key")
                    .cloned()
                    .unwrap_or_else(|| String::from("<missing>")),
            );
        }
        for child in children {
            todo_row_keys(child, out);
        }
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
    assert!(pixels_near(&rgba, ADD_BG, 12) > 500, "add button");
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
        pixels_near(&rgba, ADD_BG, 12) > 500,
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
/// The proof types into the input through the generated event dispatcher,
/// renders before the Add click, and checks that the draft is painted.
#[test]
fn typed_draft_reaches_the_input_before_add() {
    let state = Arc::new(app::script_rs::State::new());
    let (empty_png, empty_rgba) = render(&state, LARGE.0, LARGE.1);
    let empty_rect = input_rect(&state, LARGE.0, LARGE.1);
    let empty_dark_pixels = dark_pixels_in(&empty_rgba, empty_rect, LARGE.0, LARGE.1);

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
    assert_eq!(state.draft(), "Ship the rewrite", "draft should hold input");

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
    // The renderer paints the input's value. Count dark text pixels inside the
    // laid-out input so a state-only change cannot pass this proof.
    let typed_rect = input_rect(&state, LARGE.0, LARGE.1);
    let typed_dark_pixels = dark_pixels_in(&rgba, typed_rect, LARGE.0, LARGE.1);
    assert!(
        typed_dark_pixels > empty_dark_pixels + 5,
        "typed draft must add dark text pixels inside the input ({typed_dark_pixels} vs {empty_dark_pixels})"
    );
    assert_ne!(empty_rgba, rgba, "typed draft did not change the render");
    let path = write_proof("todo-draft", LARGE.0, LARGE.1, &png);
    assert!(path.exists(), "proof png missing: {}", path.display());
    assert_ne!(empty_png, png, "typed draft did not change the png");

    // Add is a real click handler, not a renderer submit event source.
    on_event("add_todo", None);
    assert_eq!(state.draft(), "", "draft should clear after Add");
    assert_eq!(
        state.todoitem.todos.get().len(),
        3,
        "todo should be appended"
    );
}

#[test]
fn events_drive_the_visible_list() {
    let state = Arc::new(app::script_rs::State::new());
    let mut on_event = app::make_on_event(Arc::clone(&state));
    let (_, before) = render(&state, LARGE.0, LARGE.1);

    on_event("on_input", Some("Ship the rewrite"));
    let (_, typed) = render(&state, LARGE.0, LARGE.1);
    assert_ne!(before, typed, "input event did not change the render");

    on_event("add_todo", None);
    assert_eq!(state.draft(), "", "draft should clear after Add");
    assert_eq!(
        state.todoitem.todos.get().len(),
        3,
        "todo should be appended"
    );

    let (_, after_add) = render(&state, LARGE.0, LARGE.1);
    assert_ne!(typed, after_add, "Add click did not change the render");
    assert!(
        pixels_near(&after_add, ROW_BG, 8) > pixels_near(&before, ROW_BG, 8),
        "added todo did not add a card"
    );

    // Toggling maps the rendered row back to its source todo.
    on_event("on_toggle", Some("1"));
    let (png, after_toggle) = render(&state, LARGE.0, LARGE.1);
    assert_ne!(after_add, after_toggle, "toggle did not change the render");
    write_proof("todo-toggled", LARGE.0, LARGE.1, &png);

    // The generated dispatcher must remove the selected row, not merely call a
    // handler. The typed item is the second item in the underlying list and
    // is visible under the current "all" filter, so removing index 2 has a
    // directly observable VNode and pixel result.
    let before_remove_tree = build(&state);
    assert!(
        vnode_contains_text(&before_remove_tree, "Ship the rewrite"),
        "the item selected for removal should be visible before remove"
    );
    let before_remove_rows = pixels_near(&after_toggle, ROW_BG, 8);
    on_event("on_remove", Some("2"));
    assert_eq!(
        state.todoitem.todos.get().len(),
        2,
        "remove should drop the selected todo"
    );
    let (_, after_remove) = render(&state, LARGE.0, LARGE.1);
    let after_remove_tree = build(&state);
    assert!(
        !vnode_contains_text(&after_remove_tree, "Ship the rewrite"),
        "removed todo text should disappear from the rendered tree"
    );
    assert!(
        vnode_contains_text(&after_remove_tree, "Learn Velox"),
        "the remaining todo should still render"
    );
    assert_ne!(
        after_toggle, after_remove,
        "remove did not change the render"
    );
    assert!(
        pixels_near(&after_remove, ROW_BG, 8) < before_remove_rows,
        "removed todo should reduce the rendered row area"
    );

    // Cycling the filter twice reaches "completed", which hides the remaining
    // active rows. This keeps the filter proof after the remove proof so both
    // generated dispatch paths are exercised in a stable order.
    on_event("cycle_filter", None);
    on_event("cycle_filter", None);
    let (_, after_filter) = render(&state, LARGE.0, LARGE.1);
    assert_ne!(
        after_remove, after_filter,
        "filter did not change the render"
    );
    assert!(
        pixels_near(&after_filter, ROW_BG, 8) < pixels_near(&after_remove, ROW_BG, 8),
        "completed filter should hide active cards"
    );
}

/// Proof for the `:key` directive itself, which the pixel proofs above cannot
/// see: the list `v-for` in `App.vx` carries `:key="todo.id"`, so every
/// rendered row must carry that key in `props.attrs["key"]`.
///
/// This fails if key insertion is removed from codegen (every row would read
/// `<missing>`), and because the keys are read back after the store changes,
/// it fails if the key were positional rather than taken from the todo.
#[test]
fn every_todo_row_carries_its_key() {
    let state = Arc::new(app::script_rs::State::new());

    let mut keys = Vec::new();
    todo_row_keys(&build(&state), &mut keys);
    assert_eq!(
        keys,
        vec![String::from("0"), String::from("1")],
        "each rendered row must carry the `key` of its todo"
    );

    let mut on_event = app::make_on_event(Arc::clone(&state));
    on_event("on_input", Some("Ship the rewrite"));
    on_event("add_todo", None);

    let mut with_new = Vec::new();
    todo_row_keys(&build(&state), &mut with_new);
    assert_eq!(
        with_new,
        vec![String::from("0"), String::from("1"), String::from("2")],
        "an appended todo must bring its own key"
    );

    // Remove the FIRST row. The survivors keep their own ids (1 and 2), so a
    // positional key (0 and 1) would fail this assertion.
    on_event("on_remove", Some("0"));
    let mut after_remove = Vec::new();
    todo_row_keys(&build(&state), &mut after_remove);
    assert_eq!(
        after_remove,
        vec![String::from("1"), String::from("2")],
        "keys must follow the todo, not its position in the list"
    );

    // Toggling changes `completed`, not the id, so the keys are unchanged.
    on_event("on_toggle", Some("1"));
    let mut after_toggle = Vec::new();
    todo_row_keys(&build(&state), &mut after_toggle);
    assert_eq!(
        after_toggle,
        vec![String::from("1"), String::from("2")],
        "toggling a todo must not change its key"
    );
}
