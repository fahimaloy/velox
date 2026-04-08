// velox-core/src/lifecycle.rs
use std::cell::RefCell;
use std::collections::HashMap;

/// A unique identifier for a component instance
type ComponentId = usize;

thread_local! {
    #[allow(clippy::missing_const_for_thread_local)]
    static NEXT_COMPONENT_ID: RefCell<usize> = RefCell::new(1);
    #[allow(clippy::type_complexity, clippy::missing_const_for_thread_local)]
    static MOUNTED_HOOKS: RefCell<HashMap<ComponentId, Vec<Box<dyn FnOnce()>>>> = RefCell::new(HashMap::new());
    #[allow(clippy::type_complexity, clippy::missing_const_for_thread_local)]
    static DESTROY_HOOKS: RefCell<HashMap<ComponentId, Vec<Box<dyn FnOnce()>>>> = RefCell::new(HashMap::new());
    #[allow(clippy::missing_const_for_thread_local)]
    static CURRENT_COMPONENT: RefCell<Option<ComponentId>> = RefCell::new(None);
}

/// Generate a new unique component ID
pub fn generate_component_id() -> ComponentId {
    NEXT_COMPONENT_ID.with(|id| {
        let current = *id.borrow();
        *id.borrow_mut() = current + 1;
        current
    })
}

/// Set the current component context for registering hooks
pub fn set_current_component(id: ComponentId) {
    CURRENT_COMPONENT.with(|c| {
        *c.borrow_mut() = Some(id);
    });
}

/// Clear the current component context
pub fn clear_current_component() {
    CURRENT_COMPONENT.with(|c| {
        *c.borrow_mut() = None;
    });
}

/// Get the current component ID if any
pub fn current_component_id() -> Option<ComponentId> {
    CURRENT_COMPONENT.with(|c| *c.borrow())
}

/// Register a hook to run when the current component is mounted
pub fn on_mounted(f: impl FnOnce() + 'static) {
    CURRENT_COMPONENT.with(|c| {
        if let Some(id) = *c.borrow() {
            MOUNTED_HOOKS.with(|h| {
                if h.borrow().contains_key(&id) {
                    h.borrow_mut().get_mut(&id).unwrap().push(Box::new(f));
                } else {
                    h.borrow_mut().insert(id, vec![Box::new(f)]);
                }
            });
        } else {
            eprintln!("[velox-core] WARNING: on_mounted called without a current component context. ");
            eprintln!("Did you forget to call set_current_component()? The hook will not be registered.");
        }
    });
}

/// Internal: run all mounted hooks for a specific component and clear them
pub fn run_mounted_hooks(id: ComponentId) {
    MOUNTED_HOOKS.with(|h| {
        if let Some(hooks) = h.borrow_mut().remove(&id) {
            for hook in hooks {
                hook();
            }
        }
    });
}

/// Register a hook to run before a component is destroyed
pub fn before_destroy(f: impl FnOnce() + 'static) {
    CURRENT_COMPONENT.with(|c| {
        if let Some(id) = *c.borrow() {
            DESTROY_HOOKS.with(|h| {
                h.borrow_mut()
                    .entry(id)
                    .or_default()
                    .push(Box::new(f));
            });
        } else {
            eprintln!("[velox-core] WARNING: before_destroy called without a current component context.");
            eprintln!("Did you forget to call set_current_component()? The hook will not be registered.");
        }
    });
}

/// Internal: run all destroy hooks for a specific component and clear them
pub fn run_destroy_hooks(id: ComponentId) {
    DESTROY_HOOKS.with(|h| {
        if let Some(hooks) = h.borrow_mut().remove(&id) {
            for hook in hooks {
                hook();
            }
        }
    });
}

/// Clean up all hooks for a component (call when component is fully destroyed)
pub fn cleanup_component(id: ComponentId) {
    MOUNTED_HOOKS.with(|h| {
        h.borrow_mut().remove(&id);
    });
    DESTROY_HOOKS.with(|h| {
        h.borrow_mut().remove(&id);
    });
}
