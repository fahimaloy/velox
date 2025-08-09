// velox-core/src/signal.rs

use std::cell::RefCell;
use std::rc::Rc;

// Holds the currently registering effect (if any)
thread_local! {
    static CURRENT_EFFECT: RefCell<Option<Rc<RefCell<Box<dyn FnMut()>>>>> =
        RefCell::new(None);
}

/// A reactive signal wrapping a `T: Clone`.
pub struct Signal<T> {
    value: RefCell<T>,
    subscribers: RefCell<Vec<Rc<RefCell<Box<dyn FnMut()>>>>>,
}

impl<T> Signal<T>
where
    T: Clone,
{
    /// Create a new signal.
    pub fn new(initial: T) -> Self {
        Self {
            value: RefCell::new(initial),
            subscribers: RefCell::new(Vec::new()),
        }
    }

    /// Read the value, and if inside an `effect`, register that effect as a subscriber.
    pub fn get(&self) -> T {
        CURRENT_EFFECT.with(|current| {
            if let Some(effect_rc) = current.borrow().as_ref() {
                let mut subs = self.subscribers.borrow_mut();
                // only add if not already present
                if !subs.iter().any(|e| Rc::ptr_eq(e, effect_rc)) {
                    subs.push(effect_rc.clone());
                }
            }
        });
        self.value.borrow().clone()
    }

    /// Update the value and notify all subscribers.
    pub fn set(&self, new: T) {
        *self.value.borrow_mut() = new;
        // clone out the list so we drop the borrow on `subscribers` before calling
        let subscribers = {
            let subs = self.subscribers.borrow();
            subs.clone()
        };
        for subscriber in subscribers {
            (subscriber.borrow_mut())();
        }
    }
}

/// Register a closure as a reactive effect: it runs immediately,
/// and then again whenever any `Signal` it `get()`-reads is `set()`.
pub fn effect<F>(f: F)
where
    F: FnMut() + 'static,
{
    // wrap the user closure in an Rc<RefCell<Box<dyn FnMut()>>>
    let effect_rc = Rc::new(RefCell::new(Box::new(f) as Box<dyn FnMut()>));
    // mark it as the current effect for dependency collection
    CURRENT_EFFECT.with(|current| {
        *current.borrow_mut() = Some(effect_rc.clone());
    });
    // run it once to collect dependencies
    (effect_rc.borrow_mut())();
    // clear the current effect
    CURRENT_EFFECT.with(|current| {
        *current.borrow_mut() = None;
    });
}
