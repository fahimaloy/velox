use std::collections::HashMap;

use velox_dom::VNode;

use crate::RenderTree;

type EventHandler = Box<dyn FnMut(Option<&str>)>;

pub struct EventRegistry {
    handlers: HashMap<String, EventHandler>,
}

impl Default for EventRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl EventRegistry {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }
    pub fn on<F: FnMut(Option<&str>) + 'static>(&mut self, name: impl Into<String>, f: F) {
        self.handlers.insert(name.into(), Box::new(f));
    }
    pub fn remove(&mut self, name: &str) {
        self.handlers.remove(name);
    }
    pub fn has(&self, name: &str) -> bool {
        self.handlers.contains_key(name)
    }
}

#[derive(Debug, Clone)]
pub struct ClickTarget {
    pub rect: velox_dom::layout::Rect,
    pub handler: String,
    pub payload: Option<String>,
    pub z_index: i32,
    pub order: i32,
    /// Intersected clip stack (`parent_clip ∩ node.clip`) at collection
    /// time. Hit testing rejects points outside this rect: content that
    /// protrudes past an `overflow:hidden`/scroll ancestor stays
    /// unclickable even though its rect contains the point.
    pub clip: Option<velox_dom::layout::Rect>,
    /// Stacking-context position used for stacking-aware hit ordering.
    pub sc: StackCtx,
}

#[derive(Debug, Clone)]
pub struct HoverTarget {
    pub rect: velox_dom::layout::Rect,
    pub id: u32,
    pub z_index: i32,
    pub order: i32,
    /// Intersected clip stack at collection time (see `ClickTarget::clip`).
    pub clip: Option<velox_dom::layout::Rect>,
    /// Stacking-context position used for stacking-aware hit ordering.
    pub sc: StackCtx,
}

/// A focusable text-input element. `path` records the child source-index path
/// from the VNode root so keyboard input can find the element in the freshly
/// rebuilt tree.
#[derive(Debug, Clone)]
pub struct InputTarget {
    pub rect: velox_dom::layout::Rect,
    pub path: Vec<usize>,
    pub z_index: i32,
    pub order: i32,
    /// Intersected clip stack at collection time (see `ClickTarget::clip`).
    pub clip: Option<velox_dom::layout::Rect>,
    /// Stacking-context position used for stacking-aware hit ordering.
    pub sc: StackCtx,
    /// Whether this input currently holds keyboard focus. Drives the focus ring
    /// and gates caret painting; exported to paint as the `focused` vnode attr.
    pub focused: bool,
    /// Caret position as a **char** index into the value string (never a byte
    /// index). Exported to paint as the `caret` vnode attr.
    pub cursor: usize,
    /// Selection anchor as a **char** index; `None` means the selection is
    /// collapsed (a plain caret). The selection spans
    /// `min(anchor, cursor) .. max(anchor, cursor)` — see [`InputTarget::selection`].
    pub anchor: Option<usize>,
    /// Caret blink phase. `true` = the caret bar is visible this frame. Reset to
    /// `true` on focus gain and after any editing key (the caret goes solid
    /// while you type), then flipped on the blink cadence. Exported to paint as
    /// the `caret_blink` vnode attr.
    pub blink_on: bool,
}

impl InputTarget {
    /// The selected char range as a sorted `(start, end)` pair, or `None` when
    /// the selection is collapsed (no `anchor`, or `anchor == cursor`).
    pub fn selection(&self) -> Option<(usize, usize)> {
        let anchor = self.anchor?;
        if anchor == self.cursor {
            return None;
        }
        Some(if anchor < self.cursor {
            (anchor, self.cursor)
        } else {
            (self.cursor, anchor)
        })
    }
}

/// Stacking-context position of a node, threaded through the `collect_*`
/// walk and stored on hit-test targets. Stacking contexts form atomic
/// groups, so a descendant compares against outside targets by its
/// outermost group's z-index, not its own:
///
/// - `group_z`: z-index of the outermost stacking-context ancestor-or-self
///   (0 while no stacking context has been entered — the root flow).
/// - `depth`: number of stacking-context ancestors-or-self; nested contexts
///   paint above their parents' backgrounds and in-flow siblings.
///
/// Hit testing sorts by `(group_z, depth, z_index, order)` descending:
/// stacking-context group first, depth before the z-index fallback, and
/// source order as the final tiebreaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackCtx {
    pub group_z: i32,
    pub depth: usize,
}

impl StackCtx {
    /// Position of the tree root (no enclosing stacking context).
    pub const ROOT: Self = Self {
        group_z: 0,
        depth: 0,
    };

    /// Position of `node` when entered from `self` (its parent context).
    fn enter(&self, node: &velox_dom::layout::LayoutNode) -> Self {
        let creates = node.stacking_context;
        Self {
            group_z: if self.depth == 0 && creates {
                node.z_index
            } else {
                self.group_z
            },
            depth: self.depth + usize::from(creates),
        }
    }
}

pub fn is_hoverable(tag: &str, props: &velox_dom::Props) -> bool {
    if props.attrs.contains_key("on:click") || tag == "button" {
        return true;
    }
    props
        .attrs
        .get("class")
        .map(|s| s.split_whitespace().any(|c| c == "btn"))
        .unwrap_or(false)
}

/// Compute the intersection of two rectangles.
/// Returns `Some(Rect)` if they intersect, `None` if they don't.
fn intersect(
    a: velox_dom::layout::Rect,
    b: velox_dom::layout::Rect,
) -> Option<velox_dom::layout::Rect> {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.w).min(b.x + b.w);
    let y1 = (a.y + a.h).min(b.y + b.h);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some(velox_dom::layout::Rect {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    })
}

/// Advance the intersected clip stack one level: `parent_clip ∩ node.clip`.
/// Returns `Some(next)` where `next` is the intersected clip (`None` when
/// nothing clips this node), or `None` when the two clips are disjoint — the
/// node and its subtree are fully clipped away and can never be hit, so
/// callers must prune them instead of treating them as unclipped.
fn clip_stack_next(
    parent: Option<velox_dom::layout::Rect>,
    node: &velox_dom::layout::LayoutNode,
) -> Option<Option<velox_dom::layout::Rect>> {
    match (parent, node.clip) {
        (Some(c), Some(lc)) => intersect(c, lc).map(Some),
        (None, Some(lc)) => Some(Some(lc)),
        (Some(c), None) => Some(Some(c)),
        (None, None) => Some(None),
    }
}

pub fn collect_click_targets(
    vnode: &VNode,
    layout: &velox_dom::layout::LayoutNode,
    clip: Option<velox_dom::layout::Rect>,
    sc: StackCtx,
    order: &mut i32,
    out: &mut Vec<ClickTarget>,
) {
    let sc = sc.enter(layout);
    let Some(next_clip) = clip_stack_next(clip, layout) else {
        return;
    };
    match vnode {
        VNode::Text(_) => {}
        VNode::Element {
            props, children, ..
        } => {
            if let Some(handler) = props.attrs.get("on:click").cloned() {
                let payload = props.attrs.get("on:click-payload").cloned();
                if next_clip
                    .map(|c| rects_intersect(layout.rect, c))
                    .unwrap_or(true)
                {
                    let ord = *order;
                    *order += 1;
                    out.push(ClickTarget {
                        rect: layout.rect,
                        handler,
                        payload,
                        z_index: layout.z_index,
                        order: ord,
                        clip: next_clip,
                        sc,
                    });
                }
            }
            let mut ordered: Vec<(i32, usize)> = layout
                .children
                .iter()
                .enumerate()
                .filter_map(|(i, ln)| {
                    if ln.display_none {
                        None
                    } else {
                        Some((ln.z_index, i))
                    }
                })
                .collect();
            ordered.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
            for (_, idx) in ordered {
                if let Some(child_layout) = layout.children.get(idx)
                    && let Some(src_idx) = child_layout.source_index
                    && let Some(child) = children.get(src_idx)
                {
                    collect_click_targets(child, child_layout, next_clip, sc, order, out);
                }
            }
        }
    }
}

pub fn collect_hover_targets(
    vnode: &VNode,
    layout: &velox_dom::layout::LayoutNode,
    clip: Option<velox_dom::layout::Rect>,
    sc: StackCtx,
    order: &mut i32,
    out: &mut Vec<HoverTarget>,
) {
    let sc = sc.enter(layout);
    let Some(next_clip) = clip_stack_next(clip, layout) else {
        return;
    };
    match vnode {
        VNode::Text(_) => {}
        VNode::Element {
            tag,
            props,
            children,
            ..
        } => {
            if is_hoverable(tag, props) {
                let id = props
                    .attrs
                    .get("data-hover-id")
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(0);
                if next_clip
                    .map(|c| rects_intersect(layout.rect, c))
                    .unwrap_or(true)
                {
                    let ord = *order;
                    *order += 1;
                    out.push(HoverTarget {
                        rect: layout.rect,
                        id,
                        z_index: layout.z_index,
                        order: ord,
                        clip: next_clip,
                        sc,
                    });
                }
            }
            let mut ordered: Vec<(i32, usize)> = layout
                .children
                .iter()
                .enumerate()
                .filter_map(|(i, ln)| {
                    if ln.display_none {
                        None
                    } else {
                        Some((ln.z_index, i))
                    }
                })
                .collect();
            ordered.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
            for (_, idx) in ordered {
                if let Some(child_layout) = layout.children.get(idx)
                    && let Some(src_idx) = child_layout.source_index
                    && let Some(child) = children.get(src_idx)
                {
                    collect_hover_targets(child, child_layout, next_clip, sc, order, out);
                }
            }
        }
    }
}

fn rects_intersect(a: velox_dom::layout::Rect, b: velox_dom::layout::Rect) -> bool {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.w).min(b.x + b.w);
    let y1 = (a.y + a.h).min(b.y + b.h);
    x1 > x0 && y1 > y0
}

/// Inclusive point-in-rect test on a layout-domain (integer logical px) rect.
fn rect_contains_point(r: velox_dom::layout::Rect, x: f32, y: f32) -> bool {
    let x0 = r.x as f32;
    let y0 = r.y as f32;
    let x1 = (r.x + r.w) as f32;
    let y1 = (r.y + r.h) as f32;
    x >= x0 && x <= x1 && y >= y0 && y <= y1
}

/// Stacking-aware topmost-first ordering for hit testing: sort descending by
/// stacking-context group z-index, then context depth, then the target's
/// own z-index (the fallback within the same group/depth), then collection
/// order (source-order tiebreak).
fn stack_order_desc<T>(
    targets: &[T],
    z_index: impl Fn(&T) -> i32,
    order: impl Fn(&T) -> i32,
    sc: impl Fn(&T) -> StackCtx,
) -> Vec<usize> {
    let mut ordered: Vec<(i32, usize, i32, i32, usize)> = targets
        .iter()
        .enumerate()
        .map(|(i, t)| (sc(t).group_z, sc(t).depth, z_index(t), order(t), i))
        .collect();
    ordered.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(b.1.cmp(&a.1))
            .then(b.2.cmp(&a.2))
            .then(b.3.cmp(&a.3))
    });
    ordered.into_iter().map(|(_, _, _, _, i)| i).collect()
}

pub fn hit_test_click(targets: &[ClickTarget], x: f32, y: f32) -> Option<(&str, Option<&str>)> {
    for idx in stack_order_desc(targets, |t| t.z_index, |t| t.order, |t| t.sc) {
        let target = &targets[idx];
        // Reject points outside the target's intersected clip stack: a
        // protrusion clipped away by an overflow/scroll ancestor is not
        // clickable even though the target rect contains the point.
        if let Some(c) = target.clip
            && !rect_contains_point(c, x, y)
        {
            continue;
        }
        if rect_contains_point(target.rect, x, y) {
            return Some((target.handler.as_str(), target.payload.as_deref()));
        }
    }
    None
}

/// Is this element a single-line text input that can take keyboard focus and a
/// caret?
///
/// Single source of truth for "is this a text input". `collect_input_targets`
/// uses it to decide what to make focusable, and the renderer's caret-attribute
/// injection uses it to decide which elements carry the `caret` / `caret_blink`
/// / `sel_start` / `sel_end` / `focused` contract. If these two disagreed, a
/// focusable input could be missing the attributes paint needs, or an element
/// with no target could be handed attrs that drive a phantom caret.
///
/// `type` is case-insensitive per the HTML spec, and a missing or blank `type`
/// defaults to `text`; anything that is not a single-line text type (`email`,
/// `number`, `password`, `checkbox`, …) is excluded.
pub fn is_text_input(tag: &str, props: &velox_dom::Props) -> bool {
    if tag != "input" {
        return false;
    }
    match props.attrs.get("type") {
        None => true,
        Some(t) => {
            let t = t.trim();
            t.is_empty() || t.eq_ignore_ascii_case("text")
        }
    }
}

/// Collect focusable text-input elements (`<input type="text">`) with their
/// tree paths. Used to route keyboard input to the focused field.
pub fn collect_input_targets(
    vnode: &VNode,
    layout: &velox_dom::layout::LayoutNode,
    clip: Option<velox_dom::layout::Rect>,
    sc: StackCtx,
    path: &mut Vec<usize>,
    order: &mut i32,
    out: &mut Vec<InputTarget>,
) {
    let sc = sc.enter(layout);
    let Some(next_clip) = clip_stack_next(clip, layout) else {
        return;
    };
    match vnode {
        VNode::Text(_) => {}
        VNode::Element {
            tag,
            props,
            children,
            ..
        } => {
            if is_text_input(tag, props)
                && next_clip
                    .map(|c| rects_intersect(layout.rect, c))
                    .unwrap_or(true)
            {
                let ord = *order;
                *order += 1;
                out.push(InputTarget {
                    rect: layout.rect,
                    path: path.clone(),
                    z_index: layout.z_index,
                    order: ord,
                    clip: next_clip,
                    sc,
                    // Edit state is owned by the caller and re-applied by
                    // `preserve_input_state` after every rebuild, so a fresh
                    // collection always starts unfocused with a collapsed
                    // caret at index 0 and the caret visible.
                    focused: false,
                    cursor: 0,
                    anchor: None,
                    blink_on: true,
                });
            }
            let mut ordered: Vec<(i32, usize)> = layout
                .children
                .iter()
                .enumerate()
                .filter_map(|(i, ln)| {
                    if ln.display_none {
                        None
                    } else {
                        Some((ln.z_index, i))
                    }
                })
                .collect();
            ordered.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
            for (_, idx) in ordered {
                if let Some(child_layout) = layout.children.get(idx)
                    && let Some(src_idx) = child_layout.source_index
                    && let Some(child) = children.get(src_idx)
                {
                    path.push(src_idx);
                    collect_input_targets(child, child_layout, next_clip, sc, path, order, out);
                    path.pop();
                }
            }
        }
    }
}

/// Return the topmost text-input target under a point, if any.
pub fn hit_test_input(targets: &[InputTarget], x: f32, y: f32) -> Option<&InputTarget> {
    hit_test_input_index(targets, x, y).map(|i| &targets[i])
}

/// Index-returning form of [`hit_test_input`], for callers that need to
/// *mutate* the hit target (focus/caret edits) — `hit_test_input` borrows the
/// slice immutably, which would conflict with `&mut input_targets`.
pub fn hit_test_input_index(targets: &[InputTarget], x: f32, y: f32) -> Option<usize> {
    for idx in stack_order_desc(targets, |t| t.z_index, |t| t.order, |t| t.sc) {
        let target = &targets[idx];
        // Points outside the intersected clip stack cannot focus the input.
        if let Some(c) = target.clip
            && !rect_contains_point(c, x, y)
        {
            continue;
        }
        if rect_contains_point(target.rect, x, y) {
            return Some(idx);
        }
    }
    None
}

/// Index of the focused input, i.e. the one whose `focused` flag is set.
pub fn focused_input_index(targets: &[InputTarget]) -> Option<usize> {
    targets.iter().position(|t| t.focused)
}

/// Whether *any* text input currently holds keyboard focus.
///
/// This is the gate on app-level single-key shortcuts — `r` to reload, `q` to
/// quit. Those keys are text while a field is focused, so an unguarded shortcut
/// means typing "r" into a search box exits the process. The loops must ask this
/// before acting on one, and a shortcut that is withheld here must fall through
/// to the editor so the character still reaches the field.
///
/// Note this is deliberately *not* `focused_input_index(..).is_some()` in the
/// loops: the question "is the user typing?" and the question "which field is
/// focused?" have different answers the moment the target vector is rebuilt
/// between two events, and the loops should not be able to drift into asking
/// the wrong one.
pub fn any_input_focused(targets: &[InputTarget]) -> bool {
    targets.iter().any(|t| t.focused)
}

/// The structural path of the focused input, if any.
///
/// `InputTarget::focused` is the single source of truth for focus; this is a
/// derived read of it in the shape both event loops keep a local of, so a caller
/// that wants "the path of the focused input" reads it here instead of
/// re-deriving it from an index.
pub fn focused_input_path(targets: &[InputTarget]) -> Option<Vec<usize>> {
    focused_input_index(targets).map(|i| targets[i].path.clone())
}

/// Move keyboard focus to `targets[index]`, blurring whichever input held it.
///
/// Returns `true` when focus actually moved, so the caller can decide whether to
/// repaint.
///
/// Focus is single-valued by construction: every other target's flag is cleared,
/// so [`focused_input_index`] can never report a stale first-of-two. Two targets
/// flagged at once is not a state the editor can represent — it would send
/// keystrokes to whichever one happens to come first in the vector.
///
/// An out-of-range `index` is a no-op returning `false`, not a panic: the index
/// comes from hit testing or from a caller holding an index into a vector it
/// built itself, and a target list rebuilt underneath it is an ordinary event
/// rather than a bug worth taking the window down for.
pub fn focus_input(targets: &mut [InputTarget], index: usize) -> bool {
    if index >= targets.len() {
        return false;
    }
    let mut changed = false;
    for (i, t) in targets.iter_mut().enumerate() {
        let want = i == index;
        if t.focused != want {
            t.focused = want;
            changed = true;
        }
    }
    changed
}

/// Index of the input target whose structural `path` equals `path`.
///
/// This is the bridge from a *stable* name to the `usize` [`focus_input`]
/// speaks. The index into `targets` is a paint-order position the author cannot
/// see and cannot keep: insert an input above theirs, or reorder the list they
/// live in, and it shifts. The structural path is derived from the tree, so it
/// survives both — which is why authored focus bindings resolve through a
/// `data-focus-id` to a path, and from there through this function to an index.
///
/// Returns the first match in vector order. Two targets cannot normally share a
/// path (`collect_input_targets` emits one per text input node), so "first" is
/// only a determinism guarantee, not a preference.
pub fn input_index_by_path(targets: &[InputTarget], path: &[usize]) -> Option<usize> {
    targets.iter().position(|t| t.path == path)
}

/// Structural path of the first element declaring `data-focus-id="id"`.
///
/// The other half of the stable-name bridge: [`input_index_by_path`] turns a
/// path into the `usize` the focus API speaks, this turns the name an author
/// writes into that path.
///
/// The two live here, ungated, rather than in the skia-gated loop code, because
/// neither needs a window or a backend — they are tree and vector lookups. That
/// is what lets a headless caller (an embedding host, or a test) resolve and
/// apply an authored focus binding without pulling in the native backend, and it
/// keeps "what does `data-focus-id` mean" in one file instead of splitting the
/// definition from the resolution.
///
/// Ids are the author's to keep unique. A duplicate resolves to the first
/// element in document order, which is total and deterministic — deliberately
/// not a panic and deliberately not "the last one wins", so a template with a
/// duplicated id behaves the same on every run.
pub fn find_focus_id_path(node: &VNode, path: &[usize], id: &str) -> Option<Vec<usize>> {
    let VNode::Element {
        props, children, ..
    } = node
    else {
        return None;
    };
    if props
        .attrs
        .get("data-focus-id")
        .is_some_and(|v| v.trim() == id)
    {
        return Some(path.to_vec());
    }
    for (i, child) in children.iter().enumerate() {
        let mut child_path = path.to_vec();
        child_path.push(i);
        if let Some(found) = find_focus_id_path(child, &child_path, id) {
            return Some(found);
        }
    }
    None
}

/// Everything one key press has to reach, gathered from the current tree in a
/// single pass.
///
/// A key press has no pointer, so unlike a click there is no hit test and no
/// single "target" element — what a key can mean is whatever the author attached
/// it to. Collecting both halves in one walk keeps them in the same document
/// order and, more importantly, keeps the two effects (focus grants and handler
/// calls) decided together: a key that both focuses a field and calls a handler
/// must not be able to do one without the other.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct KeydownPlan {
    /// `on:keydown` handler names, in document order.
    pub handlers: Vec<String>,
    /// Paths of elements whose `data-focus-on` names this key, in document
    /// order.
    pub focus_paths: Vec<Vec<usize>>,
}

/// Plan one key press, named `key_name`, against `vnode`.
///
/// Focus grants come from a *declarative* pair of attributes on the input
/// itself, not from the handler: `data-focus-on="F2"` says which key focuses
/// this field, `data-focus-id="composer"` says what it is called. The
/// alternative — an author handler calling into the renderer to move focus —
/// cannot be expressed from generated component code at all, because the
/// `State` a `.vx` compiles to has no handle on the event loop's target vector;
/// the key would reach the app and then have nowhere to go. Keeping the grant in
/// the tree makes "which key focuses this field" one declarative fact instead of
/// two halves that can disagree.
///
/// This is a *plan*, not an action: it reads the tree and reports. Applying it is
/// [`apply_keydown`], so the read half is testable — and reusable — without a
/// window.
pub fn plan_keydown(vnode: &VNode, key_name: &str) -> KeydownPlan {
    let mut plan = KeydownPlan::default();
    collect_keydown(vnode, &[], key_name, &mut plan);
    plan
}

fn collect_keydown(node: &VNode, path: &[usize], key_name: &str, plan: &mut KeydownPlan) {
    let VNode::Element {
        props, children, ..
    } = node
    else {
        return;
    };
    // A keydown listener has no per-element gating the way a click does — there
    // is no cursor to hit-test — so every `on:keydown` in the tree is invoked,
    // in document order. An author puts the binding in one place; a nested
    // binding is an additional observer, not a competing handler.
    if let Some(handler) = props.attrs.get("on:keydown")
        && !handler.is_empty()
    {
        plan.handlers.push(handler.clone());
    }
    // Case-insensitive so `data-focus-on="f2"` still works: this is compared
    // against the same string the handler compares against, and an author who
    // writes `key == "F2"` should not silently fail to focus because the
    // attribute was lower-cased.
    if props
        .attrs
        .get("data-focus-on")
        .is_some_and(|v| v.trim().eq_ignore_ascii_case(key_name))
    {
        plan.focus_paths.push(path.to_vec());
    }
    for (i, child) in children.iter().enumerate() {
        let mut child_path = path.to_vec();
        child_path.push(i);
        collect_keydown(child, &child_path, key_name, plan);
    }
}

/// Apply a [`KeydownPlan`]: grant every focus it asked for, then invoke every
/// handler with `key_name` as the payload.
///
/// Returns `true` when the caller must repaint — either focus moved or a
/// handler ran and may have changed state.
///
/// `value_len` supplies the current char length of the value at a path, so a
/// freshly focused field can put its caret at the end and the next keystroke
/// appends. It is a callback because the answer lives in the VNode and this
/// module deliberately knows nothing about views — the same reason
/// [`preserve_input_state`] takes one.
///
/// Handlers run for every plan regardless of what the focus half did, and this
/// function returns no instruction to consume the key. That is the whole
/// additive contract: the editing keys a `@keydown` binding can name
/// (`Backspace`, `Enter`, the arrows) are exactly the ones the editor also maps,
/// and both run on the same press. A dispatch that could swallow a keystroke
/// would make `@keydown` a footgun that breaks the text editor whenever one is
/// present, which is exactly why there is no `preventDefault` here and the
/// payload is a bare name.
///
/// The loop calls this before its reload/quit guard, so the binding an author
/// writes is the binding that runs for every key, and adding one never changes
/// which keys the app treats as text.
pub fn apply_keydown(
    plan: &KeydownPlan,
    key_name: &str,
    input_targets: &mut [InputTarget],
    focused_input: &mut Option<Vec<usize>>,
    value_len: &dyn Fn(&[usize]) -> usize,
    on_event: &mut impl FnMut(&str, Option<&str>),
) -> bool {
    let mut focus_changed = false;
    for path in &plan.focus_paths {
        // `focus_input` is the only writer of the `focused` flag and it clears
        // every other target's, so two grants in one press cannot both stick.
        // It reports `false` when nothing moved, which is what keeps a repeat of
        // the same key from yanking the caret back to the end of the value
        // mid-sentence.
        let Some(idx) = input_index_by_path(input_targets, path) else {
            // The element asked for focus but produced no input target: not a
            // text input, or laid out away. Not an error — a key that matches
            // nothing simply does nothing.
            continue;
        };
        if focus_input(input_targets, idx) {
            // Focus gain looks like a fresh click: caret to the end of the value,
            // no selection, caret solid.
            input_targets[idx].blink_on = true;
            input_targets[idx].anchor = None;
            input_targets[idx].cursor = value_len(path);
            // The mirror is the loops' index-free copy of focus. It has to move
            // here, in the same breath as the flag, because a caller that
            // updated one and not the other would send the next keystroke to a
            // field the two disagree about.
            *focused_input = Some(input_targets[idx].path.clone());
            focus_changed = true;
        }
    }
    for handler in &plan.handlers {
        on_event(handler, Some(key_name));
    }
    focus_changed || !plan.handlers.is_empty()
}

/// Drop keyboard focus from every input, discarding any selection with it.
///
/// Returns `true` when something changed — i.e. when the caller must repaint to
/// take the focus ring and the selection highlight off the screen.
///
/// The caret position is deliberately *kept*: losing focus is not the same as
/// forgetting where you were, so clicking back into a field restores the caret
/// rather than dumping the user at the end of the value. Esc is the deliberate
/// exception and clears everything — see [`EditAction::Blur`].
pub fn blur_focused_input(targets: &mut [InputTarget]) -> bool {
    let mut changed = false;
    for t in targets.iter_mut() {
        if t.focused || t.anchor.is_some() {
            t.focused = false;
            t.anchor = None;
            t.blink_on = true;
            changed = true;
        }
    }
    changed
}

/// Carry per-input edit state (focus / caret / anchor / blink phase) across a
/// target rebuild, matched by `path`.
///
/// `collect_input_targets` rebuilds the vector from scratch every frame and
/// cannot know about keystrokes, so without this the caret would snap back to 0
/// on every redraw. `value_len` supplies the current char length of the value at
/// a given path (0 when there is no such input) so caret and anchor can be
/// clamped: the app may rewrite the value between two frames (uppercasing,
/// truncation, async load), and a stale out-of-range index would otherwise be
/// carried forward. Clamping to a *char* length is what makes the later
/// byte-offset conversion safe.
pub fn preserve_input_state(
    targets: &mut [InputTarget],
    previous: &[InputTarget],
    value_len: &dyn Fn(&[usize]) -> usize,
) {
    for t in targets.iter_mut() {
        if let Some(prev) = previous.iter().find(|p| p.path == t.path) {
            let len = value_len(&t.path);
            t.focused = prev.focused;
            t.blink_on = prev.blink_on;
            t.cursor = prev.cursor.min(len);
            t.anchor = prev.anchor.map(|a| a.min(len));
        }
    }
}

/// Map a click x-position to a char index inside a single-line text input.
///
/// `rect` is the laid-out input box and `click_x` is in the same logical space.
/// `text_origin_x` is where the text run starts (the box's left content edge,
/// i.e. `rect.x + padding-left`), so the caller owns padding and this function
/// needs no style knowledge. `font_size` is a fallback advance used only when
/// `measure` reports a non-finite or negative width. `measure` receives a
/// candidate prefix (`&value[..byte_i]`, always on a char boundary) and returns
/// its advance width in logical px.
///
/// The scan is a **linear** walk over char boundaries, not a binary search:
/// `measure` is an arbitrary caller-supplied closure, and kerning or ligatures
/// make prefix widths non-monotonic in principle, which would make a binary
/// search land on a boundary that is not the nearest one to the click. This
/// implementation records the last boundary at or left of the click *and* the
/// first boundary to its right, then picks whichever is horizontally closer —
/// correct for any prefix-width sequence, monotonic or not.
pub fn click_to_char_index(
    value: &str,
    rect: velox_dom::layout::Rect,
    click_x: f32,
    text_origin_x: f32,
    font_size: f32,
    measure: &dyn Fn(&str) -> f32,
) -> usize {
    let char_count = value.chars().count();
    if char_count == 0 {
        return 0;
    }
    // Horizontal distance from the start of the text run, clamped at 0. The
    // origin is taken from `text_origin_x` but never left of the box's own left
    // edge, so a caller that forgets padding still gets sane results.
    let origin = text_origin_x.max(rect.x as f32);
    let local = (click_x - origin).max(0.0);

    // Fallback advance used only when the measurer is unusable.
    let fallback = |s: &str| font_size * 0.5 * s.chars().count() as f32;
    let width_of = |s: &str| {
        let w = measure(s);
        if w.is_finite() && w >= 0.0 {
            w
        } else {
            fallback(s)
        }
    };

    let mut best_left = 0usize;
    let mut left_x = 0.0f32;
    let mut best_right: Option<usize> = None;
    let mut right_x = 0.0f32;

    for (char_i, (byte_i, _)) in value.char_indices().enumerate() {
        let w = width_of(&value[..byte_i]);
        if w <= local {
            best_left = char_i;
            left_x = w;
        } else {
            best_right = Some(char_i);
            right_x = w;
            break;
        }
    }

    match best_right {
        // Click at or past the right edge of the text run: end of the value.
        None => char_count,
        Some(right) => {
            // Nearest boundary wins; ties go left.
            let d_left = (local - left_x).abs();
            let d_right = (right_x - local).abs();
            if d_left <= d_right { best_left } else { right }
        }
    }
}

/// Snap an arbitrary index down to the nearest `char` boundary of `value`,
/// clamped to `0..=value.len()`.
///
/// This is the single panic guard for char-index arithmetic: Rust string
/// slicing panics on a non-boundary index, and a caret index can arrive from
/// anywhere (a stale frame, a caller-supplied offset, a value the app rewrote
/// under us). Stepping *down* keeps the caret visually stable and, because
/// every caller only ever slices at a snapped index, cannot panic.
pub fn snap_to_char_boundary(value: &str, index: usize) -> usize {
    let mut i = index.min(value.len());
    while i > 0 && !value.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Byte offset of char index `char_i` in `value`, clamped to `value.len()`.
fn byte_of_char(value: &str, char_i: usize) -> usize {
    value
        .char_indices()
        .nth(char_i)
        .map(|(b, _)| b)
        .unwrap_or(value.len())
}

/// Delete the char range `[a, b)` (char indices, order-independent) from
/// `value`, returning the new string. Out-of-range and inverted input is
/// tolerated: the range is sorted and both ends snapped to char boundaries
/// before any slicing happens.
pub fn delete_char_range(value: &str, a: usize, b: usize) -> String {
    let (a, b) = if a <= b { (a, b) } else { (b, a) };
    let ba = snap_to_char_boundary(value, byte_of_char(value, a));
    let bb = snap_to_char_boundary(value, byte_of_char(value, b));
    let (ba, bb) = if ba <= bb { (ba, bb) } else { (bb, ba) };
    let mut out = String::with_capacity(value.len().saturating_sub(bb - ba));
    out.push_str(&value[..ba]);
    out.push_str(&value[bb..]);
    out
}

/// Insert `ch` at char index `char_i` of `value`, returning the new string.
pub fn insert_char_at(value: &str, char_i: usize, ch: char) -> String {
    let b = snap_to_char_boundary(value, byte_of_char(value, char_i));
    let mut out = String::with_capacity(value.len() + ch.len_utf8());
    out.push_str(&value[..b]);
    out.push(ch);
    out.push_str(&value[b..]);
    out
}

/// A single caret/selection/text editing intent, resolved against an
/// [`InputTarget`] by [`apply_edit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditAction {
    /// Move the caret one char left; with `shift`, extend the selection.
    MoveLeft { shift: bool },
    /// Move the caret one char right; with `shift`, extend the selection.
    MoveRight { shift: bool },
    /// Caret to the start of the value; with `shift`, extend the selection.
    Home { shift: bool },
    /// Caret to the end of the value; with `shift`, extend the selection.
    End { shift: bool },
    /// Delete the selection, else the char before the caret.
    Backspace,
    /// Delete the selection, else the char at the caret.
    Delete,
    /// Delete the selection, then insert `ch` at the (collapsed) caret.
    Insert(char),
    /// Delete the selection, then submit.
    Submit,
    /// Leave the field: drop keyboard focus, the selection and the caret.
    ///
    /// Esc is the one key that ends an editing session rather than editing the
    /// text, so it takes every piece of caret state with it. That is what
    /// distinguishes it from [`blur_focused_input`], which keeps the caret so
    /// clicking back into a field restores the position: Esc is an explicit
    /// "I am done here", a click on empty space is just a click.
    ///
    /// The field's value is untouched, so no `on:input` is dispatched.
    Blur,
}

/// Outcome of applying an [`EditAction`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EditResult {
    /// The new field value, present only when the text actually changed.
    pub value: Option<String>,
    /// True when the action should submit the field (Return).
    pub submit: bool,
    /// True when the field's focus, caret or selection changed. A pure caret
    /// move repaints without dispatching anything to the app; so does a blur,
    /// which changes what is painted (the focus ring and the selection
    /// highlight go away) without changing the text.
    pub moved: bool,
}

impl EditResult {
    /// Whether the caller must repaint (and run updated hooks) after this edit.
    pub fn needs_repaint(&self) -> bool {
        self.value.is_some() || self.submit || self.moved
    }
}

/// Apply an editing action to `target`, treating `value` as the field's current
/// text, and return what changed.
///
/// This is deliberately a pure state transition: the caller owns dispatching
/// `result.value` to the app's `on:input` handler and repainting. Both event
/// loops (plain and HMR) call this same function, which is what keeps them
/// from drifting apart again.
///
/// Panic safety: every index is snapped to a char boundary and every slice goes
/// through [`delete_char_range`] / [`insert_char_at`], which snap internally, so
/// no index derived from a caret position can slice a multi-byte character in
/// half. Caret positions coming from callers are also clamped to the value's
/// char length, so a stale or bogus index degrades to a clamped one rather than
/// panicking.
pub fn apply_edit(target: &mut InputTarget, value: &str, action: EditAction) -> EditResult {
    let len = value.chars().count();
    let mut cursor = target.cursor.min(len);
    let anchor = target.anchor.map(|a| a.min(len));
    let sel = match anchor {
        Some(a) if a != cursor => Some(if a < cursor { (a, cursor) } else { (cursor, a) }),
        _ => None,
    };
    let mut res = EditResult::default();
    // Any text/caret change makes the caret solid again; the blink tick resumes
    // from the caller's deadline.
    let mut touched = false;

    match action {
        EditAction::MoveLeft { shift } => {
            if cursor > 0 {
                let old = cursor;
                cursor -= 1;
                if shift {
                    target.anchor = Some(anchor.unwrap_or(old));
                } else {
                    target.anchor = None;
                }
                touched = true;
            }
        }
        EditAction::MoveRight { shift } => {
            if cursor < len {
                let old = cursor;
                cursor += 1;
                if shift {
                    target.anchor = Some(anchor.unwrap_or(old));
                } else {
                    target.anchor = None;
                }
                touched = true;
            }
        }
        EditAction::Home { shift } => {
            if shift {
                target.anchor = Some(anchor.unwrap_or(cursor));
            } else {
                target.anchor = None;
            }
            if cursor != 0 {
                cursor = 0;
                touched = true;
            } else {
                // Selection collapse onto 0 is still a visual change.
                res.moved = sel.is_some();
            }
        }
        EditAction::End { shift } => {
            if shift {
                target.anchor = Some(anchor.unwrap_or(cursor));
            } else {
                target.anchor = None;
            }
            if cursor != len {
                cursor = len;
                touched = true;
            } else {
                res.moved = sel.is_some();
            }
        }
        EditAction::Backspace => {
            if let Some((s, e)) = sel {
                res.value = Some(delete_char_range(value, s, e));
                cursor = s;
                target.anchor = None;
                touched = true;
            } else if cursor > 0 {
                res.value = Some(delete_char_range(value, cursor - 1, cursor));
                cursor -= 1;
                target.anchor = None;
                touched = true;
            }
        }
        EditAction::Delete => {
            if let Some((s, e)) = sel {
                res.value = Some(delete_char_range(value, s, e));
                cursor = s;
                target.anchor = None;
                touched = true;
            } else if cursor < len {
                res.value = Some(delete_char_range(value, cursor, cursor + 1));
                target.anchor = None;
                touched = true;
            }
        }
        EditAction::Insert(ch) => {
            if let Some((s, e)) = sel {
                let cleared = delete_char_range(value, s, e);
                res.value = Some(insert_char_at(&cleared, s, ch));
                cursor = s + 1;
            } else {
                res.value = Some(insert_char_at(value, cursor, ch));
                cursor += 1;
            }
            target.anchor = None;
            touched = true;
        }
        EditAction::Submit => {
            if let Some((s, e)) = sel {
                res.value = Some(delete_char_range(value, s, e));
                cursor = s;
                touched = true;
            }
            target.anchor = None;
            res.submit = true;
        }
        EditAction::Blur => {
            // Report a repaint only when there was something on screen to take
            // away: Esc with nothing focused must stay free, exactly like any
            // other keypress that changes nothing.
            res.moved = target.focused || sel.is_some();
            target.focused = false;
            target.anchor = None;
            cursor = 0;
        }
    }

    target.cursor = cursor;
    if touched {
        target.blink_on = true;
        res.moved = true;
    }
    res
}

pub fn hit_test_hover(targets: &[HoverTarget], x: f32, y: f32) -> Option<u32> {
    for idx in stack_order_desc(targets, |t| t.z_index, |t| t.order, |t| t.sc) {
        let target = &targets[idx];
        // Points outside the intersected clip stack do not hover the target.
        if let Some(c) = target.clip
            && !rect_contains_point(c, x, y)
        {
            continue;
        }
        if rect_contains_point(target.rect, x, y) {
            return Some(target.id);
        }
    }
    None
}

/// Find the deepest scrollable LayoutNode containing `point`.
/// Returns the path (source_index chain) to the deepest match.
pub fn hit_test_scrollable(
    layout: &velox_dom::layout::LayoutNode,
    x: f32,
    y: f32,
) -> Option<Vec<usize>> {
    fn dfs(
        node: &velox_dom::layout::LayoutNode,
        x: f32,
        y: f32,
        path: &mut Vec<usize>,
        best: &mut Option<(Vec<usize>, usize)>,
        depth: usize,
    ) {
        if node.scrollable && rect_contains_point(node.rect, x, y) {
            // Prefer deeper depth; at equal depth the first candidate found
            // (earliest source order) wins — the guard below keeps the
            // existing best on ties.
            let candidate = path.clone();
            match best {
                Some((_, best_depth)) if depth <= *best_depth => {}
                _ => *best = Some((candidate, depth)),
            }
        }
        for child in &node.children {
            if let Some(idx) = child.source_index {
                path.push(idx);
                dfs(child, x, y, path, best, depth + 1);
                path.pop();
            } else {
                dfs(child, x, y, path, best, depth + 1);
            }
        }
    }
    let mut best: Option<(Vec<usize>, usize)> = None;
    let mut path = Vec::new();
    dfs(layout, x, y, &mut path, &mut best, 0);
    best.map(|(p, _)| p)
}

/// Resolve a LayoutNode by its source_index path (as produced by
/// `hit_test_scrollable`). The empty path resolves to the root.
pub fn node_at_path<'a>(
    layout: &'a velox_dom::layout::LayoutNode,
    path: &[usize],
) -> Option<&'a velox_dom::layout::LayoutNode> {
    let mut node = layout;
    for &idx in path {
        node = node.children.iter().find(|c| c.source_index == Some(idx))?;
    }
    Some(node)
}

/// Shift a node and all its descendants vertically by `dy` (logical px).
/// Descendant clips move with their owners; the scroll container's own clip
/// is left untouched (callers only shift the container's children).
fn shift_subtree_y(node: &mut velox_dom::layout::LayoutNode, dy: i32) {
    node.rect.y += dy;
    if let Some(clip) = node.clip.as_mut() {
        clip.y += dy;
    }
    for child in &mut node.children {
        shift_subtree_y(child, dy);
    }
}

/// Apply stored scroll offsets (keyed by source_index path) into a freshly
/// computed layout tree. For each scrollable node with a stored offset the
/// children subtree is shifted so that `content_y_scrolled = content_y -
/// scroll_y` holds in the layout rects themselves — render, hit-testing and
/// click targets all consume the same shifted rects. Without a stored entry
/// the layout is untouched (synthetic scroll-top/scroll-left bake stays).
pub fn apply_scroll_offsets(
    layout: &mut velox_dom::layout::LayoutNode,
    offsets: &std::collections::HashMap<Vec<usize>, f32>,
    path: &mut Vec<usize>,
) {
    if layout.scrollable
        && let Some(&off) = offsets.get(path)
    {
        let max = (layout.max_scroll_y as f32).max(0.0);
        let target = off.clamp(0.0, max).round() as i32;
        let dy = layout.scroll_y - target;
        if dy != 0 {
            for child in &mut layout.children {
                shift_subtree_y(child, dy);
            }
        }
        layout.scroll_y = target;
    }
    for child in &mut layout.children {
        if let Some(idx) = child.source_index {
            path.push(idx);
            apply_scroll_offsets(child, offsets, path);
            path.pop();
        } else {
            apply_scroll_offsets(child, offsets, path);
        }
    }
}

/// Wheel handler: hit-test the deepest scrollable node under the cursor and
/// advance its stored offset by `delta_y` (positive = content moves up),
/// clamped to `[0, max_scroll_y]` via `ScrollState::on_wheel`. The first
/// wheel over a node seeds the offset from its synthetic scroll-top (the
/// deprecated style is thereby mapped into the ScrollState model). Returns
/// true when the offset changed and a redraw is needed.
pub fn apply_wheel_scroll(
    layout: &velox_dom::layout::LayoutNode,
    x: f32,
    y: f32,
    offsets: &mut std::collections::HashMap<Vec<usize>, f32>,
    delta_y: f32,
) -> bool {
    let Some(path) = hit_test_scrollable(layout, x, y) else {
        return false;
    };
    let Some(node) = node_at_path(layout, &path) else {
        return false;
    };
    // Seed from the node's current (possibly synthetic scroll-top) offset.
    let entry = offsets.entry(path).or_insert(node.scroll_y as f32);
    let before = *entry;
    let mut state = velox_dom::layout::ScrollState::new(node.max_scroll_y as f32);
    state.offset_y = before;
    state.on_wheel(delta_y);
    *entry = state.offset_y;
    state.offset_y != before
}

/// Dispatches an event by scanning the VNode tree for props of the form
/// `on:<event>` and invoking registered callbacks with the string value.
/// Also collects `on:<event>-payload` values and forwards them.
/// Returns the number of callbacks invoked.
pub fn dispatch(event: &str, tree: &RenderTree, registry: &mut EventRegistry) -> usize {
    let mut invoked = 0;
    let key = format!("on:{}", event);
    let payload_key = format!("on:{}-payload", event);
    fn walk(node: &VNode, key: &str, payload_key: &str, out: &mut Vec<(String, Option<String>)>) {
        match node {
            VNode::Text(_) => {}
            VNode::Element {
                props, children, ..
            } => {
                if let Some(v) = props.attrs.get(key) {
                    let payload = props.attrs.get(payload_key).cloned();
                    out.push((v.clone(), payload));
                }
                for c in children {
                    walk(c, key, payload_key, out);
                }
            }
        }
    }
    let mut targets = Vec::new();
    walk(&tree.root, &key, &payload_key, &mut targets);
    for (name, payload) in targets {
        if let Some(cb) = registry.handlers.get_mut(&name) {
            cb(payload.as_deref());
            invoked += 1;
        }
    }
    invoked
}

use std::time::{Duration, Instant};

/// Runtime helper to translate high-level input events to dispatcher calls.
pub struct Runtime {
    pub tree: RenderTree,
    pub registry: EventRegistry,
    last_click: Option<Instant>,
    hover_sent: bool,
}

impl Runtime {
    pub fn new(tree: RenderTree) -> Self {
        Self {
            tree,
            registry: EventRegistry::new(),
            last_click: None,
            hover_sent: false,
        }
    }

    /// Call on mouse left-button press; detects double-click within 400ms.
    pub fn mouse_click(&mut self) -> usize {
        let now = Instant::now();

        if let Some(prev) = self.last_click {
            if now.duration_since(prev) <= Duration::from_millis(400) {
                self.last_click = None;
                dispatch("dblclick", &self.tree, &mut self.registry)
            } else {
                self.last_click = Some(now);
                dispatch("click", &self.tree, &mut self.registry)
            }
        } else {
            self.last_click = Some(now);
            dispatch("click", &self.tree, &mut self.registry)
        }
    }

    /// Call on cursor moved; fires hover events on every movement.
    /// This enables continuous hover tracking for UI interactions.
    pub fn cursor_moved(&mut self) -> usize {
        self.hover_sent = true;
        dispatch("hover", &self.tree, &mut self.registry)
    }

    /// Reset hover state (useful for tests or leaving the window).
    pub fn reset_hover(&mut self) {
        self.hover_sent = false;
    }
}
