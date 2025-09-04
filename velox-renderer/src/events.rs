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

