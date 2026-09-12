// velox-core/src/signal.rs

use std::cell::{Cell, RefCell};
use std::collections::{HashSet, VecDeque};
use std::rc::{Rc, Weak};

/// Type alias for reactive effect closures.
type Effect = Rc<RefCell<Box<dyn FnMut()>>>;

type WeakEffect = Weak<RefCell<Box<dyn FnMut()>>>;

// Holds the currently running/collecting effect during dependency tracking.
thread_local! {
    static CURRENT_EFFECT: RefCell<Option<Effect>> = RefCell::new(None);

    // Simple microtask-style scheduler queue and guards.
    #[allow(clippy::type_complexity)]
    static EFFECT_QUEUE: RefCell<VecDeque<Effect>> = RefCell::new(VecDeque::new());
    static QUEUED: RefCell<HashSet<usize>> = RefCell::new(HashSet::new());
    static IS_FLUSHING: Cell<bool> = const { Cell::new(false) };

    // Keep effects alive - they clean themselves up when dropped
    static EFFECT_STORAGE: RefCell<Vec<Effect>> = RefCell::new(Vec::new());

    // Track stopped effect IDs so they can be skipped in flush_queue
    static STOPPED_EFFECTS: RefCell<HashSet<usize>> = RefCell::new(HashSet::new());
}

fn ptr_id(eff: &Effect) -> usize {
    eff.as_ptr() as usize
}

fn enqueue_effect(eff: Effect) {
    EFFECT_QUEUE.with(|q| {
        QUEUED.with(|set| {
            let id = ptr_id(&eff);
            let mut set_b = set.borrow_mut();
            if set_b.insert(id) {
                q.borrow_mut().push_back(eff);
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
        let next = EFFECT_QUEUE.with(|q| q.borrow_mut().pop_front());
        let Some(eff) = next else { break };

        let id = ptr_id(&eff);

        // Mark as not queued before running, so re-enqueues are allowed.
        QUEUED.with(|set| {
            set.borrow_mut().remove(&id);
        });

        // Skip if this effect has been stopped
        let is_stopped = STOPPED_EFFECTS.with(|stopped| stopped.borrow().contains(&id));
        if is_stopped {
            continue;
        }

        // Run the effect directly without replacing it
        CURRENT_EFFECT.with(|cur| *cur.borrow_mut() = Some(eff.clone()));
        eff.borrow_mut()();
        CURRENT_EFFECT.with(|cur| *cur.borrow_mut() = None);
    }

    IS_FLUSHING.with(|f| f.set(false));
}

/// A handle to stop/dispose a reactive effect.
pub struct EffectHandle {
    effect: Effect,
    active: Cell<bool>,
}

impl EffectHandle {
    /// Stop the effect from running in the future.
    pub fn stop(&self) {
        if self.active.get() {
            self.active.set(false);
            // Mark the effect as stopped so it won't run in flush_queue
            let id = ptr_id(&self.effect);
            STOPPED_EFFECTS.with(|stopped| {
                stopped.borrow_mut().insert(id);
            });
            // Clean up from storage to prevent memory leak
            EFFECT_STORAGE.with(|storage| {
                storage.borrow_mut().retain(|e| ptr_id(e) != id);
            });
        }
    }

    /// Check if the effect is still active.
    pub fn is_active(&self) -> bool {
        self.active.get()
    }
}

impl Drop for EffectHandle {
    fn drop(&mut self) {
        // When handle is dropped, mark as stopped and clean up resources
        let id = ptr_id(&self.effect);
        STOPPED_EFFECTS.with(|stopped| {
            stopped.borrow_mut().insert(id);
        });
        EFFECT_STORAGE.with(|storage| {
            storage.borrow_mut().retain(|e| ptr_id(e) != id);
        });
    }
}

/// A reactive signal wrapping a `T: Clone`.
pub struct Signal<T> {
    value: RefCell<T>,
    /// Stores a deferred update when `set()` is called while `value` is borrowed.
    pending: RefCell<Option<T>>,
    subscribers: RefCell<Vec<WeakEffect>>,
    /// Effect handle for computed signals - keeps the internal effect alive
    _effect_handle: RefCell<Option<EffectHandle>>,
}

impl<T> Signal<T>
where
    T: Clone,
{
    /// Create a new signal.
    pub fn new(initial: T) -> Self {
        Self {
            value: RefCell::new(initial),
            pending: RefCell::new(None),
            subscribers: RefCell::new(Vec::new()),
            _effect_handle: RefCell::new(None),
        }
    }

    /// Read the value, and if inside an `effect`, register that effect as a subscriber.
    /// Applies any pending deferred update before reading.
    pub fn get(&self) -> T {
        // Apply any pending deferred update before returning the value.
        if let Some(pending) = self.pending.borrow_mut().take() {
            if let Ok(mut v) = self.value.try_borrow_mut() {
                *v = pending;
            } else {
                // Still borrowed — put pending back, read will return stale value.
                // This is extremely unlikely since we're about to borrow it ourselves.
                *self.pending.borrow_mut() = Some(pending);
            }
        }
        CURRENT_EFFECT.with(|current| {
            if let Some(effect_rc) = current.borrow().as_ref() {
                let mut subs = self.subscribers.borrow_mut();
                // Prune dead weak references first
                subs.retain(|w| w.upgrade().is_some());
                // Check if already subscribed
                let already_subscribed = subs
                    .iter()
                    .any(|w| w.upgrade().is_some_and(|rc| Rc::ptr_eq(&rc, effect_rc)));
                if !already_subscribed {
                    subs.push(Rc::downgrade(effect_rc));
                }
            }
        });
        self.value.borrow().clone()
    }

    /// Update the value and notify all subscribers via the scheduler.
    pub fn set(&self, new: T) {
        match self.value.try_borrow_mut() {
            Ok(mut v) => *v = new,
            Err(_) => {
                // Value is currently borrowed (likely by an active effect reading this signal).
                // Store the new value as pending; it will be applied on the next `get()` call.
                *self.pending.borrow_mut() = Some(new);
            }
        }

        // Snapshot subscribers before enqueuing, filtering out dead references.
        let subscribers: Vec<Effect> = {
            let mut subs = self.subscribers.borrow_mut();
            // Clean up dead weak references and upgrade living ones
            let living: Vec<Effect> = subs.drain(..).filter_map(|w| w.upgrade()).collect();
            // Put living weak refs back
            for eff in &living {
                subs.push(Rc::downgrade(eff));
            }
            living
        };

        for subscriber in subscribers {
            enqueue_effect(subscriber);
        }
        flush_queue();
    }

    /// Functional update: apply a closure to the current value and set the result.
    /// Useful for updates like `count.update(|v| v + 1)`.
    pub fn update<F>(&self, f: F)
    where
        F: FnOnce(T) -> T,
    {
        // Read directly without registering as subscriber to avoid self-subscription
        let current = self.value.borrow().clone();
        let new = f(current);
        self.set(new);
    }

    /// Set value only if it changed (requires T: PartialEq).
    pub fn set_if_changed(&self, new: T)
    where
        T: PartialEq,
    {
        if *self.value.borrow() != new {
            self.set(new);
        }
    }
}

/// Register a closure as a reactive effect:
/// - runs immediately to collect dependencies,
/// - then re-runs whenever any `Signal` it `get()`s is `set()`.
///
/// Returns an EffectHandle that can be used to stop the effect.
pub fn effect<F>(f: F) -> EffectHandle
where
    F: FnMut() + 'static,
{
    let eff = Rc::new(RefCell::new(Box::new(f) as Box<dyn FnMut()>));

    let handle = EffectHandle {
        effect: eff.clone(),
        active: Cell::new(true),
    };

    // Store effect to keep it alive - effect will clean itself up when handle is dropped
    EFFECT_STORAGE.with(|storage| {
        storage.borrow_mut().push(eff.clone());
    });

    // Initial run with dependency collection.
    CURRENT_EFFECT.with(|current| *current.borrow_mut() = Some(eff.clone()));

    // Run the effect without replacing it (unlike flush_queue which needs swap pattern)
    eff.borrow_mut()();

    CURRENT_EFFECT.with(|current| *current.borrow_mut() = None);

    handle
}

/// Create a computed (derived) signal that automatically updates when its dependencies change.
/// The computation function is re-run whenever any signal it reads changes.
pub fn computed<T, F>(compute: F) -> Rc<Signal<T>>
where
    T: Clone + PartialEq + 'static,
    F: Fn() -> T + 'static,
{
    use std::cell::RefCell;

    let compute_rc = Rc::new(RefCell::new(compute));
    let signal = Rc::new(Signal::new((compute_rc.borrow())()));

    // Create an effect that re-runs the computation when dependencies change
    let signal_weak = Rc::downgrade(&signal);

    let handle = effect({
        let compute_rc = compute_rc.clone();
        move || {
            if let Some(sig) = signal_weak.upgrade() {
                let new_value = (compute_rc.borrow())();
                // Use set_if_changed to notify subscribers but avoid infinite loops
                // when the value hasn't actually changed
                if *sig.value.borrow() != new_value {
                    sig.set(new_value);
                }
            }
        }
    });

    // Store the effect handle in the signal to keep the effect alive
    *signal._effect_handle.borrow_mut() = Some(handle);

    signal
}
