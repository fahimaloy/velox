use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Type alias for the injection context map.
type InjectionMap = Rc<RefCell<HashMap<String, Box<dyn Any>>>>;

// Note: the type complexity here is inherent to the type-erased injection pattern.
thread_local! {
    static INJECTION_CONTEXT: RefCell<Option<InjectionMap>> = RefCell::new(None);
    // Stack of parent contexts for hierarchical injection
    static CONTEXT_STACK: RefCell<Vec<InjectionMap>> = RefCell::new(Vec::new());
}

/// Provide a value to descendant components
pub fn provide<T: 'static>(key: &str, value: T) {
    INJECTION_CONTEXT.with(|ctx| {
        if let Some(map_rc) = ctx.borrow().as_ref() {
            map_rc
                .borrow_mut()
                .insert(key.to_string(), Box::new(Rc::new(value)));
        }
    });
}

/// Inject a value from ancestor, searching up the context stack
pub fn inject<T: 'static>(key: &str) -> Option<Rc<T>> {
    // First try current context
    let current = INJECTION_CONTEXT.with(|ctx| {
        ctx.borrow().as_ref().and_then(|map_rc| {
            map_rc
                .borrow()
                .get(key)
                .and_then(|boxed| boxed.downcast_ref::<Rc<T>>().cloned())
        })
    });

    if current.is_some() {
        return current;
    }

    // Search up the context stack
    CONTEXT_STACK.with(|stack| {
        for parent_ctx in stack.borrow().iter().rev() {
            if let Some(value) = parent_ctx
                .borrow()
                .get(key)
                .and_then(|boxed| boxed.downcast_ref::<Rc<T>>().cloned())
            {
                return Some(value);
            }
        }
        None
    })
}

/// Alternative: returns default if not found
pub fn inject_or<T: 'static>(key: &str, default: T) -> Rc<T> {
    inject(key).unwrap_or_else(|| Rc::new(default))
}

/// Set up an injection context for the duration of a closure
/// This creates a new context that can access parent values via the context stack
pub fn with_injection_context<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    // Create a new injection map for this context
    let new_map: InjectionMap = Rc::new(RefCell::new(HashMap::new()));

    // Scope guard to ensure restoration even on panic
    struct ContextGuard {
        saved_context: Option<InjectionMap>,
        pushed_to_stack: bool,
    }
    impl Drop for ContextGuard {
        fn drop(&mut self) {
            INJECTION_CONTEXT.with(|ctx| {
                *ctx.borrow_mut() = self.saved_context.take();
            });
            // Only pop from context stack if we pushed to it
            if self.pushed_to_stack {
                CONTEXT_STACK.with(|stack| {
                    stack.borrow_mut().pop();
                });
            }
        }
    }

    let previous = INJECTION_CONTEXT.with(|ctx| ctx.borrow_mut().replace(new_map.clone()));

    // Push previous context to stack so child can access parent values
    let pushed_to_stack = previous.is_some();
    if let Some(parent) = previous.clone() {
        CONTEXT_STACK.with(|stack| {
            stack.borrow_mut().push(parent);
        });
    }

    let _guard = ContextGuard { saved_context: previous, pushed_to_stack };

    // Run the closure
    f()
}

/// Initialize injection context at the start of an application
pub fn init_injection_context() {
    INJECTION_CONTEXT.with(|ctx| {
        if ctx.borrow().is_none() {
            let new_map: InjectionMap = Rc::new(RefCell::new(HashMap::new()));
            *ctx.borrow_mut() = Some(new_map);
        }
    });
}

/// Clear the injection context (useful for testing)
pub fn clear_injection_context() {
    INJECTION_CONTEXT.with(|ctx| {
        *ctx.borrow_mut() = None;
    });
    CONTEXT_STACK.with(|stack| {
        stack.borrow_mut().clear();
    });
}
