//! Renderer crate with optional backends.
//! No features enabled => stub, compiles fast.

use std::collections::{HashMap, HashSet};
use velox_dom::VNode;
use velox_style::Stylesheet;

/// Apply the renderer-wide UA < author < inline cascade before layout or paint.
///
/// The renderer always calls this once on a freshly-built VNode. Keeping the
/// composition here prevents a backend from accidentally dropping the UA layer.
pub fn style_vnode_with_hover<F>(vnode: &VNode, author: &Stylesheet, is_hovered: &F) -> VNode
where
    F: Fn(&str, &velox_dom::Props) -> bool,
{
    velox_style::apply_with_cascade_with_hover(vnode, author, is_hovered)
}

/// Lifecycle wiring helper — ensures `on_unmounted` / `before_destroy` hooks
/// fire even if the event loop exits via `Drop` rather than `CloseRequested`.
#[allow(dead_code)]
pub struct LifecycleCleanupGuard;

impl Drop for LifecycleCleanupGuard {
    fn drop(&mut self) {
        let _ = std::panic::catch_unwind(velox_core::lifecycle::run_all_destroy_hooks);
    }
}

#[allow(dead_code)]
pub fn ensure_mounted(flag: &mut bool) {
    if !*flag {
        *flag = true;
        velox_core::lifecycle::run_all_mounted_hooks();
    }
}

/// Return the VNode at a child source-index path, if it exists.
pub fn find_node_at_path<'a>(node: &'a VNode, path: &[usize]) -> Option<&'a VNode> {
    let mut cur = node;
    for &idx in path {
        match cur {
            VNode::Element { children, .. } => cur = children.get(idx)?,
            _ => return None,
        }
    }
    Some(cur)
}

/// Unified logical size helper — single rounding point for all frame paths.
/// Delegates to Viewport::from_i32 for consistent clamping; kept as free function for compat.
#[cfg(any(feature = "skia-native", test))]
fn viewport_logical_dimensions(width: i32, height: i32, scale_factor: f32) -> (u32, u32) {
    Viewport::from_i32(width, height, scale_factor).logical_size()
}

#[cfg(feature = "skia-native")]
pub fn logical_size(width: i32, height: i32, scale_factor: f32) -> (u32, u32) {
    viewport_logical_dimensions(width, height, scale_factor)
}

/// Notify resize hooks only after a coalesced physical resize has committed and
/// its logical viewport differs from the last committed size.
#[cfg(any(feature = "skia-native", test))]
fn dispatch_resize_if_changed(
    committed: bool,
    logical: (u32, u32),
    last_resize_size: &mut Option<(u32, u32)>,
) {
    if committed && *last_resize_size != Some(logical) {
        velox_core::lifecycle::run_resize_hooks(logical.0, logical.1);
        *last_resize_size = Some(logical);
    }
}

#[cfg(any(feature = "skia-native", test))]
fn frame_logical_size(
    width: i32,
    height: i32,
    scale_factor: f32,
    committed: bool,
    last_resize_size: &mut Option<(u32, u32)>,
) -> (u32, u32) {
    let logical = viewport_logical_dimensions(width, height, scale_factor);
    dispatch_resize_if_changed(committed, logical, last_resize_size);
    logical
}

/// Per-renderer resize state. Physical events are coalesced here and the
/// committed logical baseline is kept locally to this window/event loop.
#[cfg(any(feature = "skia-native", test))]
struct ResizeState {
    pending_resize: Option<(u32, u32)>,
    last_resize_size: Option<(u32, u32)>,
}

#[cfg(any(feature = "skia-native", test))]
impl ResizeState {
    fn new() -> Self {
        Self {
            pending_resize: None,
            last_resize_size: None,
        }
    }

    fn queue(&mut self, physical: (u32, u32)) {
        self.pending_resize = Some(physical);
    }

    fn take_pending(&mut self) -> Option<(u32, u32)> {
        self.pending_resize.take()
    }

    fn record_initial(&mut self, width: i32, height: i32, scale_factor: f32) -> (u32, u32) {
        let logical = viewport_logical_dimensions(width, height, scale_factor);
        self.last_resize_size = Some(logical);
        logical
    }

    fn frame_logical_size(
        &mut self,
        width: i32,
        height: i32,
        scale_factor: f32,
        committed: bool,
    ) -> (u32, u32) {
        frame_logical_size(
            width,
            height,
            scale_factor,
            committed,
            &mut self.last_resize_size,
        )
    }
}

/// Build hit-test targets from a precomputed layout. No layout recompute here.
#[cfg(feature = "skia-native")]
fn recompute_targets(
    vnode: &velox_dom::VNode,
    layout: &velox_dom::layout::LayoutNode,
    click_targets: &mut Vec<crate::events::ClickTarget>,
    hover_targets: &mut Vec<crate::events::HoverTarget>,
    input_targets: &mut Vec<crate::events::InputTarget>,
) {
    click_targets.clear();
    let mut order = 0;
    crate::events::collect_click_targets(
        vnode,
        layout,
        None,
        crate::events::StackCtx::ROOT,
        &mut order,
        click_targets,
    );
    hover_targets.clear();
    let mut order = 0;
    crate::events::collect_hover_targets(
        vnode,
        layout,
        None,
        crate::events::StackCtx::ROOT,
        &mut order,
        hover_targets,
    );
    // Edit state (focus / caret / selection / blink phase) lives on the targets,
    // and a target rebuild throws it all away. Move the old vector aside and
    // re-apply it onto the freshly collected targets by tree path, so a
    // re-render does not silently drop the caret. `std::mem::take` is only a
    // move — the fresh collection below reuses the same allocation.
    let previous = std::mem::take(input_targets);
    let mut order = 0;
    let mut path = Vec::new();
    crate::events::collect_input_targets(
        vnode,
        layout,
        None,
        crate::events::StackCtx::ROOT,
        &mut path,
        &mut order,
        input_targets,
    );
    crate::events::preserve_input_state(input_targets, &previous, &|p| {
        input_value_char_len(vnode, p)
    });
}

#[cfg(feature = "skia-native")]
fn with_hover_ids(vnode: &velox_dom::VNode, next_id: &mut u32) -> velox_dom::VNode {
    match vnode {
        velox_dom::VNode::Text(_) => vnode.clone(),
        velox_dom::VNode::Element {
            tag,
            props,
            children,
        } => {
            let mut new_props = props.clone();
            if crate::events::is_hoverable(tag, props) {
                let id = *next_id;
                *next_id += 1;
                new_props = new_props.set("data-hover-id", id.to_string());
            }
            let new_children = children
                .iter()
                .map(|c| with_hover_ids(c, next_id))
                .collect();
            velox_dom::VNode::Element {
                tag: tag.clone(),
                props: new_props,
                children: new_children,
            }
        }
    }
}

/// Caret blink half-period, in milliseconds.
///
/// 530 ms is the conventional caret cadence. It is a named const rather than a
/// literal at the call site because the tick interval, the "solid while
/// typing" grace window, and the post-key re-arm deadline must all agree.
#[cfg(feature = "skia-native")]
const CARET_BLINK_MS: u64 = 530;

/// Default font size (px) assumed for caret hit testing when the cascaded
/// style carries no `font-size`.
#[cfg(feature = "skia-native")]
const DEFAULT_INPUT_FONT_SIZE: f32 = 16.0;

/// Posts a recurring `UserEvent` to the winit event loop so the caret blink has
/// a clock.
///
/// Why there is a thread at all: the loops run with `ControlFlow::Wait`, so they
/// only wake for real OS events. A bare `Instant::now()` deadline check inside
/// the event closure would therefore never fire while the user is idle, and the
/// caret would freeze solid. winit 0.28 has no timer API, so the tick has to
/// come in as a user event.
///
/// Why this is not a shared-state race: the thread owns nothing but an
/// `EventLoopProxy`, a cloned copy of the tick payload, and an `AtomicBool`
/// stop flag. It has no reference to `input_targets`, the VNode, the renderer,
/// or the presenter — it cannot read or write renderer state at all. Every
/// `blink_on` mutation happens on the event-loop thread, inside the event
/// closure, where the renderer is already single-threaded. This is exactly the
/// property a `Mutex<InputState>` design would have had to give up.
///
/// The thread is joined in `Drop`, so closing the window does not leave a
/// detached ticker posting events at a dead loop.
#[cfg(feature = "skia-native")]
struct CaretBlinkTicker<T: Clone + Send + 'static> {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
    /// The tick payload is moved into the thread, so the struct itself does
    /// not otherwise mention `T`. This marker is what ties the type parameter
    /// to the type; it is zero-sized and owns nothing.
    _tick: std::marker::PhantomData<fn() -> T>,
}

#[cfg(feature = "skia-native")]
impl<T: Clone + Send + 'static> CaretBlinkTicker<T> {
    /// Start ticking. `tick` is the payload posted on every interval — `()` for
    /// the plain loop, [`HmrMessage::KeepWindow`] for the HMR loop, whose
    /// channel is typed.
    fn start(proxy: winit::event_loop::EventLoopProxy<T>, tick: T) -> Self {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_thread = std::sync::Arc::clone(&stop);
        let period = std::time::Duration::from_millis(CARET_BLINK_MS);
        let handle = std::thread::Builder::new()
            .name("velox-caret-blink".into())
            .spawn(move || {
                while !stop_thread.load(std::sync::atomic::Ordering::Relaxed) {
                    std::thread::sleep(period);
                    if stop_thread.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                    // winit 0.28's `EventLoopProxy<T>::send_event` takes the
                    // user-event payload `T` directly — the platform layer wraps
                    // it into `Event::UserEvent(..)` on delivery. Sending fails
                    // once the loop is gone, which is the exit condition.
                    if proxy.send_event(tick.clone()).is_err() {
                        break;
                    }
                }
            })
            .ok();
        Self {
            stop,
            handle,
            _tick: std::marker::PhantomData,
        }
    }
}

#[cfg(feature = "skia-native")]
impl<T: Clone + Send + 'static> Drop for CaretBlinkTicker<T> {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Advance the caret blink phase on a ticker event.
///
/// Returns `true` when the phase actually flipped and the frame must be
/// repainted. A tick that arrives inside the "solid while typing" window set by
/// a keypress or a focus change does *not* flip: it only re-arms the deadline,
/// which is what stops the caret from blinking off milliseconds after a
/// keystroke.
#[cfg(feature = "skia-native")]
fn on_caret_blink_tick(
    input_targets: &mut [crate::events::InputTarget],
    blink_deadline: &mut Option<std::time::Instant>,
) -> bool {
    let Some(idx) = crate::events::focused_input_index(input_targets) else {
        return false;
    };
    let now = std::time::Instant::now();
    if let Some(d) = *blink_deadline
        && now < d
    {
        return false;
    }
    input_targets[idx].blink_on = !input_targets[idx].blink_on;
    *blink_deadline = Some(now + std::time::Duration::from_millis(CARET_BLINK_MS));
    true
}

/// Re-arm the blink grace window after a keypress or focus change.
///
/// This is the "caret goes solid while you type" rule: any editing action
/// defers the next phase flip by one cadence period. Without it a keystroke
/// landing just before a tick would be blinked off a few milliseconds later,
/// which reads as flicker rather than as a caret.
#[cfg(feature = "skia-native")]
fn arm_blink_deadline(blink_deadline: &mut Option<std::time::Instant>) {
    *blink_deadline =
        Some(std::time::Instant::now() + std::time::Duration::from_millis(CARET_BLINK_MS));
}

/// Read a declaration out of a cascaded `style` attribute (`"a: b; c: d"`).
#[cfg(feature = "skia-native")]
fn style_decl<'s>(style: &'s str, key: &str) -> Option<&'s str> {
    style
        .split(';')
        .filter_map(|d| d.split_once(':'))
        .find(|(k, _)| k.trim() == key)
        .map(|(_, v)| v.trim())
}

/// Parse a `px` length, rejecting non-finite and non-positive values.
#[cfg(feature = "skia-native")]
fn parse_px(v: &str) -> Option<f32> {
    let n = v.trim().strip_suffix("px")?.trim().parse::<f32>().ok()?;
    (n.is_finite() && n > 0.0).then_some(n)
}

/// Effective font size (px) of a styled element, for caret hit testing.
#[cfg(feature = "skia-native")]
fn resolve_font_size(props: &velox_dom::Props) -> f32 {
    props
        .attrs
        .get("style")
        .and_then(|s| style_decl(s, "font-size"))
        .and_then(parse_px)
        .unwrap_or(DEFAULT_INPUT_FONT_SIZE)
}

/// Left content edge of a text input: the box origin plus `padding-left`.
/// The click-to-caret mapping is relative to this, so padding shifts the caret
/// to match the painted glyphs.
#[cfg(feature = "skia-native")]
fn resolve_text_origin_x(props: &velox_dom::Props, rect: velox_dom::layout::Rect) -> f32 {
    let pad = props
        .attrs
        .get("style")
        .and_then(|s| style_decl(s, "padding-left"))
        .and_then(parse_px)
        .unwrap_or(0.0);
    rect.x as f32 + pad
}

/// The `value` attribute of the element at `path`, if it is an element.
#[cfg(feature = "skia-native")]
fn input_value_at(vnode: &VNode, path: &[usize]) -> Option<String> {
    match find_node_at_path(vnode, path) {
        Some(VNode::Element { props, .. }) => {
            Some(props.attrs.get("value").cloned().unwrap_or_default())
        }
        _ => None,
    }
}

/// Char length of the value at `path`, 0 when there is no such input.
#[cfg(feature = "skia-native")]
fn input_value_char_len(vnode: &VNode, path: &[usize]) -> usize {
    input_value_at(vnode, path)
        .map(|s| s.chars().count())
        .unwrap_or(0)
}

/// Dispatch an explicit new value to the input at `path` via its `on:input`
/// handler, so the app's state and the next `make_view` see the edit.
#[cfg(feature = "skia-native")]
fn dispatch_input_value(
    last_vnode: &Option<VNode>,
    path: &[usize],
    new_value: &str,
    on_event: &mut impl FnMut(&str, Option<&str>),
) {
    let Some(vnode) = last_vnode else { return };
    let Some(node) = find_node_at_path(vnode, path) else {
        return;
    };
    let VNode::Element { props, .. } = node else {
        return;
    };
    let Some(handler) = props.attrs.get("on:input").cloned() else {
        return;
    };
    on_event(&handler, Some(new_value));
}

/// Apply one editing action to the focused input and dispatch any text change.
///
/// Shared by both event loops — that shared call is what keeps the HMR loop
/// from drifting away from the plain one again. Returns `true` when the caller
/// must repaint.
#[cfg(feature = "skia-native")]
fn apply_edit_to_focused(
    input_targets: &mut [crate::events::InputTarget],
    action: crate::events::EditAction,
    last_vnode: &Option<VNode>,
    on_event: &mut impl FnMut(&str, Option<&str>),
) -> bool {
    let Some(idx) = crate::events::focused_input_index(input_targets) else {
        return false;
    };
    let path = input_targets[idx].path.clone();
    let value = last_vnode
        .as_ref()
        .and_then(|v| input_value_at(v, &path))
        .unwrap_or_default();
    let res = crate::events::apply_edit(&mut input_targets[idx], &value, action);
    if res.submit {
        // Submit always dispatches: with no selection the text is unchanged, and
        // the handler still has to see the submission.
        let v = res.value.clone().unwrap_or_else(|| value.clone());
        dispatch_input_value(last_vnode, &path, &v, on_event);
    } else if let Some(new_value) = &res.value {
        dispatch_input_value(last_vnode, &path, new_value, on_event);
    }
    res.needs_repaint()
}

/// Resolve the VirtualKeyCode of a key event into an editing action, honouring
/// the Shift modifier. Returns `None` for keys that are not editing commands.
#[cfg(feature = "skia-native")]
fn edit_action_for_key(
    keycode: winit::event::VirtualKeyCode,
    shift: bool,
) -> Option<crate::events::EditAction> {
    use crate::events::EditAction;
    use winit::event::VirtualKeyCode as K;
    Some(match keycode {
        K::Left => EditAction::MoveLeft { shift },
        K::Right => EditAction::MoveRight { shift },
        K::Home => EditAction::Home { shift },
        K::End => EditAction::End { shift },
        K::Back => EditAction::Backspace,
        K::Delete => EditAction::Delete,
        K::Return => EditAction::Submit,
        _ => return None,
    })
}

/// Inject the caret/selection/focus contract onto every text input in the tree.
///
/// Paint never reads live event state. It reads these five attrs off the styled
/// VNode, which is what makes the caret headless-testable — a golden can be
/// produced from a VNode alone, with no event loop running:
///
/// - `caret`       — caret position as a char index into the value
/// - `caret_blink` — `"true"` when the caret bar is visible this frame
/// - `sel_start`   — selection start char index (== `caret` when collapsed)
/// - `sel_end`     — selection end char index
/// - `focused`     — `"true"` when the input holds keyboard focus
///
/// Runs *after* `style_vnode_with_hover` (the single style-cascade application
/// site) and before layout, so it adds no second cascade and no second
/// rounding. Every text input always carries all five attrs, so paint never has
/// to invent a default for a missing one.
#[cfg(feature = "skia-native")]
fn inject_input_caret_attrs(vnode: &VNode, focused: Option<&crate::events::InputTarget>) -> VNode {
    /// Descend one level. `on_path` means "this node is an ancestor of (or is)
    /// the focused input"; `focused_path` is the remainder to match.
    fn walk(
        node: &VNode,
        focused_path: Option<&[usize]>,
        on_path: bool,
        focused: Option<&crate::events::InputTarget>,
    ) -> VNode {
        match node {
            VNode::Text(_) => node.clone(),
            VNode::Element {
                tag,
                props,
                children,
            } => {
                let mut new_props = props.clone();
                if crate::events::is_text_input(tag, props) {
                    let (is_focused, cursor, blink, sel_start, sel_end) = match focused {
                        Some(t) if on_path => {
                            let (a, b) = t.selection().unwrap_or((t.cursor, t.cursor));
                            (true, t.cursor, t.blink_on, a, b)
                        }
                        // Unfocused inputs still get the attrs, so paint reads
                        // a total contract rather than guessing at absence.
                        _ => (false, 0, true, 0, 0),
                    };
                    new_props = new_props
                        .set("focused", if is_focused { "true" } else { "false" })
                        .set("caret", cursor.to_string())
                        .set("caret_blink", if blink { "true" } else { "false" })
                        .set("sel_start", sel_start.to_string())
                        .set("sel_end", sel_end.to_string());
                }
                let new_children = children
                    .iter()
                    .enumerate()
                    .map(|(i, child)| {
                        let child_on_path = on_path
                            && focused_path
                                .and_then(|p| p.first())
                                .map(|first| *first == i)
                                .unwrap_or(false);
                        let rest = if child_on_path {
                            focused_path.map(|p| &p[1..])
                        } else {
                            None
                        };
                        walk(child, rest, child_on_path, focused)
                    })
                    .collect();
                VNode::Element {
                    tag: tag.clone(),
                    props: new_props,
                    children: new_children,
                }
            }
        }
    }

    let focused_path = focused.map(|t| t.path.as_slice());
    walk(vnode, focused_path, true, focused)
}

/// Effective font family of a styled element, for caret hit testing.
#[cfg(feature = "skia-native")]
fn resolve_font_family(props: &velox_dom::Props) -> String {
    props
        .attrs
        .get("style")
        .and_then(|s| style_decl(s, "font-family"))
        .filter(|f| !f.is_empty())
        .unwrap_or("system-ui, sans-serif")
        .to_string()
}

/// Move keyboard focus to the text input under the cursor (or drop focus),
/// and place the caret.
///
/// Shared by both event loops for the same reason `apply_edit_to_focused` is:
/// the HMR loop used to have no focus handling at all, and a second copy of
/// this is how the drift happened.
///
/// Caret placement differs by case, and the difference is deliberate:
/// - clicking an input that did **not** have focus is a focus *gain*, so the
///   caret goes to the end of the value (the conventional behaviour, and what
///   makes "click a field and start typing" replace rather than prepend);
/// - clicking an input that already had focus is a reposition, so the caret
///   goes to the clicked glyph via [`crate::events::click_to_char_index`].
///
/// Returns `true` when the caller must repaint — i.e. focus actually changed or
/// the caret moved.
#[cfg(feature = "skia-native")]
fn apply_click_focus(
    input_targets: &mut [crate::events::InputTarget],
    last_vnode: &Option<VNode>,
    focused_input: &mut Option<Vec<usize>>,
    x: f32,
    y: f32,
    scale_factor: f32,
) -> bool {
    let Some(idx) = crate::events::hit_test_input_index(input_targets, x, y) else {
        // Clicked empty space: blur. Clearing the selection is what stops a
        // stale highlight from surviving on a field the user has left. The
        // caret position is deliberately kept, so re-entering the field
        // restores where the user was.
        let mut changed = false;
        for t in input_targets.iter_mut() {
            if t.focused || t.anchor.is_some() {
                t.focused = false;
                t.anchor = None;
                t.blink_on = true;
                changed = true;
            }
        }
        if focused_input.take().is_some() {
            changed = true;
        }
        return changed;
    };

    let was_focused = input_targets[idx].focused;
    let rect = input_targets[idx].rect;
    let path = input_targets[idx].path.clone();
    let mut caret: Option<usize> = None;
    let mut value_len: Option<usize> = None;
    if let Some(vnode) = last_vnode.as_ref()
        && let Some(VNode::Element { props, .. }) = find_node_at_path(vnode, &path)
    {
        let value = props.attrs.get("value").cloned().unwrap_or_default();
        value_len = Some(value.chars().count());
        let font_size = resolve_font_size(props);
        let config = crate::text::TextRenderConfig::new(&resolve_font_family(props), font_size);
        let origin_x = resolve_text_origin_x(props, rect);
        // Measure through the renderer's own text stack (single measure path,
        // skia-aware under `skia-native`) rather than a local heuristic, so the
        // caret lands on the glyph the paint lane will actually draw.
        let measure =
            |s: &str| crate::text::TextMeasurer::measure_with_scale(s, &config, scale_factor).0;
        caret = Some(crate::events::click_to_char_index(
            &value, rect, x, origin_x, font_size, &measure,
        ));
    }

    input_targets[idx].focused = true;
    input_targets[idx].blink_on = true;
    // A plain click is never a selection-extension; it collapses one.
    input_targets[idx].anchor = None;
    // `cursor` is a char index, so "the end" is the char count, not `len()`.
    input_targets[idx].cursor = if was_focused {
        caret.unwrap_or(input_targets[idx].cursor)
    } else {
        // Focus gain: caret to the end of the value.
        value_len.unwrap_or(input_targets[idx].cursor)
    };
    *focused_input = Some(path);
    true
}

pub mod event_binding;
pub mod events;
pub mod hmr;
pub mod text;
pub mod viewport;

pub use hmr::{DEFAULT_HMR_PORT, HmrMessage, hmr_config, run_hmr_client};
pub use viewport::{LogicalSize, PhysicalSize, Viewport};

// Native Skia GL helper module (feature-gated)
#[cfg(feature = "skia-native")]
mod skia_gl;
// Skia surface and renderer helpers (feature-gated)
#[cfg(feature = "skia-native")]
mod skia_render;
#[cfg(feature = "skia-native")]
pub mod skia_surface;
// Softbuffer presenter for window rendering (feature-gated)
#[cfg(feature = "skia-native")]
mod presenter;
#[cfg(feature = "skia-native")]
pub use skia_render::skia_impl::render_vnode_to_rgba;
#[cfg(feature = "skia-native")]
pub use skia_render::{render_vnode_to_raster_png, render_vnode_to_raster_png_with_scale};

/// In-memory representation of a mounted tree (stubbed for now).
pub struct RenderTree {
    pub root: VNode,
    pub node_count: usize,
    pub text_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct A11yNode {
    pub id: usize,
    pub role: String,
    pub name: String,
    pub rect: velox_dom::layout::Rect,
    pub children: Vec<A11yNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct A11yTree {
    pub root: A11yNode,
}

pub trait HmrRenderer {
    /// Initialize the rendering backend (e.g. create DirectContext, verify GPU).
    fn init() -> Result<(), String>
    where
        Self: Sized;

    /// Mount a VNode into the renderer for display.
    fn mount(&mut self, vnode: VNode) -> Result<(), String>;

    /// Hot-update the rendered VNode.
    fn hot_update(&mut self, new_vnode: VNode) -> Result<(), String>;

    /// Get the raw window handle for platform integration.
    fn get_window_handle(&self) -> *mut std::ffi::c_void;
}

/// High-level renderer lifecycle trait. Backends implement this to expose
/// a consistent `new -> mount -> hot_update` workflow that returns `Result`
/// instead of panicking on failure.
pub trait VeloxRenderer {
    /// Construct a new renderer instance.
    fn new() -> Result<Self, String>
    where
        Self: Sized;

    /// Mount a VNode tree for rendering, returning an error on failure.
    fn mount(&mut self, vnode: VNode) -> Result<(), String>;

    /// Hot-replace the rendered VNode tree, returning an error on failure.
    fn hot_update(&mut self, new_vnode: VNode) -> Result<(), String>;
}

fn summarize(v: &VNode, counts: &mut (usize, usize)) {
    match v {
        VNode::Text(_) => {
            counts.0 += 1;
            counts.1 += 1;
        }
        VNode::Element { children, .. } => {
            counts.0 += 1;
            for c in children {
                summarize(c, counts);
            }
        }
    }
}

fn build_render_tree(v: &VNode) -> RenderTree {
    let mut counts = (0, 0);
    summarize(v, &mut counts);
    RenderTree {
        root: v.clone(),
        node_count: counts.0,
        text_count: counts.1,
    }
}

fn vnode_text_content(node: &VNode) -> String {
    match node {
        VNode::Text(t) => t.clone(),
        VNode::Element { children, .. } => {
            let mut out = String::new();
            for ch in children {
                let s = vnode_text_content(ch);
                if !s.is_empty() {
                    if !out.is_empty() {
                        out.push(' ');
                    }
                    out.push_str(&s);
                }
            }
            out
        }
    }
}

fn a11y_role_for(tag: &str, props: &velox_dom::Props) -> String {
    if let Some(role) = props.attrs.get("role") {
        return role.clone();
    }
    match tag {
        "button" => "button",
        "img" => "image",
        "input" => "textbox",
        "label" => "label",
        "a" => "link",
        _ => "group",
    }
    .to_string()
}

fn a11y_name_for(tag: &str, props: &velox_dom::Props, node: &VNode) -> String {
    if let Some(label) = props.attrs.get("aria-label") {
        return label.clone();
    }
    if tag == "img"
        && let Some(alt) = props.attrs.get("alt")
    {
        return alt.clone();
    }
    vnode_text_content(node)
}

fn build_a11y_tree_with_layout(
    vnode: &VNode,
    layout: &velox_dom::layout::LayoutNode,
    next_id: &mut usize,
) -> A11yNode {
    let id = *next_id;
    *next_id += 1;
    match vnode {
        VNode::Text(t) => A11yNode {
            id,
            role: "text".to_string(),
            name: t.clone(),
            rect: layout.rect,
            children: Vec::new(),
        },
        VNode::Element {
            tag,
            props,
            children,
            ..
        } => {
            let mut child_nodes = Vec::new();
            for ch_layout in &layout.children {
                if ch_layout.display_none {
                    continue;
                }
                if let Some(src_idx) = ch_layout.source_index
                    && let Some(ch) = children.get(src_idx)
                {
                    child_nodes.push(build_a11y_tree_with_layout(ch, ch_layout, next_id));
                }
            }
            A11yNode {
                id,
                role: a11y_role_for(tag, props),
                name: a11y_name_for(tag, props, vnode),
                rect: layout.rect,
                children: child_nodes,
            }
        }
    }
}

/// Build the accessibility tree for `vnode` laid out at `width` x `height`.
///
/// DELIBERATE EXCEPTION to R-8's single-funnel invariant: this function runs its
/// own `compute_layout` and does not go through the renderer's `prepare_frame`.
///
/// The reason is that this is not a render path. It takes no `Stylesheet`, paints
/// nothing, and is given no `SkiaSurface`; its only output is a tree of roles,
/// names and rects. Funnelling it would mean inventing a stylesheet and a surface
/// for a caller that has neither, and then applying a style cascade to a tree
/// whose geometry — not its appearance — is what is being reported. It is named
/// here so the exception is on the record rather than discovered later as a
/// "missing" funnel site.
pub fn build_a11y_tree(vnode: &VNode, width: i32, height: i32) -> A11yTree {
    let layout = velox_dom::layout::compute_layout(vnode, width, height);
    let mut next_id = 1;
    let root = build_a11y_tree_with_layout(vnode, &layout, &mut next_id);
    A11yTree { root }
}

/// Reconcile two VNode children vectors using an optional `key` prop.
///
/// # This helper is wrong, and it is on a dead path
///
/// On a `key` match it pushes `old[idx].clone()` and discards the incoming node,
/// so a keyed child whose text, attrs or children changed keeps the **stale**
/// old content — the new content never reaches the tree. The `used` set below is
/// written and never read. Deleting the helper outright is blocked: seven live
/// tests across `tests/reconcile_keyed_tests.rs` and
/// `tests/event_lifecycle_tests.rs` call it, and `tests/reconcile_keyed_tests.rs`
/// *asserts the stale-content behaviour as intended* ("since we reused the old
/// node, its child text remains the original"). Whether to delete it or to
/// redefine it is an open decision.
///
/// It is also only reachable from `run_window_vnode`, which has zero in-tree
/// callers: every example and every scaffolded project calls the two `skia`
/// entry points instead. And it reconciles only the root's direct children, so
/// any `v-for` below the root is never touched.
///
/// The correct implementation already exists and is unused:
/// `velox_dom::diff::diff` / `diff_children_keyed`, which emit
/// `Patch::MoveChild` and are duplicate-key safe.
///
/// `:key` itself is a stated non-goal: it is a plain runtime `key` attribute
/// that does not reorder, diff, or preserve identity. See the module docs in
/// `velox-dom/src/diff.rs` for why identity preservation is not implementable
/// without changing the public shape of `VNode`.
pub fn reconcile_keyed_children(old: &mut Vec<VNode>, new: &[VNode]) {
    let mut key_to_index: HashMap<String, usize> = HashMap::new();
    for (i, n) in old.iter().enumerate() {
        if let VNode::Element { props, .. } = n
            && let Some(k) = props.attrs.get("key")
        {
            key_to_index.insert(k.clone(), i);
        }
    }
    let mut used: HashSet<usize> = HashSet::new();
    let mut out: Vec<VNode> = Vec::with_capacity(new.len());
    for nn in new.iter() {
        if let VNode::Element { props: nprops, .. } = nn
            && let Some(k) = nprops.attrs.get("key")
            && let Some(&idx) = key_to_index.get(k)
        {
            out.push(old[idx].clone());
            used.insert(idx);
            continue;
        }
        out.push(nn.clone());
    }
    *old = out;
}

/// Minimal renderer trait. Backends implement this to expose a consistent API.
pub trait Renderer {
    fn backend_name(&self) -> &'static str;
    fn mount(&self, vnode: &VNode) -> Result<RenderTree, String>;
}

#[cfg(feature = "wgpu")]
pub mod wgpu_backend {
    use wgpu as _wgpu;
    // use winit as _winit; // TODO: re-enable when wgpu is used

    pub fn init() {
        // Attempt headless WGPU initialization to verify adapter/device availability.
        // This is intentionally best-effort and will not panic on failure; it logs to stderr.
        let instance = _wgpu::Instance::new(_wgpu::InstanceDescriptor {
            backends: _wgpu::Backends::all(),
            dx12_shader_compiler: Default::default(),
        });
        // Try to get a real adapter first; if none is found (common in CI),
        // retry requesting a fallback adapter (software renderer) before giving up.
        let adapter =
            match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: _wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })) {
                Some(a) => a,
                None => {
                    log::warn!("wgpu backend: no adapter found; retrying with fallback adapter...");
                    match pollster::block_on(instance.request_adapter(
                        &wgpu::RequestAdapterOptions {
                            power_preference: _wgpu::PowerPreference::HighPerformance,
                            compatible_surface: None,
                            force_fallback_adapter: true,
                        },
                    )) {
                        Some(a2) => a2,
                        None => {
                            log::error!(
                                "wgpu backend: no adapter found even with fallback (init skipped)"
                            );
                            return;
                        }
                    }
                }
            };

        match pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("velox-wgpu-device"),
                features: wgpu::Features::empty(),
                limits: wgpu::Limits::default(),
            },
            None,
        )) {
            Ok((_device, _queue)) => {
                let info = adapter.get_info();
                log::info!("wgpu backend: init OK — adapter='{}'", info.name);
            }
            Err(e) => {
                log::error!("wgpu backend: failed to request device: {:?}", e);
            }
        }
    }

    pub struct WgpuRenderer;
    impl crate::Renderer for WgpuRenderer {
        fn backend_name(&self) -> &'static str {
            "wgpu"
        }
        fn mount(&self, vnode: &velox_dom::VNode) -> Result<crate::RenderTree, String> {
            // Try a GPU-backed present; log errors but do not fail the mount.
            #[cfg(all(feature = "skia-native", unix))]
            {
                if let Err(e) = crate::skia_gl::draw_gpu_test_frame(256, 256) {
                    log::error!("skia backend: GPU present failed: {}", e);
                }
            }
            Ok(crate::build_render_tree(vnode))
        }
    }
}

// Real Skia backend only when `skia-native` is enabled.
#[cfg(feature = "skia-native")]
pub mod skia_backend {
    use crate::HmrRenderer;
    use crate::VeloxRenderer;
    #[cfg(feature = "skia-native")]
    use crate::skia_gl;
    #[cfg(feature = "skia-native")]
    use crate::skia_surface;
    #[cfg(feature = "skia-native")]
    use raw_window_handle::HasRawWindowHandle;
    use velox_dom::VNode;

    pub fn init() -> Result<(), String> {
        match skia_gl::create_context() {
            Ok(gl_ctx) => match gl_ctx.into_direct_context() {
                Some(_dctx) => {
                    log::info!("skia backend: init OK (DirectContext created)");
                    Ok(())
                }
                None => Err("skia backend: init failed: couldn't create DirectContext".to_string()),
            },
            Err(e) => Err(format!("skia backend: init failed: {}", e)),
        }
    }

    pub struct SkiaRenderer {
        pub surface: Option<skia_surface::SkiaSurface>,
        pub vnode: Option<VNode>,
    }

    impl SkiaRenderer {
        pub fn with_window(
            window: &impl HasRawWindowHandle,
            width: i32,
            height: i32,
        ) -> Result<Self, String> {
            match skia_surface::create_window_surface_from_handle(window, width, height) {
                Ok(s) => Ok(SkiaRenderer {
                    surface: Some(s),
                    vnode: None,
                }),
                Err(e) => Err(e),
            }
        }

        pub fn present(&mut self) -> Result<(), String> {
            if let Some(s) = &mut self.surface {
                s.present()
            } else {
                Ok(())
            }
        }

        pub fn resize(&mut self, width: i32, height: i32) -> Result<(), String> {
            if let Some(s) = &mut self.surface {
                s.resize(width, height)
            } else {
                Ok(())
            }
        }
    }

    impl crate::Renderer for SkiaRenderer {
        fn backend_name(&self) -> &'static str {
            "skia"
        }
        fn mount(&self, vnode: &velox_dom::VNode) -> Result<crate::RenderTree, String> {
            Ok(crate::build_render_tree(vnode))
        }
    }

    impl HmrRenderer for SkiaRenderer {
        fn init() -> Result<(), String> {
            init()
        }

        fn mount(&mut self, vnode: VNode) -> Result<(), String> {
            self.vnode = Some(vnode);
            Ok(())
        }

        fn hot_update(&mut self, new_vnode: VNode) -> Result<(), String> {
            self.vnode = Some(new_vnode);
            Ok(())
        }

        fn get_window_handle(&self) -> *mut std::ffi::c_void {
            std::ptr::null_mut()
        }
    }

    impl VeloxRenderer for SkiaRenderer {
        fn new() -> Result<Self, String> {
            Ok(SkiaRenderer {
                surface: None,
                vnode: None,
            })
        }

        fn mount(&mut self, vnode: VNode) -> Result<(), String> {
            self.vnode = Some(vnode);
            Ok(())
        }

        fn hot_update(&mut self, new_vnode: VNode) -> Result<(), String> {
            self.vnode = Some(new_vnode);
            Ok(())
        }
    }
}

// Skia stub backend to allow compiling with `--features skia` without native deps.
#[cfg(all(feature = "skia", not(feature = "skia-native")))]
pub mod skia_backend {
    pub fn init() -> Result<(), String> {
        Ok(())
    }

    pub struct SkiaRenderer;
    impl crate::Renderer for SkiaRenderer {
        fn backend_name(&self) -> &'static str {
            "skia"
        }
        fn mount(&self, vnode: &velox_dom::VNode) -> Result<crate::RenderTree, String> {
            Ok(crate::build_render_tree(vnode))
        }
    }
    impl crate::HmrRenderer for SkiaRenderer {
        fn init() -> Result<(), String> {
            Ok(())
        }
        fn mount(&mut self, _vnode: velox_dom::VNode) -> Result<(), String> {
            Ok(())
        }
        fn hot_update(&mut self, _new_vnode: velox_dom::VNode) -> Result<(), String> {
            Ok(())
        }
        fn get_window_handle(&self) -> *mut std::ffi::c_void {
            std::ptr::null_mut()
        }
    }
    impl crate::VeloxRenderer for SkiaRenderer {
        fn new() -> Result<Self, String> {
            Ok(SkiaRenderer)
        }
        fn mount(&mut self, _vnode: velox_dom::VNode) -> Result<(), String> {
            Ok(())
        }
        fn hot_update(&mut self, _new_vnode: velox_dom::VNode) -> Result<(), String> {
            Ok(())
        }
    }
}

/// Stub init used when no backend features are enabled.
#[cfg(not(any(feature = "wgpu", feature = "skia")))]
pub fn init() -> Result<(), String> {
    // Intentionally empty — no backend enabled
    Ok(())
}

// Simple identifier of the selected backend, useful for tests.
#[cfg(all(feature = "wgpu", feature = "skia"))]
pub const BACKEND: &str = "wgpu+skia";
#[cfg(all(feature = "wgpu", not(feature = "skia")))]
pub const BACKEND: &str = "wgpu";
#[cfg(all(not(feature = "wgpu"), feature = "skia"))]
pub const BACKEND: &str = "skia";
#[cfg(all(not(feature = "wgpu"), not(feature = "skia")))]
pub const BACKEND: &str = "stub";

pub fn backend_name() -> &'static str {
    BACKEND
}

/// Feature-selected renderer type and constructor for tests and examples.
#[cfg(feature = "wgpu")]
pub type SelectedRenderer = wgpu_backend::WgpuRenderer;
#[cfg(all(not(feature = "wgpu"), any(feature = "skia", feature = "skia-native")))]
pub type SelectedRenderer = skia_backend::SkiaRenderer;
#[cfg(all(not(feature = "wgpu"), not(feature = "skia")))]
pub struct StubRenderer;
#[cfg(all(not(feature = "wgpu"), not(feature = "skia")))]
pub type SelectedRenderer = StubRenderer;
#[cfg(all(not(feature = "wgpu"), not(feature = "skia")))]
impl Renderer for StubRenderer {
    fn backend_name(&self) -> &'static str {
        "stub"
    }
    fn mount(&self, vnode: &VNode) -> Result<RenderTree, String> {
        Ok(build_render_tree(vnode))
    }
}
#[cfg(all(not(feature = "wgpu"), not(feature = "skia")))]
impl HmrRenderer for StubRenderer {
    fn init() -> Result<(), String> {
        Ok(())
    }
    fn mount(&mut self, _vnode: VNode) -> Result<(), String> {
        Ok(())
    }
    fn hot_update(&mut self, _new_vnode: VNode) -> Result<(), String> {
        Ok(())
    }
    fn get_window_handle(&self) -> *mut std::ffi::c_void {
        std::ptr::null_mut()
    }
}
#[cfg(all(not(feature = "wgpu"), not(feature = "skia")))]
impl VeloxRenderer for StubRenderer {
    fn new() -> Result<Self, String> {
        Ok(StubRenderer)
    }
    fn mount(&mut self, _vnode: VNode) -> Result<(), String> {
        Ok(())
    }
    fn hot_update(&mut self, _new_vnode: VNode) -> Result<(), String> {
        Ok(())
    }
}

/// Construct the feature-selected renderer.
pub fn new_selected_renderer() -> SelectedRenderer {
    #[cfg(feature = "wgpu")]
    {
        wgpu_backend::WgpuRenderer
    }
    #[cfg(all(not(feature = "wgpu"), feature = "skia-native"))]
    {
        // Construct SkiaRenderer with no surface for the default selected renderer.
        skia_backend::SkiaRenderer {
            surface: None,
            vnode: None,
        }
    }
    #[cfg(all(not(feature = "wgpu"), feature = "skia", not(feature = "skia-native")))]
    {
        skia_backend::SkiaRenderer
    }
    #[cfg(all(not(feature = "wgpu"), not(feature = "skia")))]
    {
        StubRenderer
    }
}

pub use events::Runtime as EventRuntime;

/// Test helper: exercise a small Skia draw path (native-only).
#[cfg(all(feature = "skia-native", unix))]
pub fn skia_draw_test_frame() -> Result<(), String> {
    crate::skia_gl::draw_gpu_test_frame(256, 256)
}

/// Convenience wrapper to create a Skia `DirectContext` from the crate root.
#[cfg(all(feature = "skia-native", unix))]
pub fn create_direct_context() -> Result<skia_safe::gpu::DirectContext, String> {
    crate::skia_gl::create_direct_context()
}

/// Extracts the human-readable message from a caught panic payload
/// (`panic!("literal")` stores `&'static str`, `format!`-based panics store
/// `String`). Used so compositor failures caught by `catch_unwind` are
/// surfaced instead of silently discarded (CX-13 / F-23).
#[cfg(feature = "skia-native")]
fn panic_detail(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

/// Run a Skia window whose contents are produced by `make_view`.
///
/// # Viewport contract (1A / X-H1) — Viewport Root Normalization (CX-04)
///
/// `make_view` is `FnMut(w: u32, h: u32) -> (VNode, Stylesheet)` where
/// `(w, h)` are **logical viewport dimensions** — `Viewport::from_i32(physical, scale).logical_size()`.
/// The renderer calls it:
/// * once on startup,
/// * on every `RedrawRequested` / resize / DPI change with the *current* logical `w,h`.
///   Templates must not ignore `(w,h)` (no `|_w, _h|`). The canonical
///   responsive pattern is a viewport-filling root:
///   `width: 100%; min-height: 100vh` on `.app` (see `velox-cli/templates/project/src/App.vx`).
///   With `width:100%` and `min-height:100vh`, `compute_layout(vnode, w as i32, h as i32)`
///   reflows visibly on every window resize — no element hidden when it should be visible
///   (Flutter invariant). Callers may also thread `(w,h)` into style/layout decisions if needed.
///
/// ## Root normalization
///
/// `velox_dom::layout::root_is_viewport_filling` ensures the *first* VNode (index 0 / `None`)
/// always fills the logical viewport — even without explicit `width:100%`. The expanded
/// predicate `is_viewport_filling(style, is_root_index)` additionally treats `100% | 100vw | 100dvw | 100vh | 100dvh | min-height:100%|vh|dvh`
/// as viewport-filling, validated via `compute_layout` percent chains (`height:100%` fills
/// when parent is definite) and `vw/vh/dvh` units that resolve against `Viewport::logical_size`.
#[cfg(feature = "skia-native")]
pub fn run_window_vnode_skia<F, G, H>(
    title: &str,
    mut make_view: F,
    mut on_event: G,
    mut get_title: H,
) -> Result<(), String>
where
    F: FnMut(u32, u32) -> (velox_dom::VNode, Stylesheet) + 'static,
    G: FnMut(&str, Option<&str>) + 'static,
    H: FnMut() -> String + 'static,
{
    use winit::dpi::PhysicalSize;
    use winit::event::{ElementState, Event, MouseButton, StartCause, WindowEvent};
    use winit::event_loop::{ControlFlow, EventLoop};
    use winit::window::WindowBuilder;

    // Headless mode: when no compositor is available (CI, containers, SSH)
    // we still create the window + run the event loop but skip softbuffer
    // presentation, rendering offscreen only. Enable with VELOX_HEADLESS=1
    // or it is auto-detected via presenter::is_compositor_available().
    let headless_env = std::env::var("VELOX_HEADLESS").as_deref() == Ok("1")
        || !crate::presenter::is_compositor_available();

    let mut last_vnode: Option<velox_dom::VNode> = None;
    let mut _hmr_pending = false;

    // Prepare the winit backend: force X11 if Wayland socket is stale to
    // avoid winit's Wayland backend calling process::exit() on EPIPE.
    let _headless_check = crate::presenter::prepare_backend();

    // Try to create a winit event loop and window. In headless mode or when
    // no compositor is available, both can fail (winit may panic instead of
    // returning Err) — we catch this and proceed with headless rendering.
    let (event_loop_opt, window, window_size, scale_factor): (
        Option<winit::event_loop::EventLoop<()>>,
        Option<winit::window::Window>,
        PhysicalSize<u32>,
        f32,
    ) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let event_loop = EventLoop::new();
        let window_result = WindowBuilder::new()
            .with_title(title)
            .with_inner_size(PhysicalSize::new(800, 600))
            .build(&event_loop);
        match window_result {
            Ok(w) => {
                let size = w.inner_size();
                let sf = w.scale_factor() as f32;
                (Some(event_loop), Some(w), size, sf)
            }
            Err(e) => {
                let msg = e.to_string();
                let lower = msg.to_ascii_lowercase();
                let is_display_err = lower.contains("broken pipe")
                    || lower.contains("os error 32")
                    || lower.contains("no compositor")
                    || lower.contains("no display server")
                    || lower.contains("failed to connect");
                if headless_env || is_display_err {
                    log::warn!("window creation failed — continuing in headless mode: {msg}");
                    (Some(event_loop), None, PhysicalSize::new(800, 600), 1.0)
                } else {
                    panic!("failed to create window: {e}");
                }
            }
        }
    }))
    .unwrap_or_else(|payload| {
        // Surface the panic payload (usually the compositor error, e.g.
        // broken pipe) instead of silently masking it — log::warn is
        // invisible without an initialized logger (CX-13 / F-23).
        eprintln!(
            "[velox] window/event loop creation panicked — continuing in headless mode: {}",
            panic_detail(payload)
        );
        (None, None, PhysicalSize::new(800, 600), 1.0)
    });

    let window_opt: Option<winit::window::Window> = window;
    let mut renderer = match crate::skia_surface::SkiaSurface::new_raster(
        window_size.width as i32,
        window_size.height as i32,
    ) {
        Ok(surface) => skia_backend::SkiaRenderer {
            surface: Some(surface),
            vnode: None,
        },
        Err(e) => {
            return Err(format!("failed to create SkiaSurface: {e}"));
        }
    };
    let mut presenter: Option<crate::presenter::SoftbufferPresenter> = None;
    if let Some(w) = window_opt.as_ref() {
        // softbuffer::Context::new() can panic when the display server is
        // unreachable even though DISPLAY/WAYLAND_DISPLAY are set (broken pipe).
        // Catch such panics and degrade to headless rendering.
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::presenter::SoftbufferPresenter::new(w, window_size.width, window_size.height)
        })) {
            Ok(Ok(p)) => presenter = Some(p),
            Ok(Err(e)) => {
                if headless_env {
                    log::warn!(
                        "softbuffer presenter unavailable — continuing in headless mode: {e}"
                    );
                } else {
                    return Err(format!("failed to create softbuffer presenter: {e}"));
                }
            }
            Err(payload) => {
                // Surface the panic payload (softbuffer panics with a
                // broken pipe when the display is unreachable) instead of
                // silently masking it (CX-13 / F-23).
                eprintln!(
                    "[velox] softbuffer presenter creation panicked — continuing in headless mode: {}",
                    panic_detail(payload)
                );
            }
        }
    }
    let mut scale_factor = scale_factor;
    let mut mouse_pos = (0.0f32, 0.0f32);
    let mut hovered_id: Option<u32> = None;
    let mut click_targets: Vec<crate::events::ClickTarget> = Vec::new();
    let mut hover_targets: Vec<crate::events::HoverTarget> = Vec::new();
    let mut input_targets: Vec<crate::events::InputTarget> = Vec::new();
    // Path (child source indices) to the focused text input, if any.
    let mut focused_input: Option<Vec<usize>> = None;
    // Instant before which the caret must stay solid regardless of tick
    // timing. Armed by every editing key and by focus changes.
    let mut blink_deadline: Option<std::time::Instant> = None;
    // Shift state, latched from `WindowEvent::ModifiersChanged`. winit 0.28
    // deprecates `KeyboardInput::modifiers` in favour of this event, so the
    // caret reads the latched value instead of the deprecated field.
    let mut shift_held = false;
    // Scrollable overflow model: wheel clamping + deepest hit_test
    let mut scroll_offsets: std::collections::HashMap<Vec<usize>, f32> =
        std::collections::HashMap::new();
    let mut last_layout: Option<velox_dom::layout::LayoutNode> = None;
    // Lifecycle: ensure on_mounted fires once on first RedrawRequested and
    // on_unmounted/before_destroy fire on CloseRequested or drop.
    let _lifecycle_guard = LifecycleCleanupGuard;
    let mut did_mount = false;
    // R-L3: coalesce rapid resize drags — only last size per frame materializes.
    // Surface recreation (raster_n32_premul) is deferred to RedrawRequested.
    let mut resize_state = ResizeState::new();

    // Render first frame immediately before entering the event loop.
    // This ensures the window has content even on platforms where
    // request_redraw() from NewEvents(StartCause::Init) may not trigger
    // a RedrawRequested event (e.g. certain Wayland/X11 compositors).

    if let Some(s) = &mut renderer.surface {
        s.set_scale_factor(scale_factor);
        let (vw, vh) = resize_state.record_initial(s.width, s.height, scale_factor);
        let (vnode_raw, sheet) = make_view(vw, vh);
        let mut next_id = 1u32;
        let vnode_tagged = with_hover_ids(&vnode_raw, &mut next_id);
        let vnode = crate::style_vnode_with_hover(&vnode_tagged, &sheet, &|_tag, props| {
            props
                .attrs
                .get("data-hover-id")
                .and_then(|v| v.parse::<u32>().ok())
                .map(|id| Some(id) == hovered_id)
                .unwrap_or(false)
        });
        // First frame: no input can be focused yet, so every text input gets
        // the unfocused defaults. Injected anyway so the attr contract holds on
        // frame one and paint never has to special-case absence.
        let vnode = crate::inject_input_caret_attrs(&vnode, None);
        let mut layout = velox_dom::layout::compute_layout(&vnode, vw as i32, vh as i32);
        {
            let mut path = Vec::new();
            crate::events::apply_scroll_offsets(&mut layout, &scroll_offsets, &mut path);
        }
        last_layout = Some(layout.clone());
        recompute_targets(
            &vnode,
            &layout,
            &mut click_targets,
            &mut hover_targets,
            &mut input_targets,
        );
        // Render and present the initial frame so the window has immediate content.
        if let Err(e) = crate::skia_render::skia_impl::render_frame(s, &vnode, &layout, &sheet) {
            log::error!("skia initial render error: {}", e);
        }
        if let Some(presenter) = presenter.as_mut()
            && let Err(e) = presenter.present(s)
        {
            log::error!("skia initial present error: {}", e);
        }
    }

    if let Some(event_loop) = event_loop_opt {
        // The event loop can panic if the display server becomes unreachable
        // (e.g. "Io error: Broken pipe") — catch that and degrade gracefully.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Caret blink clock. `EventLoop::new()` is an alias for
            // `EventLoopBuilder::new().build()`, and that constructor calls
            // `with_user_event()`, so the user-event channel is already enabled
            // here and `create_proxy()` needs no builder change. Held in a local
            // inside the unwind closure so its `Drop` joins the thread when the
            // loop ends.
            let _blink_ticker = crate::CaretBlinkTicker::start(event_loop.create_proxy(), ());
            event_loop.run(move |event, _, control_flow| {
                *control_flow = ControlFlow::Wait;
                match event {
                    Event::NewEvents(StartCause::Init) => {
                        if let Some(w) = window_opt.as_ref() {
                            w.request_redraw();
                        }
                    }
                    Event::WindowEvent {
                        event: WindowEvent::CloseRequested,
                        ..
                    } => {
                        velox_core::lifecycle::run_all_destroy_hooks();
                        *control_flow = ControlFlow::Exit;
                    }
                    Event::WindowEvent {
                        event: WindowEvent::Resized(new_size),
                        ..
                    } => {
                        // R-L3: coalesce — do not touch raster surface here; defer to RedrawRequested.
                        resize_state.queue((new_size.width, new_size.height));
                        // `ResizeState::queue` is the production `pending_resize = Some(...)` path.
                        if let Some(w) = window_opt.as_ref() {
                            w.request_redraw();
                        }
                    }
                    Event::WindowEvent {
                        event:
                            WindowEvent::ScaleFactorChanged {
                                scale_factor: new_scale,
                                new_inner_size,
                                ..
                            },
                        ..
                    } => {
                        let old_scale = scale_factor;
                        scale_factor = new_scale as f32;
                        // R-M1: atomically rescale mouse_pos to keep physical cursor stable
                        if old_scale.is_finite()
                            && old_scale > 0.0
                            && scale_factor.is_finite()
                            && scale_factor > 0.0
                        {
                            mouse_pos.0 = mouse_pos.0 * old_scale / scale_factor;
                            mouse_pos.1 = mouse_pos.1 * old_scale / scale_factor;
                        }
                        // R-L3: coalesce renderer/presenter resize to RedrawRequested as well.
                        resize_state.queue((new_inner_size.width, new_inner_size.height));
                        if let Some(s) = &mut renderer.surface {
                            s.set_scale_factor(scale_factor);
                        }
                        // Hit-test/layout refresh deferred to RedrawRequested (single layout/frame, R-H5).
                        if let Some(w) = window_opt.as_ref() {
                            w.request_redraw();
                        }
                    }
                    Event::WindowEvent {
                        event: WindowEvent::CursorMoved { position, .. },
                        ..
                    } => {
                        mouse_pos = (
                            position.x as f32 / scale_factor,
                            position.y as f32 / scale_factor,
                        );
                        let now_hovered =
                            crate::events::hit_test_hover(&hover_targets, mouse_pos.0, mouse_pos.1);
                        if now_hovered != hovered_id {
                            hovered_id = now_hovered;
                            if let Some(w) = window_opt.as_ref() {
                                w.request_redraw();
                            }
                        }
                    }
                    Event::WindowEvent {
                        event:
                            WindowEvent::MouseInput {
                                state: ElementState::Pressed,
                                button: MouseButton::Left,
                                ..
                            },
                        ..
                    } => {
                        // Text-input focus: clicking a text field focuses it and
                        // places the caret; clicking anywhere else drops focus and
                        // clears the selection. This runs BEFORE the click-handler
                        // test and is deliberately independent of it — see the
                        // redraw hoist at the bottom of this arm.
                        crate::apply_click_focus(
                            &mut input_targets,
                            &last_vnode,
                            &mut focused_input,
                            mouse_pos.0,
                            mouse_pos.1,
                            scale_factor,
                        );
                        let mut handled_click = false;
                        if let Some((handler, payload_opt)) =
                            crate::events::hit_test_click(&click_targets, mouse_pos.0, mouse_pos.1)
                        {
                            let payload_owned =
                                payload_opt.map(|p| p.to_string()).unwrap_or_else(|| {
                                    format!("{{\"x\":{},\"y\":{}}}", mouse_pos.0, mouse_pos.1)
                                });
                            on_event(handler, Some(&payload_owned));
                            velox_core::lifecycle::run_all_updated_hooks();
                            if let Some(s) = &mut renderer.surface {
                                let (vw, vh) = logical_size(s.width, s.height, scale_factor);
                                let (vnode_raw, sheet) = make_view(vw, vh);
                                let mut next_id = 1u32;
                                let vnode_tagged = with_hover_ids(&vnode_raw, &mut next_id);
                                let vnode = crate::style_vnode_with_hover(
                                    &vnode_tagged,
                                    &sheet,
                                    &|_tag, props| {
                                        props
                                            .attrs
                                            .get("data-hover-id")
                                            .and_then(|v| v.parse::<u32>().ok())
                                            .map(|id| Some(id) == hovered_id)
                                            .unwrap_or(false)
                                    },
                                );
                                let vnode = crate::inject_input_caret_attrs(
                                    &vnode,
                                    crate::events::focused_input_index(&input_targets)
                                        .and_then(|i| input_targets.get(i)),
                                );
                                let mut layout =
                                    velox_dom::layout::compute_layout(&vnode, vw as i32, vh as i32);
                                {
                                    let mut path = Vec::new();
                                    crate::events::apply_scroll_offsets(
                                        &mut layout,
                                        &scroll_offsets,
                                        &mut path,
                                    );
                                }
                                last_layout = Some(layout.clone());
                                recompute_targets(
                                    &vnode,
                                    &layout,
                                    &mut click_targets,
                                    &mut hover_targets,
                                    &mut input_targets,
                                );
                            }
                            handled_click = true;
                        }
                        if let Some(w) = window_opt.as_ref() {
                            if handled_click {
                                w.set_title(&get_title());
                            }
                            // HOISTED OUT of the `hit_test_click` arm on purpose.
                            // This used to live inside it, so a click that hit
                            // empty space changed focus and then never repainted:
                            // the focus ring and caret were stuck at their old
                            // values. Focus is now state the frame depends on, so
                            // any change to it must schedule a redraw.
                            w.request_redraw();
                        }
                    }
                    Event::WindowEvent {
                        event: WindowEvent::ModifiersChanged(mods),
                        ..
                    } => {
                        // Latch modifier state for Shift-extends-selection.
                        shift_held = mods.shift();
                    }
                    Event::WindowEvent {
                        event: WindowEvent::KeyboardInput { input, .. },
                        ..
                    } => {
                        // Handle keyboard shortcuts and text-input editing keys.
                        use winit::event::VirtualKeyCode;
                        if let Some(keycode) = input.virtual_keycode
                            && input.state == ElementState::Pressed
                        {
                            match keycode {
                                VirtualKeyCode::R => {
                                    // Trigger reload (app will exit, dev server will restart it)
                                    velox_core::lifecycle::run_all_destroy_hooks();
                                    *control_flow = ControlFlow::Exit;
                                }
                                VirtualKeyCode::Q => {
                                    velox_core::lifecycle::run_all_destroy_hooks();
                                    *control_flow = ControlFlow::Exit;
                                }
                                _ => {
                                    // Every remaining editing key routes through the
                                    // one shared call, so the HMR loop cannot drift
                                    // from this one again.
                                    if let Some(action) =
                                        crate::edit_action_for_key(keycode, shift_held)
                                    {
                                        let changed = crate::apply_edit_to_focused(
                                            &mut input_targets,
                                            action,
                                            &last_vnode,
                                            &mut on_event,
                                        );
                                        if changed {
                                            // Caret goes solid while you type.
                                            arm_blink_deadline(&mut blink_deadline);
                                            velox_core::lifecycle::run_all_updated_hooks();
                                            if let Some(w) = window_opt.as_ref() {
                                                w.request_redraw();
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Event::UserEvent(()) => {
                        // Caret blink tick from `CaretBlinkTicker`. See that
                        // type for why a thread is involved and why it is not a
                        // shared-state race.
                        if crate::on_caret_blink_tick(&mut input_targets, &mut blink_deadline)
                            && let Some(w) = window_opt.as_ref()
                        {
                            w.request_redraw();
                        }
                    }
                    Event::WindowEvent {
                        event: WindowEvent::ReceivedCharacter(c),
                        ..
                    } => {
                        // Printable characters go to the focused text input.
                        if !c.is_control()
                            && c != '\u{7f}'
                            && crate::apply_edit_to_focused(
                                &mut input_targets,
                                crate::events::EditAction::Insert(c),
                                &last_vnode,
                                &mut on_event,
                            )
                        {
                            // Caret goes solid while you type.
                            arm_blink_deadline(&mut blink_deadline);
                            velox_core::lifecycle::run_all_updated_hooks();
                            if let Some(w) = window_opt.as_ref() {
                                w.request_redraw();
                            }
                        }
                    }
                    Event::WindowEvent {
                        event: WindowEvent::MouseWheel { delta, .. },
                        ..
                    } => {
                        // Scrollable overflow: deepest scrollable under the cursor,
                        // clamped via ScrollState. Winit reports positive y for
                        // wheel-up, so negate for the natural direction (wheel-down
                        // increases the offset and reveals content below). One
                        // line ~= 40 logical px; PixelDelta is physical.
                        let delta_y: f32 = match delta {
                            winit::event::MouseScrollDelta::LineDelta(_, y) => -y * 40.0,
                            winit::event::MouseScrollDelta::PixelDelta(pos) => {
                                -(pos.y as f32) / scale_factor
                            }
                        };
                        if let Some(layout) = last_layout.as_ref()
                            && crate::events::apply_wheel_scroll(
                                layout,
                                mouse_pos.0,
                                mouse_pos.1,
                                &mut scroll_offsets,
                                delta_y,
                            )
                            && let Some(w) = window_opt.as_ref()
                        {
                            w.request_redraw();
                        }
                    }
                    Event::RedrawRequested(_) => {
                        // R-L3: materialize any coalesced resize exactly once per frame.
                        let mut committed_resize = false;
                        if let Some((pw, ph)) = resize_state.take_pending() {
                            // `ResizeState::take_pending` is the production `pending_resize.take()` path.
                            match renderer.resize(pw as i32, ph as i32) {
                                Ok(()) => committed_resize = true,
                                Err(e) => {
                                    log::warn!("renderer resize failed ({}x{}): {}", pw, ph, e);
                                }
                            }
                            if let Some(presenter) = presenter.as_mut()
                                && let Err(e) = presenter.resize(pw, ph)
                            {
                                log::warn!("presenter resize failed: {}", e);
                            }
                        }
                        // First mount: fire on_mounted once.
                        ensure_mounted(&mut did_mount);
                        // Render VNode -> Skia frame and present.
                        if let Some(s) = &mut renderer.surface {
                            s.set_scale_factor(scale_factor);
                            let (vw, vh) = resize_state.frame_logical_size(
                                s.width,
                                s.height,
                                scale_factor,
                                committed_resize,
                            );
                            let (vnode_raw, sheet) = make_view(vw, vh);
                            let mut next_id = 1u32;
                            let vnode_tagged = with_hover_ids(&vnode_raw, &mut next_id);
                            let vnode = crate::style_vnode_with_hover(
                                &vnode_tagged,
                                &sheet,
                                &|_tag, props| {
                                    props
                                        .attrs
                                        .get("data-hover-id")
                                        .and_then(|v| v.parse::<u32>().ok())
                                        .map(|id| Some(id) == hovered_id)
                                        .unwrap_or(false)
                                },
                            );
                            // Caret/selection/focus contract for the paint lane.
                            // After the style cascade, before layout: it adds no
                            // second cascade and no second rounding site.
                            let vnode = crate::inject_input_caret_attrs(
                                &vnode,
                                crate::events::focused_input_index(&input_targets)
                                    .and_then(|i| input_targets.get(i)),
                            );
                            last_vnode = Some(vnode.clone());
                            let mut layout =
                                velox_dom::layout::compute_layout(&vnode, vw as i32, vh as i32);
                            {
                                let mut path = Vec::new();
                                crate::events::apply_scroll_offsets(
                                    &mut layout,
                                    &scroll_offsets,
                                    &mut path,
                                );
                            }
                            last_layout = Some(layout.clone());
                            recompute_targets(
                                &vnode,
                                &layout,
                                &mut click_targets,
                                &mut hover_targets,
                                &mut input_targets,
                            );
                            if let Err(e) = crate::skia_render::skia_impl::render_frame(
                                s, &vnode, &layout, &sheet,
                            ) {
                                log::error!("skia render error: {}", e);
                            }
                            if let Some(presenter) = presenter.as_mut()
                                && let Err(e) = presenter.present(s)
                            {
                                log::error!("skia present error: {}", e);
                            }
                        }
                    }
                    _ => {}
                }
            });
        }));
    } else {
        // Headless mode — no event loop, just run the initial render and return.
        log::info!("running in headless mode (no event loop)");
        velox_core::lifecycle::run_all_destroy_hooks();
    }
    Ok(())
}

/// What the caught window/loop construction yields: the event loop and its HMR
/// proxy (both survive even when the window does not), the window itself, its
/// physical size, and the DPI scale factor.
#[cfg(feature = "skia-native")]
type WindowBootstrap = (
    Option<winit::event_loop::EventLoop<HmrMessage>>,
    Option<winit::event_loop::EventLoopProxy<HmrMessage>>,
    Option<winit::window::Window>,
    winit::dpi::PhysicalSize<u32>,
    f32,
);

/// HMR variant — same viewport contract as [`run_window_vnode_skia`] (see its docs).
/// `make_view` is called with logical `(w, h)` on every frame/resize; templates use
/// `width:100%` / `min-height:100vh` so the viewport change is visibly reflected.
#[cfg(feature = "skia-native")]
pub fn run_window_vnode_skia_with_hmr<F, G, H>(
    title: &str,
    mut make_view: F,
    mut on_event: G,
    mut get_title: H,
    hmr_rx: std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<HmrMessage>>>,
) -> Result<(), String>
where
    F: FnMut(u32, u32) -> (velox_dom::VNode, Stylesheet) + 'static,
    G: FnMut(&str, Option<&str>) + 'static,
    H: FnMut() -> String + 'static,
{
    use winit::dpi::PhysicalSize;
    use winit::event::{ElementState, Event, MouseButton, StartCause, WindowEvent};
    use winit::event_loop::{ControlFlow, EventLoopBuilder};
    use winit::window::WindowBuilder;

    let headless_env_hmr = std::env::var("VELOX_HEADLESS").as_deref() == Ok("1")
        || !crate::presenter::is_compositor_available();

    // Prepare the winit backend: force X11 if Wayland socket is stale to
    // avoid winit's Wayland backend calling process::exit() on EPIPE.
    let _headless_check_hmr = crate::presenter::prepare_backend();

    // Try to create a winit event loop and window. In headless mode or when
    // no compositor is available, both can fail (winit may panic instead of
    // returning Err) — we catch this and proceed with headless rendering.
    // Use EventLoop<HmrMessage> so the HMR thread can forward messages
    // directly via EventLoopProxy::send_event(HmrMessage) without any
    // shared Mutex<Receiver>.
    let (event_loop_opt, proxy_opt, window, window_size, scale_factor): WindowBootstrap =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let event_loop = EventLoopBuilder::<HmrMessage>::with_user_event().build();
            let proxy = event_loop.create_proxy();
            let window_result = WindowBuilder::new()
                .with_title(title)
                .with_inner_size(PhysicalSize::new(800, 600))
                .build(&event_loop);
            match window_result {
                Ok(w) => {
                    let size = w.inner_size();
                    let sf = w.scale_factor() as f32;
                    (Some(event_loop), Some(proxy), Some(w), size, sf)
                }
                Err(e) => {
                    let msg = e.to_string();
                    let lower = msg.to_ascii_lowercase();
                    let is_display_err = lower.contains("broken pipe")
                        || lower.contains("os error 32")
                        || lower.contains("no compositor")
                        || lower.contains("no display server")
                        || lower.contains("failed to connect");
                    if headless_env_hmr || is_display_err {
                        log::warn!("window creation failed — continuing in headless mode: {msg}");
                        (
                            Some(event_loop),
                            Some(proxy),
                            None,
                            PhysicalSize::new(800, 600),
                            1.0,
                        )
                    } else {
                        panic!("failed to create window: {e}");
                    }
                }
            }
        }))
        .unwrap_or_else(|payload| {
            // Surface the panic payload (usually the compositor error, e.g.
            // broken pipe) instead of silently masking it — log::warn is
            // invisible without an initialized logger (CX-13 / F-23).
            eprintln!(
                "[velox] window/event loop creation panicked — continuing in headless mode: {}",
                panic_detail(payload)
            );
            (None, None, None, PhysicalSize::new(800, 600), 1.0)
        });

    // Spawn the HMR forwarding thread. It takes sole ownership of the
    // Receiver (moved out of the Arc<Mutex>) and forwards each HmrMessage
    // directly via EventLoopProxy::send_event(HmrMessage). No Mutex is
    // held across blocking recv(). Wrap in catch_unwind so HMR panics
    // never bring down the app (acceptance: catch_unwind around HMR loop).
    if let Some(proxy) = proxy_opt {
        match std::sync::Arc::try_unwrap(hmr_rx) {
            Ok(mutex) => {
                let rx = mutex.into_inner().expect("hmr mutex poisoned");
                std::thread::spawn(move || {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        while let Ok(msg) = rx.recv() {
                            if proxy.send_event(msg).is_err() {
                                break;
                            }
                        }
                    }));
                });
            }
            Err(shared) => {
                // Fallback: another Arc clone exists (unusual). Poll without
                // holding the lock across blocking recv.
                std::thread::spawn(move || {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        loop {
                            let res = {
                                let guard = shared.lock().expect("hmr mutex poisoned");
                                guard.try_recv()
                            };
                            match res {
                                Ok(msg) => {
                                    if proxy.send_event(msg).is_err() {
                                        break;
                                    }
                                }
                                Err(std::sync::mpsc::TryRecvError::Empty) => {
                                    std::thread::sleep(std::time::Duration::from_millis(16));
                                }
                                Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                            }
                        }
                    }));
                });
            }
        }
    }
    // NOTE: hmr_rx has been moved into the forwarding thread. The event loop
    // now receives HmrMessage via Event::UserEvent(msg) without any Mutex.

    let window_opt: Option<winit::window::Window> = window;
    let mut scale_factor = scale_factor;
    let mut renderer = match crate::skia_surface::SkiaSurface::new_raster(
        window_size.width as i32,
        window_size.height as i32,
    ) {
        Ok(surface) => skia_backend::SkiaRenderer {
            surface: Some(surface),
            vnode: None,
        },
        Err(e) => {
            return Err(format!("failed to create SkiaSurface: {e}"));
        }
    };
    let mut presenter: Option<crate::presenter::SoftbufferPresenter> = None;
    if let Some(w) = window_opt.as_ref() {
        // softbuffer::Context::new() can panic when the display server is
        // unreachable even though DISPLAY/WAYLAND_DISPLAY are set (broken pipe).
        // Catch such panics and degrade to headless rendering.
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::presenter::SoftbufferPresenter::new(w, window_size.width, window_size.height)
        })) {
            Ok(Ok(p)) => presenter = Some(p),
            Ok(Err(e)) => {
                if headless_env_hmr || !crate::presenter::is_compositor_available() {
                    log::warn!(
                        "softbuffer presenter unavailable — continuing in headless mode: {e}"
                    );
                } else {
                    return Err(format!("failed to create softbuffer presenter: {e}"));
                }
            }
            Err(payload) => {
                // Surface the panic payload (softbuffer panics with a
                // broken pipe when the display is unreachable) instead of
                // silently masking it (CX-13 / F-23).
                eprintln!(
                    "[velox] softbuffer presenter creation panicked — continuing in headless mode: {}",
                    panic_detail(payload)
                );
            }
        }
    }
    let mut mouse_pos = (0.0f32, 0.0f32);
    let mut hovered_id: Option<u32> = None;
    let mut click_targets: Vec<crate::events::ClickTarget> = Vec::new();
    let mut hover_targets: Vec<crate::events::HoverTarget> = Vec::new();
    let mut input_targets: Vec<crate::events::InputTarget> = Vec::new();
    // Renamed from `_last_vnode`: the HMR loop had NO `focused_input` at all, so
    // typing, Backspace and Return were dead under HMR. Keeping the last vnode
    // under a real name is what makes the ported input handling possible.
    let mut last_vnode: Option<velox_dom::VNode> = None;
    // Path (child source indices) to the focused text input, if any.
    // Mirrors the plain loop's state so the two loops stay behaviourally equal.
    let mut focused_input: Option<Vec<usize>> = None;
    // Instant before which the caret must stay solid regardless of tick timing.
    let mut blink_deadline: Option<std::time::Instant> = None;
    // Shift state, latched from `WindowEvent::ModifiersChanged` — see the plain
    // loop for why the per-key `modifiers` field is not used.
    let mut shift_held = false;
    let mut scroll_offsets_hmr: std::collections::HashMap<Vec<usize>, f32> =
        std::collections::HashMap::new();
    let mut last_layout_hmr: Option<velox_dom::layout::LayoutNode> = None;
    let _lifecycle_guard_hmr = LifecycleCleanupGuard;
    let mut did_mount_hmr = false;
    // R-L3: coalesce rapid resize drags in HMR loop too (only last size per frame).
    let mut resize_state = ResizeState::new();

    // Render first frame immediately before entering the event loop.
    // This ensures the window has content even on platforms where
    // request_redraw() from NewEvents(StartCause::Init) may not trigger
    // a RedrawRequested event (e.g. certain Wayland/X11 compositors).
    if let Some(s) = &mut renderer.surface {
        s.set_scale_factor(scale_factor);
        let (vw, vh) = resize_state.record_initial(s.width, s.height, scale_factor);
        let (vnode_raw, sheet) = make_view(vw, vh);
        let mut next_id = 1u32;
        let vnode_tagged = with_hover_ids(&vnode_raw, &mut next_id);
        let vnode = crate::style_vnode_with_hover(&vnode_tagged, &sheet, &|_tag, props| {
            props
                .attrs
                .get("data-hover-id")
                .and_then(|v| v.parse::<u32>().ok())
                .map(|id| Some(id) == hovered_id)
                .unwrap_or(false)
        });
        // First frame: no input can be focused yet, so every text input gets the
        // unfocused defaults.
        let vnode = crate::inject_input_caret_attrs(&vnode, None);
        last_vnode = Some(vnode.clone());
        let mut layout = velox_dom::layout::compute_layout(&vnode, vw as i32, vh as i32);
        {
            let mut path = Vec::new();
            crate::events::apply_scroll_offsets(&mut layout, &scroll_offsets_hmr, &mut path);
        }
        last_layout_hmr = Some(layout.clone());
        recompute_targets(
            &vnode,
            &layout,
            &mut click_targets,
            &mut hover_targets,
            &mut input_targets,
        );
        // Render and present the initial frame so the window has immediate content.
        if let Err(e) = crate::skia_render::skia_impl::render_frame(s, &vnode, &layout, &sheet) {
            log::error!("skia initial render error: {}", e);
        }
        if let Some(presenter) = presenter.as_mut()
            && let Err(e) = presenter.present(s)
        {
            log::error!("skia initial present error: {}", e);
        }
    }

    if let Some(event_loop) = event_loop_opt {
        // The event loop can panic if the display server becomes unreachable
        // (e.g. "Io error: Broken pipe") — catch that and degrade gracefully.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Caret blink clock for the HMR loop. `EventLoop::run` consumes
            // `self`, so the proxy has to be made before the call. This loop's
            // channel is typed `HmrMessage`, so the tick rides
            // `HmrMessage::KeepWindow` — see that arm for why.
            let _blink_ticker =
                crate::CaretBlinkTicker::start(event_loop.create_proxy(), HmrMessage::KeepWindow);
            event_loop.run(move |event, _, control_flow| {
                *control_flow = ControlFlow::Wait;
                match event {
                    Event::UserEvent(msg) => match msg {
                        HmrMessage::FullReload => {
                            velox_core::lifecycle::run_all_destroy_hooks();
                            *control_flow = ControlFlow::Exit;
                        }
                        HmrMessage::HotReload { module_path: _ } => {
                            // Rebuild view and refresh all hit-test targets including
                            // input_targets, then request a redraw so the new VNode
                            // is painted on the next frame.
                            if let Some(s) = renderer.surface.as_ref() {
                                let (vw, vh) = logical_size(s.width, s.height, scale_factor);
                                let (vnode_raw, _) = make_view(vw, vh);
                                if let Err(e) = HmrRenderer::hot_update(&mut renderer, vnode_raw) {
                                    log::error!("hot_update failed: {}", e);
                                }
                                // Recompute all targets from the updated view so
                                // input focus / hit-test stays in sync after HMR.
                                if let Some(s2) = renderer.surface.as_ref() {
                                    let (vw2, vh2) =
                                        logical_size(s2.width, s2.height, scale_factor);
                                    let (vnode2_raw, sheet2) = make_view(vw2, vh2);
                                    let mut nid = 1u32;
                                    let tagged = with_hover_ids(&vnode2_raw, &mut nid);
                                    let vnode2 = crate::style_vnode_with_hover(
                                        &tagged,
                                        &sheet2,
                                        &|_tag, props| {
                                            props
                                                .attrs
                                                .get("data-hover-id")
                                                .and_then(|v| v.parse::<u32>().ok())
                                                .map(|id| Some(id) == hovered_id)
                                                .unwrap_or(false)
                                        },
                                    );
                                    // Caret attrs survive a hot reload: the
                                    // focused path is matched by tree path, and
                                    // `recompute_targets` below re-applies the
                                    // edit state onto the new targets.
                                    let vnode2 = crate::inject_input_caret_attrs(
                                        &vnode2,
                                        crate::events::focused_input_index(&input_targets)
                                            .and_then(|i| input_targets.get(i)),
                                    );
                                    last_vnode = Some(vnode2.clone());
                                    let mut layout = velox_dom::layout::compute_layout(
                                        &vnode2, vw2 as i32, vh2 as i32,
                                    );
                                    {
                                        let mut path = Vec::new();
                                        crate::events::apply_scroll_offsets(
                                            &mut layout,
                                            &scroll_offsets_hmr,
                                            &mut path,
                                        );
                                    }
                                    last_layout_hmr = Some(layout.clone());
                                    recompute_targets(
                                        &vnode2,
                                        &layout,
                                        &mut click_targets,
                                        &mut hover_targets,
                                        &mut input_targets,
                                    );
                                }
                            }
                            if let Some(w) = window_opt.as_ref() {
                                w.request_redraw();
                            }
                        }
                        // `KeepWindow` is overloaded as the HMR loop's caret
                        // blink tick. The HMR channel is typed `HmrMessage`, and
                        // winit 0.28 has no timer API, so the ticker posts a
                        // no-op HMR message on the same channel rather than
                        // forcing the loop to be untyped. The dev server's real
                        // keep-alives land here too, and both are no-ops, so
                        // overloading costs nothing.
                        HmrMessage::KeepWindow => {
                            if crate::on_caret_blink_tick(&mut input_targets, &mut blink_deadline)
                                && let Some(w) = window_opt.as_ref()
                            {
                                w.request_redraw();
                            }
                        }
                    },
                    Event::NewEvents(StartCause::Init) => {
                        if let Some(w) = window_opt.as_ref() {
                            w.request_redraw();
                        }
                    }
                    Event::WindowEvent {
                        event: WindowEvent::CloseRequested,
                        ..
                    } => {
                        velox_core::lifecycle::run_all_destroy_hooks();
                        *control_flow = ControlFlow::Exit;
                    }
                    Event::WindowEvent {
                        event: WindowEvent::Resized(new_size),
                        ..
                    } => {
                        // R-L3: coalesce — defer surface recreation to RedrawRequested.
                        resize_state.queue((new_size.width, new_size.height));
                        // `ResizeState::queue` is the production `pending_resize = Some(...)` path.
                        if let Some(w) = window_opt.as_ref() {
                            w.request_redraw();
                        }
                    }
                    Event::WindowEvent {
                        event:
                            WindowEvent::ScaleFactorChanged {
                                scale_factor: new_scale,
                                new_inner_size,
                                ..
                            },
                        ..
                    } => {
                        let old_scale = scale_factor;
                        scale_factor = new_scale as f32;
                        if old_scale.is_finite()
                            && old_scale > 0.0
                            && scale_factor.is_finite()
                            && scale_factor > 0.0
                        {
                            mouse_pos.0 = mouse_pos.0 * old_scale / scale_factor;
                            mouse_pos.1 = mouse_pos.1 * old_scale / scale_factor;
                        }
                        // R-L3: coalesce renderer/presenter resize to RedrawRequested.
                        resize_state.queue((new_inner_size.width, new_inner_size.height));
                        if let Some(s) = &mut renderer.surface {
                            s.set_scale_factor(scale_factor);
                        }
                        // Hit-test/layout refresh deferred to RedrawRequested (single layout/frame, R-H5).
                        if let Some(w) = window_opt.as_ref() {
                            w.request_redraw();
                        }
                    }
                    Event::WindowEvent {
                        event: WindowEvent::CursorMoved { position, .. },
                        ..
                    } => {
                        mouse_pos = (
                            position.x as f32 / scale_factor,
                            position.y as f32 / scale_factor,
                        );
                        let now_hovered =
                            crate::events::hit_test_hover(&hover_targets, mouse_pos.0, mouse_pos.1);
                        if now_hovered != hovered_id {
                            hovered_id = now_hovered;
                            if let Some(w) = window_opt.as_ref() {
                                w.request_redraw();
                            }
                        }
                    }
                    Event::WindowEvent {
                        event:
                            WindowEvent::MouseInput {
                                state: ElementState::Pressed,
                                button: MouseButton::Left,
                                ..
                            },
                        ..
                    } => {
                        // Text-input focus: identical to the plain loop, and now
                        // actually present. This arm previously had NO focus
                        // handling at all, which is why the whole caret was
                        // unreachable under HMR.
                        crate::apply_click_focus(
                            &mut input_targets,
                            &last_vnode,
                            &mut focused_input,
                            mouse_pos.0,
                            mouse_pos.1,
                            scale_factor,
                        );
                        let mut handled_click = false;
                        if let Some((handler, payload_opt)) =
                            crate::events::hit_test_click(&click_targets, mouse_pos.0, mouse_pos.1)
                        {
                            let payload_owned =
                                payload_opt.map(|p| p.to_string()).unwrap_or_else(|| {
                                    format!("{{\"x\":{},\"y\":{}}}", mouse_pos.0, mouse_pos.1)
                                });
                            on_event(handler, Some(&payload_owned));
                            velox_core::lifecycle::run_all_updated_hooks();
                            if let Some(s) = &mut renderer.surface {
                                let (vw, vh) = logical_size(s.width, s.height, scale_factor);
                                let (vnode_raw, sheet) = make_view(vw, vh);
                                let mut next_id = 1u32;
                                let vnode_tagged = with_hover_ids(&vnode_raw, &mut next_id);
                                let vnode = crate::style_vnode_with_hover(
                                    &vnode_tagged,
                                    &sheet,
                                    &|_tag, props| {
                                        props
                                            .attrs
                                            .get("data-hover-id")
                                            .and_then(|v| v.parse::<u32>().ok())
                                            .map(|id| Some(id) == hovered_id)
                                            .unwrap_or(false)
                                    },
                                );
                                let vnode = crate::inject_input_caret_attrs(
                                    &vnode,
                                    crate::events::focused_input_index(&input_targets)
                                        .and_then(|i| input_targets.get(i)),
                                );
                                last_vnode = Some(vnode.clone());
                                let mut layout =
                                    velox_dom::layout::compute_layout(&vnode, vw as i32, vh as i32);
                                {
                                    let mut path = Vec::new();
                                    crate::events::apply_scroll_offsets(
                                        &mut layout,
                                        &scroll_offsets_hmr,
                                        &mut path,
                                    );
                                }
                                last_layout_hmr = Some(layout.clone());
                                recompute_targets(
                                    &vnode,
                                    &layout,
                                    &mut click_targets,
                                    &mut hover_targets,
                                    &mut input_targets,
                                );
                            }
                            handled_click = true;
                        }
                        if let Some(w) = window_opt.as_ref() {
                            if handled_click {
                                w.set_title(&get_title());
                            }
                            // HOISTED OUT of the `hit_test_click` arm, for the
                            // same reason as the plain loop: this arm had the
                            // identical nesting bug, so a click on empty space
                            // changed focus and never repainted.
                            w.request_redraw();
                        }
                    }
                    Event::WindowEvent {
                        event: WindowEvent::ModifiersChanged(mods),
                        ..
                    } => {
                        // Latch modifier state for Shift-extends-selection.
                        shift_held = mods.shift();
                    }
                    Event::WindowEvent {
                        event: WindowEvent::KeyboardInput { input, .. },
                        ..
                    } => {
                        // Keyboard shortcuts and text-input editing keys.
                        use winit::event::VirtualKeyCode;
                        if let Some(keycode) = input.virtual_keycode
                            && input.state == ElementState::Pressed
                        {
                            match keycode {
                                VirtualKeyCode::R => {
                                    // Trigger reload (app will exit, dev server will restart it)
                                    velox_core::lifecycle::run_all_destroy_hooks();
                                    *control_flow = ControlFlow::Exit;
                                }
                                VirtualKeyCode::Q => {
                                    velox_core::lifecycle::run_all_destroy_hooks();
                                    *control_flow = ControlFlow::Exit;
                                }
                                // Ported from the plain loop. This arm used to be
                                // a bare `_ => {}` stub with no `focused_input` in
                                // scope at all, so typing did nothing under HMR.
                                _ => {
                                    if let Some(action) =
                                        crate::edit_action_for_key(keycode, shift_held)
                                    {
                                        let changed = crate::apply_edit_to_focused(
                                            &mut input_targets,
                                            action,
                                            &last_vnode,
                                            &mut on_event,
                                        );
                                        if changed {
                                            // Caret goes solid while you type.
                                            crate::arm_blink_deadline(&mut blink_deadline);
                                            velox_core::lifecycle::run_all_updated_hooks();
                                            if let Some(w) = window_opt.as_ref() {
                                                w.request_redraw();
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Event::WindowEvent {
                        event: WindowEvent::ReceivedCharacter(c),
                        ..
                    } => {
                        // Printable characters go to the focused text input.
                        // Ported from the plain loop; there was no
                        // `ReceivedCharacter` arm anywhere in this loop, so
                        // typing inserted nothing under HMR.
                        if !c.is_control()
                            && c != '\u{7f}'
                            && crate::apply_edit_to_focused(
                                &mut input_targets,
                                crate::events::EditAction::Insert(c),
                                &last_vnode,
                                &mut on_event,
                            )
                        {
                            // Caret goes solid while you type.
                            crate::arm_blink_deadline(&mut blink_deadline);
                            velox_core::lifecycle::run_all_updated_hooks();
                            if let Some(w) = window_opt.as_ref() {
                                w.request_redraw();
                            }
                        }
                    }
                    Event::WindowEvent {
                        event: WindowEvent::MouseWheel { delta, .. },
                        ..
                    } => {
                        // Scrollable overflow: deepest scrollable under the cursor,
                        // clamped via ScrollState. Negate winit's y (positive = wheel
                        // up) for the natural direction; one line ~= 40 logical px.
                        let delta_y: f32 = match delta {
                            winit::event::MouseScrollDelta::LineDelta(_, y) => -y * 40.0,
                            winit::event::MouseScrollDelta::PixelDelta(pos) => {
                                -(pos.y as f32) / scale_factor
                            }
                        };
                        if let Some(layout) = last_layout_hmr.as_ref()
                            && crate::events::apply_wheel_scroll(
                                layout,
                                mouse_pos.0,
                                mouse_pos.1,
                                &mut scroll_offsets_hmr,
                                delta_y,
                            )
                            && let Some(w) = window_opt.as_ref()
                        {
                            w.request_redraw();
                        }
                    }
                    Event::RedrawRequested(_) => {
                        // R-L3: materialize any coalesced resize exactly once per frame.
                        let mut committed_resize = false;
                        if let Some((pw, ph)) = resize_state.take_pending() {
                            // `ResizeState::take_pending` is the production `pending_resize.take()` path.
                            match renderer.resize(pw as i32, ph as i32) {
                                Ok(()) => committed_resize = true,
                                Err(e) => {
                                    log::warn!("renderer resize failed ({}x{}): {}", pw, ph, e);
                                }
                            }
                            if let Some(presenter) = presenter.as_mut()
                                && let Err(e) = presenter.resize(pw, ph)
                            {
                                log::warn!("presenter resize failed: {}", e);
                            }
                        }
                        ensure_mounted(&mut did_mount_hmr);
                        // Render VNode -> Skia frame and present.
                        if let Some(s) = &mut renderer.surface {
                            s.set_scale_factor(scale_factor);
                            let (vw, vh) = resize_state.frame_logical_size(
                                s.width,
                                s.height,
                                scale_factor,
                                committed_resize,
                            );
                            let (vnode_raw, sheet) = make_view(vw, vh);
                            let mut next_id = 1u32;
                            let vnode_tagged = with_hover_ids(&vnode_raw, &mut next_id);
                            let vnode = crate::style_vnode_with_hover(
                                &vnode_tagged,
                                &sheet,
                                &|_tag, props| {
                                    props
                                        .attrs
                                        .get("data-hover-id")
                                        .and_then(|v| v.parse::<u32>().ok())
                                        .map(|id| Some(id) == hovered_id)
                                        .unwrap_or(false)
                                },
                            );
                            // Caret/selection/focus contract for the paint lane.
                            let vnode = crate::inject_input_caret_attrs(
                                &vnode,
                                crate::events::focused_input_index(&input_targets)
                                    .and_then(|i| input_targets.get(i)),
                            );
                            last_vnode = Some(vnode.clone());
                            let mut layout =
                                velox_dom::layout::compute_layout(&vnode, vw as i32, vh as i32);
                            {
                                let mut path = Vec::new();
                                crate::events::apply_scroll_offsets(
                                    &mut layout,
                                    &scroll_offsets_hmr,
                                    &mut path,
                                );
                            }
                            last_layout_hmr = Some(layout.clone());
                            recompute_targets(
                                &vnode,
                                &layout,
                                &mut click_targets,
                                &mut hover_targets,
                                &mut input_targets,
                            );
                            if let Err(e) = crate::skia_render::skia_impl::render_frame(
                                s, &vnode, &layout, &sheet,
                            ) {
                                log::error!("skia render error: {}", e);
                            }
                            if let Some(presenter) = presenter.as_mut()
                                && let Err(e) = presenter.present(s)
                            {
                                log::error!("softbuffer present error: {}", e);
                            }
                        }
                    }
                    _ => {}
                }
            });
        }));
    } else {
        // Headless mode — no event loop, just run the initial render and return.
        log::info!("running in headless mode (no event loop)");
        velox_core::lifecycle::run_all_destroy_hooks();
    }
    Ok(())
}

#[cfg(feature = "wgpu")]
fn load_system_font() -> Option<ab_glyph::FontArc> {
    use std::fs;
    const CANDIDATES: &[&str] = &[
        "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/google-noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/gnu-free/FreeSans.ttf",
    ];
    for p in CANDIDATES {
        if let Ok(bytes) = fs::read(p) {
            if let Ok(font) = ab_glyph::FontArc::try_from_vec(bytes) {
                return Some(font);
            }
        }
    }
    None
}

// Minimal window runner using winit when `wgpu` feature is enabled.
#[cfg(feature = "wgpu")]
pub fn run_window(title: &str) -> Result<(), String> {
    use wgpu::SurfaceError;
    use winit::dpi::PhysicalSize;
    use winit::event::{Event, WindowEvent};
    use winit::event_loop::{ControlFlow, EventLoop};
    use winit::window::WindowBuilder;

    println!("[window] launching '{}'", title);
    let event_loop = EventLoop::new();
    let window = match WindowBuilder::new()
        .with_title(title)
        .with_inner_size(PhysicalSize::new(800, 600))
        .build(&event_loop)
    {
        Ok(w) => {
            println!("[window] opened: {}", title);
            w
        }
        Err(e) => {
            return Err(format!("failed to create window: {e}"));
        }
    };

    // WGPU setup
    let instance = wgpu::Instance::default();
    let surface = unsafe { instance.create_surface(&window) }
        .map_err(|e| format!("failed to create surface: {e}"))?;
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
    }))
    .ok_or("no suitable GPU adapter found")?;
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("velox-device"),
            features: wgpu::Features::empty(),
            limits: wgpu::Limits::default(),
        },
        None,
    ))
    .map_err(|e| format!("failed to request device: {e}"))?;

    let mut size = window.inner_size();
    if size.width == 0 || size.height == 0 {
        size = PhysicalSize::new(800, 600);
        window.set_inner_size(size);
    }
    let surface_caps = surface.get_capabilities(&adapter);
    let format = surface_caps.formats[0];
    let mut config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: size.width,
        height: size.height,
        present_mode: surface_caps.present_modes[0],
        alpha_mode: surface_caps.alpha_modes[0],
        view_formats: vec![],
    };
    surface.configure(&device, &config);

    // Simple colored quad pipeline (two triangles) for a button placeholder
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        pos: [f32; 2],
        color: [f32; 3],
    }

    let shader_src = r#"
        struct VsOut {
            @builtin(position) position: vec4<f32>,
            @location(0) color: vec3<f32>,
        };

        @vertex
        fn vs(@location(0) pos: vec2<f32>, @location(1) color: vec3<f32>) -> VsOut {
            var out: VsOut;
            out.position = vec4<f32>(pos, 0.0, 1.0);
            out.color = color;
            return out;
        }

        @fragment
        fn fs(@location(0) color: vec3<f32>) -> @location(0) vec4<f32> {
            return vec4<f32>(color, 1.0);
        }
    "#;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("velox-shader"),
        source: wgpu::ShaderSource::Wgsl(shader_src.into()),
    });

    let vertex_layout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 8,
                shader_location: 1,
            },
        ],
    };

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("velox-pipeline-layout"),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("velox-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs",
            buffers: &[vertex_layout],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs",
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    // Button rect in pixel space; we convert to NDC in create_vertices
    let mut mouse_pos: (f32, f32) = (0.0, 0.0);
    let mut count: i32 = 0;
    let mut hovered = false;

    let mut vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("velox-vertices"),
        size: 6 * std::mem::size_of::<Vertex>() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let create_vertices = |w: u32, h: u32, hovered: bool| -> [Vertex; 6] {
        let bw = 200.0;
        let bh = 80.0;
        let cx = w as f32 / 2.0;
        let cy = h as f32 / 2.0;
        let x0 = cx - bw / 2.0;
        let y0 = cy - bh / 2.0;
        let x1 = cx + bw / 2.0;
        let y1 = cy + bh / 2.0;
        let to_ndc = |x: f32, y: f32| -> [f32; 2] {
            [(x / w as f32) * 2.0 - 1.0, 1.0 - (y / h as f32) * 2.0]
        };
        let (r, g, b) = if hovered {
            (0.25, 0.6, 0.9)
        } else {
            (0.2, 0.5, 0.8)
        };
        [
            Vertex {
                pos: to_ndc(x0, y0),
                color: [r, g, b],
            },
            Vertex {
                pos: to_ndc(x1, y0),
                color: [r, g, b],
            },
            Vertex {
                pos: to_ndc(x1, y1),
                color: [r, g, b],
            },
            Vertex {
                pos: to_ndc(x0, y0),
                color: [r, g, b],
            },
            Vertex {
                pos: to_ndc(x1, y1),
                color: [r, g, b],
            },
            Vertex {
                pos: to_ndc(x0, y1),
                color: [r, g, b],
            },
        ]
    };

    // initial vertices
    let verts = create_vertices(config.width, config.height, hovered);
    queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&verts));

    fn render(
        surface: &wgpu::Surface,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        config: &wgpu::SurfaceConfiguration,
        render_pipeline: &wgpu::RenderPipeline,
        vertex_buffer: &wgpu::Buffer,
    ) -> Result<(), SurfaceError> {
        let frame = surface.get_current_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("velox-encoder"),
        });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("velox-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.12,
                            g: 0.12,
                            b: 0.14,
                            a: 1.0,
                        }),
                        store: true,
                    },
                })],
                depth_stencil_attachment: None,
            });
            rpass.set_pipeline(render_pipeline);
            rpass.set_vertex_buffer(0, vertex_buffer.slice(..));
            rpass.draw(0..6, 0..1);
        }
        queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }

    let mut redraw_pending = true;
    // Move owned state into the event loop
    let mut config = config;
    let mut surface = surface;
    let mut device = device;
    let mut queue = queue;
    let mut vertex_buffer = vertex_buffer;
    let render_pipeline = render_pipeline;
    let title_owned = title.to_string();

    let _ = event_loop.run(move |event, _, control_flow| match event {
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => {
            *control_flow = ControlFlow::Exit;
        }
        Event::WindowEvent {
            event: WindowEvent::Resized(new_size),
            ..
        } => {
            config.width = new_size.width.max(1);
            config.height = new_size.height.max(1);
            surface.configure(&device, &config);
            let verts2 = create_vertices(config.width, config.height, hovered);
            queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&verts2));
            redraw_pending = true;
        }
        Event::WindowEvent {
            event: WindowEvent::CursorMoved { position, .. },
            ..
        } => {
            mouse_pos = (position.x as f32, position.y as f32);
            let bw = 200.0;
            let bh = 80.0;
            let cx = config.width as f32 / 2.0;
            let cy = config.height as f32 / 2.0;
            let x0 = cx - bw / 2.0;
            let y0 = cy - bh / 2.0;
            let x1 = cx + bw / 2.0;
            let y1 = cy + bh / 2.0;
            let now_hovered =
                mouse_pos.0 >= x0 && mouse_pos.0 <= x1 && mouse_pos.1 >= y0 && mouse_pos.1 <= y1;
            if now_hovered != hovered {
                hovered = now_hovered;
                let verts3 = create_vertices(config.width, config.height, hovered);
                queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&verts3));
            }
            window.request_redraw();
        }
        Event::WindowEvent {
            event:
                WindowEvent::MouseInput {
                    state: winit::event::ElementState::Pressed,
                    button: winit::event::MouseButton::Left,
                    ..
                },
            ..
        } => {
            let bw = 200.0;
            let bh = 80.0;
            let cx = config.width as f32 / 2.0;
            let cy = config.height as f32 / 2.0;
            let x0 = cx - bw / 2.0;
            let y0 = cy - bh / 2.0;
            let x1 = cx + bw / 2.0;
            let y1 = cy + bh / 2.0;
            if mouse_pos.0 >= x0 && mouse_pos.0 <= x1 && mouse_pos.1 >= y0 && mouse_pos.1 <= y1 {
                count += 1;
                window.set_title(&format!("{} — count {}", title_owned, count));
            }
        }
        Event::MainEventsCleared => {
            if redraw_pending {
                window.request_redraw();
            }
        }
        Event::RedrawRequested(_) => {
            match render(
                &surface,
                &device,
                &queue,
                &config,
                &render_pipeline,
                &vertex_buffer,
            ) {
                Ok(()) => {}
                Err(SurfaceError::Lost) => {
                    surface.configure(&device, &config);
                }
                Err(SurfaceError::OutOfMemory) => {
                    *control_flow = ControlFlow::Exit;
                }
                Err(_) => {}
            }
            redraw_pending = false;
        }
        _ => {}
    });
}

#[cfg(feature = "wgpu")]
pub fn run_window_counter<F>(title: &str, mut on_change: F) -> Result<(), String>
where
    F: FnMut(i32) + 'static,
{
    use winit::dpi::PhysicalSize;
    use winit::event::{ElementState, Event, MouseButton, WindowEvent};
    use winit::event_loop::{ControlFlow, EventLoop};
    use winit::window::WindowBuilder;

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title(title)
        .with_inner_size(PhysicalSize::new(800, 600))
        .build(&event_loop)
        .map_err(|e| format!("failed to create window: {e}"))?;
    let title_owned = title.to_string();

    let instance = wgpu::Instance::default();
    let surface = unsafe { instance.create_surface(&window) }
        .map_err(|e| format!("failed to create surface: {e}"))?;
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
    }))
    .ok_or("no suitable GPU adapter found")?;
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("velox-device"),
            features: wgpu::Features::empty(),
            limits: wgpu::Limits::default(),
        },
        None,
    ))
    .map_err(|e| format!("failed to request device: {e}"))?;
    let mut size = window.inner_size();
    if size.width == 0 || size.height == 0 {
        size = PhysicalSize::new(800, 600);
        window.set_inner_size(size);
    }
    let caps = surface.get_capabilities(&adapter);
    let format = caps.formats[0];
    let mut config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: size.width,
        height: size.height,
        present_mode: caps.present_modes[0],
        alpha_mode: caps.alpha_modes[0],
        view_formats: vec![],
    };
    surface.configure(&device, &config);

    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        pos: [f32; 2],
        color: [f32; 3],
    }
    let shader_src = r#"
        struct VsOut { @builtin(position) position: vec4<f32>, @location(0) color: vec3<f32>, };
        @vertex fn vs(@location(0) pos: vec2<f32>, @location(1) color: vec3<f32>) -> VsOut {
            var out: VsOut; out.position = vec4<f32>(pos, 0.0, 1.0); out.color = color; return out;
        }
        @fragment fn fs(@location(0) color: vec3<f32>) -> @location(0) vec4<f32> { return vec4<f32>(color, 1.0); }
    "#;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("velox-shader"),
        source: wgpu::ShaderSource::Wgsl(shader_src.into()),
    });
    let vlayout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 8,
                shader_location: 1,
            },
        ],
    };
    let pl_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("velox-pl"),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("velox-pipeline"),
        layout: Some(&pl_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs",
            buffers: &[vlayout],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs",
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });
    let mut vbuf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("velox-vbuf"),
        size: 6 * std::mem::size_of::<Vertex>() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let make_quad = |w: u32, h: u32, hovered: bool| -> [Vertex; 6] {
        let bw = 200.0;
        let bh = 80.0;
        let cx = w as f32 / 2.0;
        let cy = h as f32 / 2.0;
        let x0 = cx - bw / 2.0;
        let y0 = cy - bh / 2.0;
        let x1 = cx + bw / 2.0;
        let y1 = cy + bh / 2.0;
        let to_ndc = |x: f32, y: f32| [(x / w as f32) * 2.0 - 1.0, 1.0 - (y / h as f32) * 2.0];
        let (r, g, b) = if hovered {
            (0.25, 0.6, 0.9)
        } else {
            (0.2, 0.5, 0.8)
        };
        [
            Vertex {
                pos: to_ndc(x0, y0),
                color: [r, g, b],
            },
            Vertex {
                pos: to_ndc(x1, y0),
                color: [r, g, b],
            },
            Vertex {
                pos: to_ndc(x1, y1),
                color: [r, g, b],
            },
            Vertex {
                pos: to_ndc(x0, y0),
                color: [r, g, b],
            },
            Vertex {
                pos: to_ndc(x1, y1),
                color: [r, g, b],
            },
            Vertex {
                pos: to_ndc(x0, y1),
                color: [r, g, b],
            },
        ]
    };
    let mut hovered = false;
    queue.write_buffer(
        &vbuf,
        0,
        bytemuck::cast_slice(&make_quad(config.width, config.height, hovered)),
    );
    let mut mouse = (0.0f32, 0.0f32);
    let mut count = 0;
    on_change(count);

    let _ = event_loop.run(move |event, _, control_flow| match event {
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => {
            *control_flow = ControlFlow::Exit;
        }
        Event::WindowEvent {
            event: WindowEvent::Resized(sz),
            ..
        } => {
            config.width = sz.width.max(1);
            config.height = sz.height.max(1);
            surface.configure(&device, &config);
            queue.write_buffer(
                &vbuf,
                0,
                bytemuck::cast_slice(&make_quad(config.width, config.height, hovered)),
            );
            window.request_redraw();
        }
        Event::WindowEvent {
            event: WindowEvent::CursorMoved { position, .. },
            ..
        } => {
            mouse = (position.x as f32, position.y as f32);
            let bw = 200.0;
            let bh = 80.0;
            let cx = config.width as f32 / 2.0;
            let cy = config.height as f32 / 2.0;
            let x0 = cx - bw / 2.0;
            let y0 = cy - bh / 2.0;
            let x1 = cx + bw / 2.0;
            let y1 = cy + bh / 2.0;
            let h = mouse.0 >= x0 && mouse.0 <= x1 && mouse.1 >= y0 && mouse.1 <= y1;
            if h != hovered {
                hovered = h;
                queue.write_buffer(
                    &vbuf,
                    0,
                    bytemuck::cast_slice(&make_quad(config.width, config.height, hovered)),
                );
            }
            window.request_redraw();
        }
        Event::WindowEvent {
            event:
                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    button: MouseButton::Left,
                    ..
                },
            ..
        } => {
            let bw = 200.0;
            let bh = 80.0;
            let cx = config.width as f32 / 2.0;
            let cy = config.height as f32 / 2.0;
            let x0 = cx - bw / 2.0;
            let y0 = cy - bh / 2.0;
            let x1 = cx + bw / 2.0;
            let y1 = cy + bh / 2.0;
            if mouse.0 >= x0 && mouse.0 <= x1 && mouse.1 >= y0 && mouse.1 <= y1 {
                count += 1;
                window.set_title(&format!("{} — count {}", title_owned, count));
                on_change(count);
            }
        }
        Event::RedrawRequested(_) => {
            let frame = match surface.get_current_texture() {
                Ok(f) => f,
                Err(wgpu::SurfaceError::Lost) => {
                    surface.configure(&device, &config);
                    return;
                }
                Err(_) => return,
            };
            let view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("velox-enc"),
            });
            {
                let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("velox-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.12,
                                g: 0.12,
                                b: 0.14,
                                a: 1.0,
                            }),
                            store: true,
                        },
                    })],
                    depth_stencil_attachment: None,
                });
                rpass.set_pipeline(&pipeline);
                rpass.set_vertex_buffer(0, vbuf.slice(..));
                rpass.draw(0..6, 0..1);
            }
            queue.submit(Some(encoder.finish()));
            frame.present();
        }
        Event::MainEventsCleared => {
            window.request_redraw();
        }
        _ => {}
    });
}

#[cfg(feature = "wgpu")]
pub fn run_counter_window() -> Result<(), String> {
    use winit::event::{ElementState, Event, MouseButton, WindowEvent};
    use winit::event_loop::{ControlFlow, EventLoop};
    use winit::window::WindowBuilder;

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Velox - Count: 0 (click to increment)")
        .build(&event_loop)
        .map_err(|e| format!("failed to create window: {e}"))?;

    let mut count: i32 = 0;
    let mut update_title = move |c: i32| {
        window.set_title(&format!("Velox - Count: {} (click to increment)", c));
    };

    let _ = event_loop.run(move |event, _, control_flow| match event {
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => {
            *control_flow = ControlFlow::Exit;
        }
        Event::WindowEvent {
            event:
                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    button: MouseButton::Left,
                    ..
                },
            ..
        } => {
            count += 1;
            update_title(count);
        }
        Event::MainEventsCleared => {}
        _ => {}
    });
}

#[cfg(test)]
mod resize_state_tests {
    use super::ResizeState;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use velox_core::lifecycle::{
        cleanup_component, clear_current_component, generate_component_id, on_resize,
        set_current_component,
    };
    use velox_core::signal::Signal;

    fn resize_test_guard() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn register_recorder(calls: Rc<RefCell<Vec<(u32, u32)>>>) -> usize {
        let id = generate_component_id();
        set_current_component(id);
        on_resize(move |width, height| calls.borrow_mut().push((width, height)));
        clear_current_component();
        id
    }

    #[test]
    fn renderer_resize_state_coalesces_raw_events_and_dispatches_once() {
        let _guard = resize_test_guard();
        let calls = Rc::new(RefCell::new(Vec::new()));
        let id = register_recorder(calls.clone());
        let mut state = ResizeState::new();

        assert_eq!(state.record_initial(640, 480, 1.0), (640, 480));
        state.queue((800, 600));
        state.queue((900, 700));
        // Raw events only update the pending size; they do not dispatch hooks.
        assert!(calls.borrow().is_empty());
        assert_eq!(state.take_pending(), Some((900, 700)));
        assert_eq!(state.frame_logical_size(900, 700, 1.0, true), (900, 700));
        assert_eq!(state.take_pending(), None);
        assert_eq!(state.frame_logical_size(900, 700, 1.0, true), (900, 700));

        assert_eq!(&*calls.borrow(), &[(900, 700)]);
        cleanup_component(id);
    }

    #[test]
    fn renderer_resize_state_initial_logical_size_suppresses_notification() {
        let _guard = resize_test_guard();
        let calls = Rc::new(RefCell::new(Vec::new()));
        let id = register_recorder(calls.clone());
        let mut state = ResizeState::new();

        assert_eq!(state.record_initial(800, 600, 1.0), (800, 600));
        assert_eq!(state.frame_logical_size(800, 600, 1.0, true), (800, 600));
        assert!(calls.borrow().is_empty());
        cleanup_component(id);
    }

    #[test]
    fn renderer_resize_state_uses_viewport_logical_pixels_for_dpi() {
        let _guard = resize_test_guard();
        let calls = Rc::new(RefCell::new(Vec::new()));
        let id = register_recorder(calls.clone());
        let mut state = ResizeState::new();

        assert_eq!(state.record_initial(1600, 1200, 2.0), (800, 600));
        state.queue((2000, 1000));
        assert_eq!(state.take_pending(), Some((2000, 1000)));
        assert_eq!(state.frame_logical_size(2000, 1000, 2.0, true), (1000, 500));
        assert_eq!(&*calls.borrow(), &[(1000, 500)]);
        cleanup_component(id);
    }

    #[test]
    fn two_windows_with_equal_committed_sizes_both_dispatch() {
        let _guard = resize_test_guard();
        let calls = Rc::new(RefCell::new(Vec::new()));
        let id = register_recorder(calls.clone());
        let mut first_window = ResizeState::new();
        let mut second_window = ResizeState::new();

        // Each window owns its baseline; both legitimately commit 900x600.
        first_window.record_initial(800, 600, 1.0);
        second_window.record_initial(700, 600, 1.0);
        first_window.queue((900, 600));
        second_window.queue((900, 600));

        first_window.frame_logical_size(900, 600, 1.0, true);
        second_window.frame_logical_size(900, 600, 1.0, true);

        assert_eq!(&*calls.borrow(), &[(900, 600), (900, 600)]);
        cleanup_component(id);
    }

    #[test]
    fn renderer_resize_state_runs_all_hooks_and_allows_signal_mutation() {
        let _guard = resize_test_guard();
        let first_calls = Rc::new(RefCell::new(Vec::new()));
        let second_calls = Rc::new(RefCell::new(Vec::new()));
        let signal = Rc::new(Signal::new(0u32));
        let id = generate_component_id();

        set_current_component(id);
        {
            let calls = first_calls.clone();
            on_resize(move |width, height| calls.borrow_mut().push((width, height)));
        }
        {
            let calls = second_calls.clone();
            let signal = signal.clone();
            on_resize(move |width, height| {
                calls.borrow_mut().push((width, height));
                signal.set(width + height);
            });
        }
        clear_current_component();

        let mut state = ResizeState::new();
        state.record_initial(100, 100, 1.0);
        state.queue((200, 150));
        assert_eq!(state.take_pending(), Some((200, 150)));
        assert_eq!(state.frame_logical_size(200, 150, 1.0, true), (200, 150));

        assert_eq!(&*first_calls.borrow(), &[(200, 150)]);
        assert_eq!(&*second_calls.borrow(), &[(200, 150)]);
        assert_eq!(signal.get(), 350);
        cleanup_component(id);
    }

    #[test]
    fn cleanup_component_removes_only_its_resize_hook() {
        let _guard = resize_test_guard();
        let removed_calls = Rc::new(RefCell::new(Vec::new()));
        let retained_calls = Rc::new(RefCell::new(Vec::new()));
        let removed_id = register_recorder(removed_calls.clone());
        let retained_id = register_recorder(retained_calls.clone());

        cleanup_component(removed_id);
        let mut state = ResizeState::new();
        state.record_initial(100, 100, 1.0);
        state.frame_logical_size(200, 150, 1.0, true);

        assert!(removed_calls.borrow().is_empty());
        assert_eq!(&*retained_calls.borrow(), &[(200, 150)]);
        cleanup_component(retained_id);
    }
}
