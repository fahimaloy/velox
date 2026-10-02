//! Headless render proof for the todo example.
//!
//! Rasterizes the compiled `App.vx` tree on the CPU (no window, no compositor)
//! and writes proof PNGs to `target/velox-render-proof/todo-<W>x<H>.png`.

// `app::script_rs::State` holds `velox_core::Ref<T>` — an `Rc<Signal<T>>` — so it
// is `!Send` and `!Sync` by construction, matching velox's one-thread-per-window
// winit model. The `Arc` below is therefore not a cross-thread choice: it is the
// parameter type velox-sfc codegen emits for `render_with_state` / `make_resolve`
// / `make_on_event` (velox-sfc/src/template_codegen.rs), and `Arc::clone` is used
// only to hand the same state to several of those functions. The lint's own
// remedies — make `State` `Send + Sync`, or emit `Rc<State>` — are a codegen-wide
// API change rather than a local fix, so the lint is allowed at file level here
// instead of silenced call-by-call. Drop this allow if velox ever drives one
// window's state from more than one thread, or if codegen is changed to emit
// `Rc<State>`; in that case the fix is to change the code, not to keep the allow.
#![allow(clippy::arc_with_non_send_sync)]

use std::cmp::Ordering;
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

/// Per-channel slack when deciding whether a pixel counts as ink. Wide enough to
/// absorb the antialiased fringe of a glyph, far narrower than the gap between
/// the theme's background and its text.
const INK_TOLERANCE: i32 = 12;

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

fn find_layout_rect(layout: &LayoutNode, vnode: &velox_dom::VNode, tag: &str) -> Option<Rect> {
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

/// The pixel coordinates strictly inside `rect`.
///
/// Inset by 4px so the element's own border is never counted as content.
fn interior_pixels(rect: Rect, width: i32, height: i32) -> impl Iterator<Item = (usize, usize)> {
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
    (top..bottom).flat_map(move |y| (left..right).map(move |x| (x, y)))
}

fn dark_pixels_in(rgba: &[u8], rect: Rect, width: i32, height: i32) -> usize {
    interior_pixels(rect, width, height)
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
        props, children, ..
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

/// Read one declaration out of a cascaded `style` attribute (`"a: b; c: d"`).
fn style_decl<'s>(style: &'s str, key: &str) -> Option<&'s str> {
    style
        .split(';')
        .filter_map(|d| d.split_once(':'))
        .find(|(k, _)| k.trim() == key)
        .map(|(_, v)| v.trim())
}

/// Parse `#rgb` / `#rrggbb` / `rgb(r, g, b)` into channels.
///
/// Deliberately narrow: this resolves the theme's own declarations, and a value
/// it cannot parse is reported as `None` so a caller can fail loudly instead of
/// silently measuring against a made-up colour.
fn parse_css_color(value: &str) -> Option<[u8; 3]> {
    let value = value.trim();
    if let Some(hex) = value.strip_prefix('#') {
        let nibble = |c: u8| char::from(c).to_digit(16).map(|d| d as u8);
        return match hex.len() {
            3 => {
                let mut out = [0u8; 3];
                for (i, slot) in out.iter_mut().enumerate() {
                    let d = nibble(hex.as_bytes()[i])?;
                    *slot = d * 17;
                }
                Some(out)
            }
            6 => {
                let bytes = hex.as_bytes();
                let mut out = [0u8; 3];
                for (i, slot) in out.iter_mut().enumerate() {
                    let hi = nibble(bytes[i * 2])?;
                    let lo = nibble(bytes[i * 2 + 1])?;
                    *slot = hi * 16 + lo;
                }
                Some(out)
            }
            _ => None,
        };
    }
    if let Some(inner) = value
        .strip_prefix("rgb(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        let mut out = [0u8; 3];
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = inner.split(',').nth(i)?.trim().parse::<u8>().ok()?;
        }
        return Some(out);
    }
    None
}

/// An input's own resolved foreground/background, read from its cascaded style.
struct InputTheme {
    background: [u8; 3],
    foreground: [u8; 3],
}

/// The input's own resolved colours.
///
/// Read from the **cascaded** `style` the cascade wrote onto the element (UA +
/// author), never hardcoded: which end of the scale counts as "ink" is a
/// property of the theme, not of the text, so a probe that measures brightness
/// is measuring the theme instead of the ink.
fn input_theme(state: &Arc<app::script_rs::State>) -> InputTheme {
    let tree = styled_build(state);
    let attrs = find_element_attrs(&tree, "input").expect("input element");
    let style = attrs.get("style").map(String::as_str).unwrap_or("");
    let background = style_decl(style, "background-color")
        .or_else(|| style_decl(style, "background"))
        .and_then(parse_css_color)
        .unwrap_or_else(|| {
            panic!("the input must resolve a literal background colour, got: {style}")
        });
    let foreground = style_decl(style, "color")
        .and_then(parse_css_color)
        .unwrap_or_else(|| panic!("the input must resolve a literal text colour, got: {style}"));
    InputTheme {
        background,
        foreground,
    }
}

/// Pixels inside `rect` that **differ from the input's own background** — an
/// ink measure.
///
/// This replaces a brightness threshold, which cannot express what it is
/// supposed to: with the input honouring the author's `#1e293b` background, a
/// "every channel < 120" test counts the *background* as dark and excludes both
/// the typed value (`#e2e8f0`) and the derived placeholder — so typing made the
/// measured count go *down*, and the assertion inverted itself for any dark
/// theme. Counting pixels that differ from the element's own resolved background
/// has no such dependency on which end of the scale is bright.
fn ink_pixels_in(
    rgba: &[u8],
    rect: Rect,
    width: i32,
    height: i32,
    background: [u8; 3],
    tolerance: i32,
) -> usize {
    interior_pixels(rect, width, height)
        .filter(|(x, y)| {
            let offset = (*y * width as usize + *x) * 4;
            (rgba[offset] as i32 - background[0] as i32).abs() > tolerance
                || (rgba[offset + 1] as i32 - background[1] as i32).abs() > tolerance
                || (rgba[offset + 2] as i32 - background[2] as i32).abs() > tolerance
        })
        .count()
}

/// Same ink measure, restricted to the element's own text colour.
///
/// Used where the question is "is *this* text painted", as opposed to "is
/// something painted": the placeholder is ink too, but in a different colour,
/// and counting it would make the empty field look occupied.
fn text_ink_pixels_in(
    rgba: &[u8],
    rect: Rect,
    width: i32,
    height: i32,
    foreground: [u8; 3],
    tolerance: i32,
) -> usize {
    interior_pixels(rect, width, height)
        .filter(|(x, y)| {
            let offset = (*y * width as usize + *x) * 4;
            (rgba[offset] as i32 - foreground[0] as i32).abs() <= tolerance
                && (rgba[offset + 1] as i32 - foreground[1] as i32).abs() <= tolerance
                && (rgba[offset + 2] as i32 - foreground[2] as i32).abs() <= tolerance
        })
        .count()
}

/// A copy of the tree with the input's `placeholder` attribute removed.
///
/// The blank-field baseline for the ink proof. Without it the "empty" input
/// still paints its placeholder, so the baseline is not empty and the proof
/// would be comparing two different strings' worth of ink rather than
/// "something vs nothing".
fn without_input_placeholder(node: &velox_dom::VNode) -> velox_dom::VNode {
    match node {
        velox_dom::VNode::Text(text) => velox_dom::VNode::Text(text.clone()),
        velox_dom::VNode::Element {
            tag,
            props,
            children,
        } => {
            let mut props = props.clone();
            if tag == "input" {
                props.attrs.remove("placeholder");
            }
            velox_dom::VNode::Element {
                tag: tag.clone(),
                props,
                children: children.iter().map(without_input_placeholder).collect(),
            }
        }
    }
}

/// Render a prepared tree, so a proof can rasterize a variant of the app's own
/// output instead of only the output the app produces.
fn render_tree(vnode: &velox_dom::VNode, width: i32, height: i32) -> Vec<u8> {
    let sheet = Stylesheet::parse(app::STYLE);
    render_vnode_to_rgba(vnode, &sheet, width, height).expect("raster rgba")
}

/// A one-input page in an explicit theme, for the theme-independence proof.
///
/// Built here rather than reused from the app so the *only* variable between the
/// light and dark cases is which end of the scale is bright. Nothing else about
/// the two renders differs.
fn themed_input_vnode(background: &str, foreground: &str, value: &str) -> velox_dom::VNode {
    let style = format!(
        "position:absolute;left:10px;top:10px;width:320px;height:40px;\
         box-sizing:border-box;padding:6px 10px;font-size:14px;\
         border:1px solid {background};background:{background};color:{foreground}"
    );
    velox_dom::VNode::Element {
        tag: "div".into(),
        props: velox_dom::Props::new().set("style", "background:#808080;width:100%;height:100%"),
        children: vec![velox_dom::VNode::Element {
            tag: "input".into(),
            props: velox_dom::Props::new()
                .set("type", "text")
                .set("value", value)
                .set("style", style),
            children: vec![],
        }],
    }
}

/// The ink the field shows for `value` in the given theme, plus the brightness
/// count for the same render.
fn themed_input_ink(background: &str, foreground: &str, value: &str) -> (usize, usize) {
    let (w, h) = (400, 120);
    let vnode = themed_input_vnode(background, foreground, value);
    let rect = find_layout_rect(&compute_layout(&vnode, w, h), &vnode, "input").expect("input");
    let rgba = render_tree(&vnode, w, h);
    let bg = parse_css_color(background).expect("fixture background");
    (
        ink_pixels_in(&rgba, rect, w, h, bg, INK_TOLERANCE),
        dark_pixels_in(&rgba, rect, w, h),
    )
}

/// The ink measure must not know which end of the scale is bright.
///
/// Two renders that differ in nothing but the theme: light background with dark
/// text, dark background with light text. The same assertion — typing adds ink
/// inside the field — has to hold for both, and the brightness measure that
/// replaced it does not: on the light theme typing *raises* the dark count and
/// on the dark theme it *lowers* it, because the background itself is what
/// crosses the threshold. That inversion is the whole reason the probe counts
/// ink now.
#[test]
fn the_ink_measure_is_theme_independent() {
    const LIGHT: (&str, &str) = ("#ffffff", "#0f172a");
    const DARK: (&str, &str) = ("#1e293b", "#e2e8f0");
    const TYPED: &str = "Ship it";

    let mut dark_counts = Vec::new();
    for (name, (background, foreground)) in [("light", LIGHT), ("dark", DARK)] {
        let (blank_ink, blank_dark) = themed_input_ink(background, foreground, "");
        let (typed_ink, typed_dark) = themed_input_ink(background, foreground, TYPED);
        assert!(
            typed_ink > blank_ink + 5,
            "{name} theme: typing must add ink pixels inside the input \
             ({typed_ink} vs {blank_ink})"
        );
        dark_counts.push((name, blank_dark, typed_dark));
    }

    // Each theme gets its OWN assertion on a named direction, and the
    // direction is not left to a score.
    //
    // The previous version scored `Greater` as +1, `Less` as -1, `Equal` as 0
    // and asserted the sum was 0. That passed VACUOUSLY in exactly the case the
    // test exists to catch: if `dark_pixels_in` ever came to count only the
    // background — the failure described in the doc comment above — BOTH themes
    // would return `Equal`, both contributed 0, and the sum was still 0. The
    // tripwire could not fire. Naming the expected direction for each theme
    // makes `Equal` a failure instead of a silent zero.
    let (light, dark) = (dark_counts[0], dark_counts[1]);
    assert_eq!(
        light.2.cmp(&light.1),
        Ordering::Greater,
        "on the light theme typing must RAISE the dark-pixel count ({blank} -> \
         {typed}). If it did not, `dark_pixels_in` is no longer measuring what \
         this test's doc comment says it measures.",
        blank = light.1,
        typed = light.2,
    );
    assert_eq!(
        dark.2.cmp(&dark.1),
        Ordering::Less,
        "on the dark theme typing must LOWER the dark-pixel count ({blank} -> \
         {typed}). If it did not, `dark_pixels_in` is no longer measuring what \
         this test's doc comment says it measures.",
        blank = dark.1,
        typed = dark.2,
    );
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
    let theme = input_theme(&state);

    // The blank-field baseline: the same input with its placeholder attribute
    // removed. The empty input above still paints the placeholder, so comparing
    // against it would weigh two different strings' worth of ink instead of
    // "something vs nothing".
    let blank_tree = without_input_placeholder(&build(&state));
    let blank_rect = find_layout_rect(
        &compute_layout(&blank_tree, LARGE.0, LARGE.1),
        &blank_tree,
        "input",
    )
    .expect("input layout");
    let blank_rgba = render_tree(&blank_tree, LARGE.0, LARGE.1);
    let blank_ink_pixels = ink_pixels_in(
        &blank_rgba,
        blank_rect,
        LARGE.0,
        LARGE.1,
        theme.background,
        INK_TOLERANCE,
    );
    let blank_text_ink_pixels = text_ink_pixels_in(
        &blank_rgba,
        blank_rect,
        LARGE.0,
        LARGE.1,
        theme.foreground,
        INK_TOLERANCE,
    );

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
    // The renderer paints the input's value. Count the pixels inside the
    // laid-out input that differ from the input's OWN resolved background, so a
    // state-only change cannot pass this proof and the measure stays correct
    // when the theme flips from light to dark.
    let typed_rect = input_rect(&state, LARGE.0, LARGE.1);
    let typed_ink_pixels = ink_pixels_in(
        &rgba,
        typed_rect,
        LARGE.0,
        LARGE.1,
        theme.background,
        INK_TOLERANCE,
    );
    assert!(
        typed_ink_pixels > blank_ink_pixels + 5,
        "typed draft must add ink pixels inside the input ({typed_ink_pixels} vs {blank_ink_pixels} ink pixels against background {:?})",
        theme.background
    );
    // And those pixels must be the value's own text colour, not merely
    // something else in the field: the placeholder is ink too, and in a
    // different colour, so counting it would let an untyped field look typed.
    let typed_text_ink_pixels = text_ink_pixels_in(
        &rgba,
        typed_rect,
        LARGE.0,
        LARGE.1,
        theme.foreground,
        INK_TOLERANCE,
    );
    assert_eq!(
        blank_text_ink_pixels, 0,
        "the blank field must be blank in the value's text colour"
    );
    assert!(
        typed_text_ink_pixels > blank_text_ink_pixels + 5,
        "typed draft must add value-coloured pixels inside the input ({typed_text_ink_pixels} vs {blank_text_ink_pixels} at foreground {:?})",
        theme.foreground
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
