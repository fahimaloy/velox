// velox-core/src/lifecycle.rs
use std::cell::RefCell;

thread_local! {
    static MOUNTED_HOOKS: RefCell<Vec<Box<dyn FnOnce()>>> = RefCell::new(Vec::new());
    static DESTROY_HOOKS: RefCell<Vec<Box<dyn FnOnce()>>> = RefCell::new(Vec::new());
}

/// Register a hook to run when a component is mounted
pub fn on_mounted(f: impl FnOnce() + 'static) {
    MOUNTED_HOOKS.with(|h| h.borrow_mut().push(Box::new(f)));
}

/// Internal: run all mounted hooks
pub fn run_mounted_hooks() {
    MOUNTED_HOOKS.with(|h| {
        for hook in h.borrow_mut().drain(..) {
            hook();
        }
    });
}

/// Register a hook to run before a component is destroyed
pub fn before_destroy(f: impl FnOnce() + 'static) {
    DESTROY_HOOKS.with(|h| h.borrow_mut().push(Box::new(f)));
}

/// Internal: run all destroy hooks
pub fn run_destroy_hooks() {
    DESTROY_HOOKS.with(|h| {
        for hook in h.borrow_mut().drain(..) {
            hook();
        }
    });
}

