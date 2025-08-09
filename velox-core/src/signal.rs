// velox-core/src/signal.rs

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

// Holds the currently running/collecting effect during dependency tracking.
thread_local! {
    static CURRENT_EFFECT: RefCell<Option<Rc<RefCell<Box<dyn FnMut()>>>>> =
        RefCell::new(None);

    // Simple microtask-style scheduler queue and guards.
    static EFFECT_QUEUE: RefCell<Vec<Rc<RefCell<Box<dyn FnMut()>>>>> =
        RefCell::new(Vec::new());
    static QUEUED: RefCell<HashSet<usize>> = RefCell::new(HashSet::new());
    static IS_FLUSHING: Cell<bool> = Cell::new(false);
}

fn ptr_id(rc: &Rc<RefCell<Box<dyn FnMut()>>>) -> usize {
    rc.as_ptr() as usize
}

fn enqueue_effect(eff: Rc<RefCell<Box<dyn FnMut()>>>) {
    EFFECT_QUEUE.with(|q| {
        QUEUED.with(|set| {
            let id = ptr_id(&eff);
            let mut set_b = set.borrow_mut();
            if set_b.insert(id) {
                q.borrow_mut().push(eff);
            }
        });
    });
}

fn flush_queue() {
    // Prevent re-entrant flush; effects scheduled during a flush will be queued
    // and processed by this outer flush.
    if IS_FLUSHING.with(|f| f.replace(true)) {
        return;
    }

    loop {
        let next = EFFECT_QUEUE.with(|q| q.borrow_mut().pop());
        let Some(eff) = next else { break };

        // Mark as not queued before running, so re-enqueues are allowed.
        QUEUED.with(|set| {
            set.borrow_mut().remove(&ptr_id(&eff));
        });

        // Extract the closure out of the RefCell so we don't hold a mutable borrow
        // while executing it (the body may call set() and re-enqueue itself).
        let mut func: Box<dyn FnMut()> = {
            let mut b = eff.borrow_mut();
            std::mem::replace(&mut *b, Box::new(|| {}))
        };

        // Set current effect for dependency collection.
        CURRENT_EFFECT.with(|cur| *cur.borrow_mut() = Some(eff.clone()));
        // Run without holding any RefCell borrows to `eff`.
        func();
        // Clear current effect.
        CURRENT_EFFECT.with(|cur| *cur.borrow_mut() = None);

        // Put the function back into the effect cell.
        {
            let mut b = eff.borrow_mut();
            *b = func;
        }
    }

    IS_FLUSHING.with(|f| f.set(false));
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
                if !subs.iter().any(|e| Rc::ptr_eq(e, effect_rc)) {
                    subs.push(effect_rc.clone());
                }
            }
        });
        self.value.borrow().clone()
    }

    /// Update the value and notify all subscribers via the scheduler.
    pub fn set(&self, new: T) {
        *self.value.borrow_mut() = new;

        // Snapshot subscribers before enqueuing.
        let subscribers = {
            let subs = self.subscribers.borrow();
            subs.clone()
        };

        for subscriber in subscribers {
            enqueue_effect(subscriber);
        }
        flush_queue();
    }
}

/// Register a closure as a reactive effect:
/// - runs immediately to collect dependencies,
/// - then re-runs whenever any `Signal` it `get()`s is `set()`.
pub fn effect<F>(f: F)
where
    F: FnMut() + 'static,
{
    let eff = Rc::new(RefCell::new(Box::new(f) as Box<dyn FnMut()>));

    // Initial run with dependency collection.
    CURRENT_EFFECT.with(|current| *current.borrow_mut() = Some(eff.clone()));

    // Extract, run, and restore (same pattern as in flush)
    let mut func: Box<dyn FnMut()> = {
        let mut b = eff.borrow_mut();
        std::mem::replace(&mut *b, Box::new(|| {}))
    };
    func();
    {
        let mut b = eff.borrow_mut();
        *b = func;
    }

    CURRENT_EFFECT.with(|current| *current.borrow_mut() = None);
}
