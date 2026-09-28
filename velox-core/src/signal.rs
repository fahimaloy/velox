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

/// How large a signal's subscriber vector must get before `Signal::get` prunes
/// the dead entries out of it.
///
/// This is a *dead-entry budget*, not a fan-out target, and the two must not be
/// confused. It has to sit above the number of live subscribers a signal
/// realistically has, because the prune fires on total length; a signal that
/// legitimately has this many live subscribers sweeps on every read and gains
/// nothing. Below that, the prune is amortised: it runs at most once per this
/// many pushes, so the per-read cost of housekeeping is O(1) instead of O(n).
///
/// Each entry is an 8-byte `Weak`, so the most this can ever leak per signal is
/// half a KiB — paid only by signals that actually accumulated that many dead
/// entries. A signal that is read but never `set` is the pathological case it
/// exists to bound: nothing else would ever reclaim those entries.
const SUBSCRIBER_SWEEP_AT: usize = 64;

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

/// Resets `IS_FLUSHING` on the way out of `flush_queue`, including on unwind.
///
/// A panicking effect body unwinds straight out of the flush loop, so the flag
/// used to be cleared only by falling off the end of that loop. One panic
/// therefore left it set for the rest of the process, and because *every*
/// `flush_queue` returns early when it is set, the scheduler was then dead
/// permanently: effects were still created and enqueued, and nothing ever ran
/// them again, with no recovery path. `next_tick::FlushGuard` is the existing
/// precedent for this shape in this crate.
struct FlushResetGuard;

impl Drop for FlushResetGuard {
    fn drop(&mut self) {
        IS_FLUSHING.with(|f| f.set(false));
    }
}

/// The scheduler state that must hold while a single effect body runs, restored
/// when that body returns *or unwinds*.
///
/// Both flags were previously restored only on the success path.
///
/// - `IS_FLUSHING` is raised so a body that writes a signal it also read cannot
///   re-enter `flush_queue` and pop the very effect that is currently executing,
///   borrowing its body a second time while the first borrow is still live.
///   The *prior* value is restored rather than `false`, because an effect can be
///   created from inside another effect's body: the inner guard has to put the
///   enclosing flush's guard back, not clear it.
/// - `CURRENT_EFFECT` is the dependency-collection target. A stranded one
///   silently attributes the next read — even one made outside any effect — to a
///   dead effect, registering a dead weak subscriber on that signal.
///
/// Restoring from `Drop` is what makes this unwind-safe, and it is sufficient:
/// `RefCell` has no poisoning (that is `Mutex`), nothing takes the body out of
/// `EffectInner`, and `Drop for EffectHandle` never runs it, so there is no path
/// that needs the body recoverable after a panic.
struct EffectScopeGuard {
    was_flushing: bool,
    previous_effect: Option<Effect>,
}

impl EffectScopeGuard {
    /// Enter the body of `eff`: it becomes the dependency target, and the
    /// re-entrancy guard goes up for the duration.
    fn enter(eff: &Effect) -> Self {
        let was_flushing = IS_FLUSHING.with(|f| f.replace(true));
        let previous_effect = CURRENT_EFFECT.with(|cur| cur.borrow_mut().replace(eff.clone()));
        Self {
            was_flushing,
            previous_effect,
        }
    }
}

impl Drop for EffectScopeGuard {
    fn drop(&mut self) {
        let previous = self.previous_effect.take();
        CURRENT_EFFECT.with(|cur| *cur.borrow_mut() = previous);
        IS_FLUSHING.with(|f| f.set(self.was_flushing));
    }
}

fn flush_queue() {
    // Prevent re-entrant flush; effects scheduled during a flush will be queued
    // and processed by this outer flush.
    if IS_FLUSHING.with(|f| f.replace(true)) {
        return;
    }

    // Only the outermost flush ever reaches this point — every re-entrant call
    // returned above — so clearing the flag unconditionally here cannot clear a
    // guard that some enclosing scope is relying on.
    let _flush_guard = FlushResetGuard;

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

        // The dependency target and the re-entrancy guard are both in place for
        // the run, and both are restored when the body returns or unwinds.
        let _scope = EffectScopeGuard::enter(&eff);
        eff.body.borrow_mut()();
    }
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
                // Prune dead weak references, but only once the vector is big
                // enough to be worth the walk. This used to run on every read,
                // which made the hottest path in the framework both O(n) and a
                // mutation.
                //
                // Bounded rather than unconditional on purpose. A dead entry is
                // created when an effect's last strong `Rc` goes away, which is
                // when its `EffectHandle` is dropped — the handle holds that last
                // reference, so calling `stop()` and keeping the handle leaves
                // the entry alive, a stopped-but-alive tombstone that
                // `flush_queue` skips by id. Either way it is not created here.
                // Nothing removes those entries again until the next `set()` on
                // this signal, so a signal that is read but never written would
                // accumulate them forever and a later subscriber would append to
                // an ever-growing vector.
                // Pruning at death time is not available to us: an effect body is
                // `Box<dyn FnMut()>` and `Signal<T>` is generic, so an effect
                // cannot remember which signals it subscribed to without a
                // type-erased registry on `EffectInner`.
                //
                // So dead entries are capped here instead. The sweep only fires
                // when the vector has actually grown, so it runs at most once per
                // `SUBSCRIBER_SWEEP_AT` pushes rather than once per read.
                if subs.len() >= SUBSCRIBER_SWEEP_AT {
                    // `strong_count` asks the same question as
                    // `upgrade().is_some()` but without materialising a temporary
                    // `Rc` to answer it.
                    subs.retain(|w| w.strong_count() > 0);
                }
                // Check if already subscribed.
                //
                // With the sweep above no longer running first to pre-clean the
                // list, duplicate suppression now rests on `Rc::ptr_eq` alone.
                // It is a sound identity test: `upgrade()` fails for a dead
                // entry, and two distinct `EffectInner`s cannot share an
                // allocation, so `ptr_eq` holds exactly when both are the same
                // effect. Cost is unchanged, O(len) — this is a constant-factor
                // win (one walk per read instead of two), not an asymptotic one.
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
    //
    // This runs under the same guard as every queued run, and it has to. Without
    // the re-entrancy guard a body that reads a signal and then writes it
    // re-enters `flush_queue`, which pops this very effect and borrows its body
    // a second time while the borrow below is still live — a panic. Deferring
    // the initial run to the queue instead would break the documented contract
    // that `effect` runs the body before returning, so the body runs here and a
    // write it performs is queued for the next flush instead, which is exactly
    // what a write from inside `flush_queue` already does.
    {
        let _scope = EffectScopeGuard::enter(&eff);
        eff.body.borrow_mut()();
    }

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

    /// Builds an effect that panics on its second run and never again, plus the
    /// run counter and the signal it subscribes to.
    ///
    /// Panicking on the *second* run is what makes the panic land inside
    /// `flush_queue` — reached from a later `set` — rather than inside the
    /// `effect` call itself, which is where `catch_unwind` in the callers sits.
    /// Panicking exactly once matters too: without it, a resurrected effect in a
    /// test that expects it *not* to be resurrected would panic again and abort
    /// the test instead of failing it with a readable assertion.
    fn panicking_on_second_run() -> (Rc<Signal<u32>>, Rc<Cell<u32>>, EffectHandle) {
        let sig = Rc::new(Signal::new(0u32));
        let runs = Rc::new(Cell::new(0u32));
        let panicked = Rc::new(Cell::new(false));

        let s = sig.clone();
        let r = runs.clone();
        let p = panicked.clone();
        let handle = effect(move || {
            r.set(r.get() + 1);
            s.get();
            if r.get() >= 2 && !p.replace(true) {
                panic!("effect body panicked on purpose");
            }
        });

        assert_eq!(runs.get(), 1, "the effect runs once on registration");
        (sig, runs, handle)
    }

    /// A panicking effect body must not leave the scheduler dead.
    ///
    /// `IS_FLUSHING` used to be cleared only by falling out of the flush loop, so
    /// a panicking body unwound past it and left the flag set. Every later
    /// `flush_queue` then returned immediately, so effects were still created and
    /// enqueued and nothing ever ran them again — and there was no recovery path.
    /// This is the reason the flag is restored from a `Drop` guard.
    ///
    /// In-file rather than in `velox-core/tests/`: the sibling test below has to
    /// read and clear `IS_FLUSHING` to isolate `CURRENT_EFFECT`, and that is only
    /// reachable from inside the module.
    ///
    /// Fails deterministically before the fix: the final `other.set(1)` enqueues
    /// the effect and nothing flushes it, so the run count stays at 1.
    #[test]
    fn a_panicking_effect_body_does_not_strand_the_scheduler() {
        let (sig, _runs, _panicking) = panicking_on_second_run();

        // This `set` flushes the queue, which runs the body; the body panics and
        // the unwind escapes `flush_queue`.
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sig.set(1);
        }));
        assert!(
            outcome.is_err(),
            "the body was supposed to panic during the flush"
        );

        // The scheduler must still be alive: a fresh, unrelated signal has to
        // schedule and run a brand new effect.
        let other = Rc::new(Signal::new(0u32));
        let other_runs = Rc::new(Cell::new(0u32));

        let o = other.clone();
        let or = other_runs.clone();
        let _other_handle = effect(move || {
            or.set(or.get() + 1);
            o.get();
        });
        assert_eq!(other_runs.get(), 1, "the fresh effect ran on registration");

        other.set(1);
        assert_eq!(
            other_runs.get(),
            2,
            "the scheduler is still dead: `IS_FLUSHING` was stranded by the unwind, \
             so this `set` enqueued the effect and nothing ever flushed it"
        );
    }

    /// A panicking effect body must not strand `CURRENT_EFFECT`.
    ///
    /// A stranded `CURRENT_EFFECT` attributes the next read — even one made
    /// outside any effect — to the dead effect, registering a dead weak
    /// subscriber that a later `set` then revives.
    ///
    /// `IS_FLUSHING` is cleared by hand here so this test isolates
    /// `CURRENT_EFFECT` stranding; the test above is the guard on that flag, and
    /// until both are fixed a stranded flag masks this one by stopping the flush
    /// that would expose it. After the fix the hand-clear is a no-op.
    #[test]
    fn a_panicking_effect_body_does_not_strand_current_effect() {
        let (sig, runs, _panicking) = panicking_on_second_run();

        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sig.set(1);
        }));
        assert!(
            outcome.is_err(),
            "the body was supposed to panic during the flush"
        );
        IS_FLUSHING.with(|f| f.set(false));

        // A read outside any effect body must not be attributed to anything.
        let fresh = Rc::new(Signal::new(0u32));
        assert_eq!(
            fresh.get(),
            0,
            "a read outside any effect must not be attributed to a dead effect"
        );

        // If the unwind stranded `CURRENT_EFFECT`, the read above registered the
        // dead effect as a subscriber of `fresh`, and this `set` revives it.
        fresh.set(1);
        assert_eq!(
            runs.get(),
            2,
            "a stranded `CURRENT_EFFECT` attributed an out-of-effect read to the \
             dead effect, which then ran again: {} runs",
            runs.get()
        );
    }

    /// An effect whose initial body writes the signal it read must not panic.
    ///
    /// This is the reachable double-borrow. `effect` used to run the initial body
    /// with `CURRENT_EFFECT` set but with the re-entrancy guard *not* raised, so
    /// a body that read a signal and then wrote it re-entered `flush_queue`,
    /// which popped the effect currently executing and borrowed its body again
    /// while the outer borrow was still live. No `#[should_panic]` here: before
    /// the fix this test simply dies with a `BorrowMutError`.
    ///
    /// The write happens only on the first run so the effect converges instead of
    /// re-enqueueing itself forever — `set` enqueues subscribers unconditionally,
    /// so a body that always wrote would loop inside the next flush. The
    /// subscription is still checked afterwards, because raising the guard must
    /// not cost the effect the dependency it collected.
    #[test]
    fn an_initial_body_that_writes_the_signal_it_reads_does_not_panic() {
        let sig = Rc::new(Signal::new(0u32));
        let runs = Rc::new(Cell::new(0u32));
        let wrote = Rc::new(Cell::new(false));

        let s = sig.clone();
        let r = runs.clone();
        let w = wrote.clone();
        let handle = effect(move || {
            r.set(r.get() + 1);
            let v = s.get();
            if !w.replace(true) {
                s.set(v + 1);
            }
        });

        assert_eq!(runs.get(), 1, "the initial run completed without panicking");
        assert_eq!(sig.get(), 1, "the initial run's write landed");

        // Still subscribed, so a later set still re-runs it.
        sig.set(5);
        assert_eq!(runs.get(), 2, "the effect must still be subscribed");
        assert_eq!(sig.get(), 5);

        drop(handle);
    }

    /// The growth bound: dead subscribers must not accumulate without limit when
    /// a signal is read but never `set` again.
    ///
    /// This is the risk that deleting the per-read sweep outright would have
    /// introduced rather than fixed. A dead entry is created when an effect's
    /// last strong `Rc` goes away, and nothing removes it again until the next
    /// `set()` on that signal. A signal that is read but never written would
    /// otherwise accumulate one entry per effect that ever subscribed to it,
    /// for the lifetime of the process, and a later subscriber would append to
    /// an ever-growing vector.
    ///
    /// What the bound actually is, stated precisely: dead entries *do*
    /// accumulate between reads, because nothing reclaims them until something
    /// reads. The guarantee is that the next read inside an effect reclaims
    /// them once the vector has reached `SUBSCRIBER_SWEEP_AT` — not that they
    /// never appear.
    ///
    /// Fails without the bounded sweep: each round would leave `BATCH` dead
    /// entries plus one, so the first round already exceeds the threshold.
    ///
    /// In-file because `subscribers` is private, on the same reasoning as
    /// `stopped_effects_store_stays_bounded` above — exposing a `pub` accessor
    /// purely so an integration test could read it would widen the public API.
    #[test]
    fn dead_subscribers_are_reclaimed_when_a_signal_is_never_set_again() {
        // Twice the threshold, so the final read of a round is the one that
        // crosses it.
        const BATCH: usize = SUBSCRIBER_SWEEP_AT * 2;
        const ROUNDS: usize = 5;

        let sig = Rc::new(Signal::new(0u32));

        for round in 0..ROUNDS {
            // A batch of effects that each subscribe by reading `sig`, then die
            // outright. Dropping the *handle* is what actually kills the `Rc`:
            // both `stop` and `Drop` drop the `EFFECT_STORAGE` reference, and
            // the handle holds the last strong one. Calling `stop()` instead
            // would leave the entry alive, which is the stopped-but-alive
            // tombstone case already covered by `2e02f1d`.
            {
                let mut handles = Vec::with_capacity(BATCH);
                for _ in 0..BATCH {
                    let s = sig.clone();
                    handles.push(effect(move || {
                        s.get();
                    }));
                }
                // `sig` is deliberately never `set`, so the write path never
                // prunes either.
                drop(handles);
            }

            // A later subscriber. Its read is what must reclaim the dead
            // entries — this is the "a later subscriber appends to an
            // ever-growing vector" case from the plan.
            {
                let s = sig.clone();
                let last = effect(move || {
                    s.get();
                });
                drop(last);
            }

            let dead = sig
                .subscribers
                .borrow()
                .iter()
                .filter(|w| w.strong_count() == 0)
                .count();
            assert!(
                dead <= SUBSCRIBER_SWEEP_AT,
                "round {round}: {dead} dead subscribers accumulated with no bound, \
                 so the bounded sweep is not reclaiming them (threshold {SUBSCRIBER_SWEEP_AT})"
            );
        }
    }
}
