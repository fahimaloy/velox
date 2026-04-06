use std::collections::HashMap;

use velox_dom::VNode;

use crate::RenderTree;

pub struct EventRegistry {
    handlers: HashMap<String, Box<dyn FnMut()>>,
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
    pub fn on<F: FnMut() + 'static>(&mut self, name: impl Into<String>, f: F) {
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
}

#[derive(Debug, Clone)]
pub struct HoverTarget {
    pub rect: velox_dom::layout::Rect,
    pub id: u32,
    pub z_index: i32,
    pub order: i32,
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

pub fn collect_click_targets(
    vnode: &VNode,
    layout: &velox_dom::layout::LayoutNode,
    clip: Option<velox_dom::layout::Rect>,
    order: &mut i32,
    out: &mut Vec<ClickTarget>,
) {
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
    let next_clip = match (clip, layout.clip) {
        (Some(c), Some(lc)) => intersect(c, lc),
        (None, Some(lc)) => Some(lc),
        (Some(c), None) => Some(c),
        (None, None) => None,
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
                if let Some(child_layout) = layout.children.get(idx) {
                    if let Some(src_idx) = child_layout.source_index {
                        if let Some(child) = children.get(src_idx) {
                            collect_click_targets(child, child_layout, next_clip, order, out);
                        }
                    }
                }
            }
        }
    }
}

pub fn collect_hover_targets(
    vnode: &VNode,
    layout: &velox_dom::layout::LayoutNode,
    clip: Option<velox_dom::layout::Rect>,
    order: &mut i32,
    out: &mut Vec<HoverTarget>,
) {
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
    let next_clip = match (clip, layout.clip) {
        (Some(c), Some(lc)) => intersect(c, lc),
        (None, Some(lc)) => Some(lc),
        (Some(c), None) => Some(c),
        (None, None) => None,
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
                if let Some(child_layout) = layout.children.get(idx) {
                    if let Some(src_idx) = child_layout.source_index {
                        if let Some(child) = children.get(src_idx) {
                            collect_hover_targets(child, child_layout, next_clip, order, out);
                        }
                    }
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

pub fn hit_test_click(targets: &[ClickTarget], x: f32, y: f32) -> Option<(&str, Option<&str>)> {
    let mut ordered: Vec<(i32, usize)> = targets
        .iter()
        .enumerate()
        .map(|(i, t)| (t.order, i))
        .collect();
    ordered.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    for (_, idx) in ordered {
        let target = &targets[idx];
        let r = target.rect;
        let x0 = r.x as f32;
        let y0 = r.y as f32;
        let x1 = (r.x + r.w) as f32;
        let y1 = (r.y + r.h) as f32;
        if x >= x0 && x <= x1 && y >= y0 && y <= y1 {
            return Some((target.handler.as_str(), target.payload.as_deref()));
        }
    }
    None
}

pub fn hit_test_hover(targets: &[HoverTarget], x: f32, y: f32) -> Option<u32> {
    let mut ordered: Vec<(i32, usize)> = targets
        .iter()
        .enumerate()
        .map(|(i, t)| (t.order, i))
        .collect();
    ordered.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    for (_, idx) in ordered {
        let target = &targets[idx];
        let r = target.rect;
        let x0 = r.x as f32;
        let y0 = r.y as f32;
        let x1 = (r.x + r.w) as f32;
        let y1 = (r.y + r.h) as f32;
        if x >= x0 && x <= x1 && y >= y0 && y <= y1 {
            return Some(target.id);
        }
    }
    None
}

/// Dispatches an event by scanning the VNode tree for props of the form
/// `on:<event>` and invoking registered callbacks with the string value.
/// Returns the number of callbacks invoked.
pub fn dispatch(event: &str, tree: &RenderTree, registry: &mut EventRegistry) -> usize {
    let mut invoked = 0;
    let key = format!("on:{}", event);
    fn walk(node: &VNode, key: &str, out: &mut Vec<String>) {
        match node {
            VNode::Text(_) => {}
            VNode::Element {
                props, children, ..
            } => {
                if let Some(v) = props.attrs.get(key) {
                    out.push(v.clone());
                }
                for c in children {
                    walk(c, key, out);
                }
            }
        }
    }
    let mut targets = Vec::new();
    walk(&tree.root, &key, &mut targets);
    for name in targets {
        if let Some(cb) = registry.handlers.get_mut(&name) {
            cb();
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

    /// Call on cursor moved; fires a one-shot hover event.
    pub fn cursor_moved(&mut self) -> usize {
        if self.hover_sent {
            return 0;
        }
        self.hover_sent = true;
        dispatch("hover", &self.tree, &mut self.registry)
    }

    /// Reset hover state (useful for tests or leaving the window).
    pub fn reset_hover(&mut self) {
        self.hover_sent = false;
    }
}
