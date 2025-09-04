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

