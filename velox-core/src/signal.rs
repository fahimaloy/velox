// velox-core/src/signal.rs

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};

/// An effect closure plus the identity the scheduler uses to track it.
///
/// The `id` used to be the effect's heap address (`Rc::as_ptr`), which was not
/// an identity at all: once an effect was dropped its `Rc` allocation was free
/// to be handed to the next effect, so a new effect could inherit a dead
/// effect's id and be silently skipped by the scheduler forever. Ids are now
/// handed out by `NEXT_EFFECT_ID` and are never reused.
struct EffectInner {
    id: u64,
    body: RefCell<Box<dyn FnMut()>>,
}

type Effect = Rc<EffectInner>;

type WeakEffect = Weak<EffectInner>;

/// Source of the monotonic effect ids in `EffectInner::id`.
///
/// Global rather than thread-local so that two effects created on different
/// threads can never be handed the same number, even though every map keyed by
/// it is thread-local. That keeps a future `Send`-ification of `Signal` from
/// inheriting an id-aliasing bug. `Relaxed` is the right ordering: this only has
/// to be unique, and nothing publishes an id across a thread boundary.
static NEXT_EFFECT_ID: AtomicU64 = AtomicU64::new(1);

/// Hand out the id for a newly created effect.
fn next_effect_id() -> u64 {
    NEXT_EFFECT_ID.fetch_add(1, Ordering::Relaxed)
}

// Holds the currently running/collecting effect during dependency tracking.
thread_local! {
    static CURRENT_EFFECT: RefCell<Option<Effect>> = const { RefCell::new(None) };

    // Simple microtask-style scheduler queue and guards.
    static EFFECT_QUEUE: RefCell<VecDeque<Effect>> = const { RefCell::new(VecDeque::new()) };
    static QUEUED: RefCell<HashSet<u64>> = RefCell::new(HashSet::new());
    static IS_FLUSHING: Cell<bool> = const { Cell::new(false) };

    // Keep effects alive - they clean themselves up when dropped
    static EFFECT_STORAGE: RefCell<Vec<Effect>> = const { RefCell::new(Vec::new()) };

    // Track stopped effect IDs so they can be skipped in flush_queue.
    //
    // The `Weak` alongside each id is what makes pruning possible: it answers
    // "does this effect still exist?" without keeping it alive itself. See
    // `mark_stopped`.
    static STOPPED_EFFECTS: RefCell<HashMap<u64, WeakEffect>> = RefCell::new(HashMap::new());
}

/// Stop tracking the effects that no longer exist, once the set has grown past
/// this size.
///
/// Effects are stopped far more often than they are looked up, and every stop
/// adds an entry that nothing ever removes, so an app that mounts and unmounts
/// components repeatedly grew this set without bound. 1024 is several orders of
/// magnitude more than a live effect count and a prune is a linear pass over a
/// thousand `Weak`s, so paying for one occasionally is cheaper than the memory.
const STOPPED_EFFECTS_PRUNE_AT: usize = 1024;

/// Record that `eff` has been stopped, so `flush_queue` will skip it, and keep
/// the set from growing without bound.
///
/// The `Weak` is only a liveness probe. An entry is dropped when the effect it
/// names is gone, which is the one predicate that is always safe — and note
/// *which* obvious predicates are not:
/// - "is it still in `EFFECT_QUEUE`?" is too eager. A stopped effect whose
///   `EffectHandle` the caller still holds stays registered as a `WeakEffect` in
///   its signals' subscriber lists, so a later `Signal::set` re-enqueues it and
///   it is only skipped because its id is still recorded here.
/// - "is it still in `EFFECT_STORAGE`?" is worse than too eager, it is simply
///   wrong: `stop` and `Drop` remove the effect from storage in the same breath
///   that they record it here, so a stopped effect is by definition absent from
///   `EFFECT_STORAGE` and every live entry would be pruned away.
fn mark_stopped(eff: &Effect) {
    STOPPED_EFFECTS.with(|stopped| {
        let mut stopped = stopped.borrow_mut();
        stopped.insert(eff.id, Rc::downgrade(eff));
        if stopped.len() > STOPPED_EFFECTS_PRUNE_AT {
            stopped.retain(|_, weak| weak.strong_count() > 0);
        }
    });
}

fn enqueue_effect(eff: Effect) {
    EFFECT_QUEUE.with(|q| {
        QUEUED.with(|set| {
            let id = eff.id;
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

        let id = eff.id;

        // Mark as not queued before running, so re-enqueues are allowed.
        QUEUED.with(|set| {
            set.borrow_mut().remove(&id);
        });

        // Skip if this effect has been stopped
        let is_stopped = STOPPED_EFFECTS.with(|stopped| stopped.borrow().contains_key(&id));
        if is_stopped {
            continue;
        }

        // Run the effect directly without replacing it
        CURRENT_EFFECT.with(|cur| *cur.borrow_mut() = Some(eff.clone()));
        eff.body.borrow_mut()();
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
            let id = self.effect.id;
            mark_stopped(&self.effect);
            // Clean up from storage to prevent memory leak
            EFFECT_STORAGE.with(|storage| {
                storage.borrow_mut().retain(|e| e.id != id);
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
        let id = self.effect.id;
        mark_stopped(&self.effect);
        EFFECT_STORAGE.with(|storage| {
            storage.borrow_mut().retain(|e| e.id != id);
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
                // Prune dead weak references first. `strong_count` asks the same
                // question as `upgrade().is_some()` but without materialising a
                // temporary `Rc` to answer it.
                subs.retain(|w| w.strong_count() > 0);
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
    let eff = Rc::new(EffectInner {
        id: next_effect_id(),
        body: RefCell::new(Box::new(f) as Box<dyn FnMut()>),
    });

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
    eff.body.borrow_mut()();

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    /// The regression guard for unbounded growth in `STOPPED_EFFECTS`.
    ///
    /// Every stopped effect added an entry and nothing ever removed one, so
    /// this store was a pure leak: a UI that mounts and unmounts components
    /// added one entry per effect for the lifetime of the process. The number
    /// of effects churned here is several times the prune threshold, so
    /// without `mark_stopped`'s pruning the store would hold all of them; with
    /// it, every effect here is dead by the time the loop ends, so the
    /// store stays near-empty.
    ///
    /// This test is deliberately *not* in `velox-core/tests/`: the store is
    /// private, and exposing it as `pub` (or adding a `pub fn` diagnostic
    /// accessor) purely so a test could read it would widen the public API of
    /// `velox-core` — a worse trade than an in-file test. `ergonomics.rs` sets
    /// the precedent for a private-state test living next to the code.
    #[test]
    fn stopped_effects_store_stays_bounded() {
        const CHURN: usize = STOPPED_EFFECTS_PRUNE_AT * 4;

        // Every handle is held alive until the whole batch has been created, and
        // this detail is load-bearing. Creating and dropping one effect per
        // iteration lets the allocator hand back the *same* address every time,
        // and a set keyed by those addresses then collapses to a single entry —
        // so the pre-fix leak becomes invisible and the test passes against the
        // broken code. Keeping the batch alive forces CHURN distinct identities,
        // which is what makes this a real regression guard.
        let mut handles = Vec::with_capacity(CHURN);
        for _ in 0..CHURN {
            let sig = Rc::new(Signal::new(0u32));
            let s = sig.clone();
            handles.push(effect(move || {
                s.get();
            }));
        }

        // Dropping the batch runs `Drop for EffectHandle` for each one, which is
        // what records the id. By this point every earlier effect is dead, so
        // `mark_stopped`'s prune can reclaim them.
        drop(handles);

        let len = STOPPED_EFFECTS.with(|stopped| stopped.borrow().len());
        assert!(
            len <= STOPPED_EFFECTS_PRUNE_AT,
            "STOPPED_EFFECTS held {len} entries after stopping {CHURN} effects; \
             the prune threshold is {STOPPED_EFFECTS_PRUNE_AT}"
        );
    }

    /// Ids come from a monotonic counter, so a fresh effect can never inherit a
    /// dead effect's identity. This is the property the whole fix exists to
    /// establish, and it is deterministic — no allocator behaviour involved,
    /// unlike a test that tries to provoke address reuse.
    #[test]
    fn effect_ids_are_unique_and_increasing() {
        let sig = Rc::new(Signal::new(0u32));

        let s1 = sig.clone();
        let first = effect(move || {
            s1.get();
        });
        let first_id = first.effect.id;

        let s2 = sig.clone();
        let second = effect(move || {
            s2.get();
        });
        let second_id = second.effect.id;

        assert_ne!(first_id, second_id, "two live effects must not share an id");
        assert!(
            second_id > first_id,
            "ids should increase: got {first_id} then {second_id}"
        );

        drop((first, second));
    }

    /// A stopped effect whose `EffectHandle` is still held must stay stopped.
    ///
    /// This is the guard on `mark_stopped`'s pruning predicate. The effect is
    /// no longer in `EFFECT_STORAGE` (that is what stopping does) and may not
    /// be in `EFFECT_QUEUE` either, but it is still alive and still registered
    /// as a weak subscriber, so the next `set()` re-enqueues it. Pruning by
    /// "not queued" or "not in storage" would let it run again; only pruning
    /// by "no longer alive" is correct.
    ///
    /// One entry, so no prune ever fires here. That half of the predicate —
    /// retaining the tombstones of effects that are still alive *across* a
    /// prune — is guarded separately by
    /// `a_prune_keeps_the_tombstone_of_a_stopped_but_live_effect`.
    #[test]
    fn a_stopped_effect_whose_handle_is_still_held_does_not_run_again() {
        let sig = Rc::new(Signal::new(0u32));
        let runs = Rc::new(Cell::new(0u32));

        let s = sig.clone();
        let r = runs.clone();
        let handle = effect(move || {
            s.get();
            r.set(r.get() + 1);
        });
        assert_eq!(runs.get(), 1, "the effect runs once on registration");

        handle.stop();
        assert!(!handle.is_active());

        // The handle is still alive, so the effect still exists and is still
        // subscribed to `sig`. Setting the signal must not revive it.
        sig.set(1);
        assert_eq!(
            runs.get(),
            1,
            "a stopped effect ran again: {} runs",
            runs.get()
        );

        drop(handle);
    }

    /// A prune must keep the tombstone of a stopped effect that is still alive.
    ///
    /// This is the half of `mark_stopped`'s predicate that the other two tests
    /// leave uncovered, and it is the subtle half. Both
    /// `stopped_effects_store_stays_bounded` and
    /// `a_stopped_effect_whose_handle_is_still_held_does_not_run_again` pass
    /// against a predicate that discards *everything* on prune: the first never
    /// reads a tombstone after a prune has fired, and the second only ever holds
    /// a single entry, so `stopped.len() > STOPPED_EFFECTS_PRUNE_AT` is never
    /// true and no prune happens at all. A `stopped.clear()` predicate would
    /// therefore ship a scheduler that forgets stopped effects the moment the
    /// store crosses the threshold.
    ///
    /// The shape here is the only one that can catch that: a stopped-but-alive
    /// effect whose tombstone must still be *present and readable* after the
    /// prune that ran. The sentinel is stopped first, so its tombstone is in
    /// the map before any churn, and it is still alive — its `EffectHandle` is
    /// held to the end of the test — so a correct predicate retains it.
    #[test]
    fn a_prune_keeps_the_tombstone_of_a_stopped_but_live_effect() {
        // The sentinel: stopped, but alive, and still a weak subscriber of `sig`.
        let sig = Rc::new(Signal::new(0u32));
        let runs = Rc::new(Cell::new(0u32));

        let s = sig.clone();
        let r = runs.clone();
        let sentinel = effect(move || {
            s.get();
            r.set(r.get() + 1);
        });
        assert_eq!(runs.get(), 1, "the effect runs once on registration");

        // Record the sentinel while the store is still far below the threshold,
        // so its tombstone is present before anything can prune.
        sentinel.stop();

        // Churn past the threshold. Each churned effect gets its own signal and
        // its handle is dropped immediately, so every one of them dies before
        // the next iteration and is a legitimate prune candidate. None of them
        // can disturb the sentinel: they never touch `sig`.
        for _ in 0..=STOPPED_EFFECTS_PRUNE_AT {
            let churn_sig = Rc::new(Signal::new(0u32));
            let c = churn_sig.clone();
            let handle = effect(move || {
                c.get();
            });
            drop(handle);
        }

        // A prune has therefore certainly fired: `STOPPED_EFFECTS_PRUNE_AT + 1`
        // effects were stopped on top of the sentinel, so a store that only ever
        // grew would hold more entries than the threshold allows.
        let len = STOPPED_EFFECTS.with(|stopped| stopped.borrow().len());
        assert!(
            len <= STOPPED_EFFECTS_PRUNE_AT,
            "expected a prune to have fired, but the store still holds {len} entries \
             after {} stops (threshold {STOPPED_EFFECTS_PRUNE_AT})",
            STOPPED_EFFECTS_PRUNE_AT + 2
        );

        // The sentinel is still alive and still subscribed, so this `set`
        // re-enqueues it. The only thing that can stop it running is its
        // tombstone, which is the entry the prune had to keep.
        sig.set(1);
        assert_eq!(
            runs.get(),
            1,
            "the prune discarded the tombstone of a stopped effect that was still \
             alive, so it ran again: {} runs",
            runs.get()
        );

        drop(sentinel);
    }
}
