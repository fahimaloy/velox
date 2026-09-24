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
            let is_text_input =
                tag == "input" && props.attrs.get("type").map(|s| s == "text").unwrap_or(true);
            if is_text_input
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
    for idx in stack_order_desc(targets, |t| t.z_index, |t| t.order, |t| t.sc) {
        let target = &targets[idx];
        // Points outside the intersected clip stack cannot focus the input.
        if let Some(c) = target.clip
            && !rect_contains_point(c, x, y)
        {
            continue;
        }
        if rect_contains_point(target.rect, x, y) {
            return Some(target);
        }
    }
    None
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
