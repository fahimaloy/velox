//! Effect identity: a stopped effect must never be able to silence a later one.
//!
//! # The bug these guard against
//!
//! Effects used to be identified by the heap address of their `Rc`
//! (`Rc::as_ptr() as usize`). `STOPPED_EFFECTS` recorded the addresses of
//! stopped effects and nothing ever removed them. Once an effect was dropped,
//! its allocation was free to be reused, so a newly created effect could land
//! on an address that was already recorded as stopped. `flush_queue` would then
//! skip it forever: the UI rendered, state updated, and the effect simply
//! stopped responding. `STOPPED_EFFECTS` also grew without bound, one entry per
//! effect ever stopped.
//!
//! Both are fixed by `EffectInner::id`, a monotonic id that is never reused,
//! plus pruning of `STOPPED_EFFECTS`. See `signal.rs`.
//!
//! # Honest note on the strength of these tests
//!
//! The deterministic regression guards for this fix are the two in-file unit
//! tests in `signal.rs` (`stopped_effects_store_stays_bounded` and
//! `effect_ids_are_unique_and_increasing`). They fail reliably without the fix
//! and need no allocator behaviour.
//!
//! The tests in *this* file are behaviour-level pins. Against the old
//! heap-address scheme they would only fail if the allocator actually handed a
//! poisoned address back to the new effect, which is likely at 50,000 churn
//! cycles but not guaranteed. They are kept because they are cheap and they
//! describe the user-visible contract in the terms a user would state it, but
//! they should not be read as deterministic before-fix failures.

use std::cell::RefCell;
use std::rc::Rc;

use velox_core::signal::{Signal, effect};

/// A fresh effect on a signal that has already had many effects stopped on it
/// must still re-run when the signal changes.
///
/// This is the plan's behavioural test, kept as specified. All `CHURN` stopped
/// effects subscribe to the *same* signal `s`, so `s.set(1)` walks a subscriber
/// list holding `CHURN` dead `Weak`s plus the live effect — that walk is what
/// re-enqueues the live effect and puts it in front of `flush_queue`, which is
/// exactly where a recycled address would get it skipped.
///
/// It passed against the old implementation too whenever the allocator did not
/// recycle a poisoned address, which is the limitation recorded above.
#[test]
fn a_fresh_effect_after_many_stops_still_runs() {
    const CHURN: usize = 50_000;

    let s = Rc::new(Signal::new(0i32));
    for _ in 0..CHURN {
        let sg = s.clone();
        effect(move || {
            let _ = sg.get();
        })
        .stop();
    }

    let n = Rc::new(RefCell::new(0usize));
    // NOTE: the handle must stay alive across the assertions below. It is bound
    // to a named binding rather than scoped in a block on purpose — see the
    // module header and the task report: `Drop for EffectHandle` marks the
    // effect stopped, so a handle dropped before `s.set(1)` makes this test
    // assert that a *stopped* effect re-runs, which is the opposite of the
    // contract.
    let r = n.clone();
    let sg = s.clone();
    let h = effect(move || {
        let _ = sg.get();
        *r.borrow_mut() += 1;
    });

    assert_eq!(*n.borrow(), 1, "initial run");

    s.set(1);
    assert_eq!(
        *n.borrow(),
        2,
        "effect must re-run; address reuse must not suppress it"
    );

    drop(h);
}

/// A stopped effect does not run again, even while its handle is still held.
///
/// This passed against the old heap-address scheme as well — stopping genuinely
/// worked in isolation. It is here to guard against the fix regressing that, and
/// because it is what makes the pruning predicate in `mark_stopped`
/// load-bearing: a stopped effect removed from `EFFECT_STORAGE` but still held
/// stays subscribed, and only its recorded id stops `set()` from reviving it.
#[test]
fn a_stopped_effect_does_not_run_again() {
    let sig = Rc::new(Signal::new(0u32));
    let runs = Rc::new(RefCell::new(0u32));
    let s = sig.clone();
    let r = runs.clone();
    let handle = effect(move || {
        s.get();
        *r.borrow_mut() += 1;
    });

    assert_eq!(*runs.borrow(), 1);

    handle.stop();
    sig.set(1);
    assert_eq!(*runs.borrow(), 1, "a stopped effect must not run again");

    drop(handle);
}

/// Stopping one effect must not stop an unrelated one created afterwards.
///
/// The distinct failure mode: `stop` left a permanent address-based tombstone,
/// and the very next effect allocated at that address became permanently
/// unschedulable — the UI silently dead rather than merely wrong. Like the
/// churn test above, this only fails against the old scheme when the allocator
/// recycles the address, so it states intent rather than pinning a
/// deterministic pre-fix failure.
#[test]
fn stopping_one_effect_does_not_stop_a_later_one() {
    let stopped_sig = Rc::new(Signal::new(0u32));
    let s = stopped_sig.clone();
    let stopped = effect(move || {
        s.get();
    });
    stopped.stop();
    drop(stopped);

    let live_sig = Rc::new(Signal::new(0u32));
    let runs = Rc::new(RefCell::new(0u32));
    let s = live_sig.clone();
    let r = runs.clone();
    let live = effect(move || {
        s.get();
        *r.borrow_mut() += 1;
    });

    live_sig.set(1);
    assert_eq!(
        *runs.borrow(),
        2,
        "an unrelated effect was silenced by a previous effect's tombstone"
    );

    drop(live);
}
