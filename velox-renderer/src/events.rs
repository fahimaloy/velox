use std::collections::HashMap;

use velox_dom::VNode;

use crate::RenderTree;

pub struct EventRegistry {
    handlers: HashMap<String, Box<dyn FnMut()>>,
}

impl EventRegistry {
    pub fn new() -> Self {
        Self { handlers: HashMap::new() }
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
}

#[derive(Debug, Clone)]
pub struct HoverTarget {
    pub rect: velox_dom::layout::Rect,
    pub id: u32,
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
    out: &mut Vec<ClickTarget>,
) {
    match vnode {
        VNode::Text(_) => {}
        VNode::Element { props, children, .. } => {
            if let Some(handler) = props.attrs.get("on:click").cloned() {
                let payload = props.attrs.get("on:click-payload").cloned();
                out.push(ClickTarget { rect: layout.rect, handler, payload });
            }
            for (child, child_layout) in children.iter().zip(&layout.children) {
                collect_click_targets(child, child_layout, out);
            }
        }
    }
}

pub fn collect_hover_targets(
    vnode: &VNode,
    layout: &velox_dom::layout::LayoutNode,
    out: &mut Vec<HoverTarget>,
) {
    match vnode {
        VNode::Text(_) => {}
        VNode::Element { tag, props, children, .. } => {
            if is_hoverable(tag, props) {
                let id = props
                    .attrs
                    .get("data-hover-id")
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(0);
                out.push(HoverTarget { rect: layout.rect, id });
            }
            for (child, child_layout) in children.iter().zip(&layout.children) {
                collect_hover_targets(child, child_layout, out);
            }
        }
    }
}

pub fn hit_test_click<'a>(
    targets: &'a [ClickTarget],
    x: f32,
    y: f32,
) -> Option<(&'a str, Option<&'a str>)> {
    for target in targets {
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
    for target in targets {
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
            VNode::Element { props, children, .. } => {
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
        Self { tree, registry: EventRegistry::new(), last_click: None, hover_sent: false }
    }

    /// Call on mouse left-button press; detects double-click within 400ms.
    pub fn mouse_click(&mut self) -> usize {
        let now = Instant::now();
        let clicks = if let Some(prev) = self.last_click {
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
        };
        clicks
    }

    /// Call on cursor moved; fires a one-shot hover event.
    pub fn cursor_moved(&mut self) -> usize {
        if self.hover_sent { return 0; }
        self.hover_sent = true;
        dispatch("hover", &self.tree, &mut self.registry)
    }

    /// Reset hover state (useful for tests or leaving the window).
    pub fn reset_hover(&mut self) { self.hover_sent = false; }
}
