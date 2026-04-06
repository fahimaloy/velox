use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Type alias for the injection context map.
type InjectionMap = Rc<RefCell<HashMap<String, Box<dyn Any>>>>;

// Note: the type complexity here is inherent to the type-erased injection pattern.
thread_local! {
    static INJECTION_CONTEXT: RefCell<Option<InjectionMap>> = RefCell::new(None);
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

/// Inject a value from ancestor
pub fn inject<T: 'static>(key: &str) -> Option<Rc<T>> {
    INJECTION_CONTEXT.with(|ctx| {
        ctx.borrow().as_ref().and_then(|map_rc| {
            map_rc
                .borrow()
                .get(key)
                .and_then(|boxed| boxed.downcast_ref::<Rc<T>>().cloned())
        })
    })
}

/// Alternative: returns default if not found
pub fn inject_or<T: 'static>(key: &str, default: T) -> Rc<T> {
    inject(key).unwrap_or_else(|| Rc::new(default))
}
