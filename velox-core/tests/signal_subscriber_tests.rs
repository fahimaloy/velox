// velox-core/tests/signal_subscriber_tests.rs
//
// Behavioural guard for the bounded subscriber sweep in `Signal::get`.
//
// The sweep is `subs.retain(|w| w.strong_count() > 0)`, which keeps live entries
// and drops dead ones. The failure mode that matters — and the one Task 1.2
// shipped once — is a prune written so that it discards *live* entries.
//
// A note on where that failure actually shows up, because it is not where I
// first assumed. The sweep fires during *registration*, not during the flush:
// the 65th effect to read the signal is the one that crosses
// `SUBSCRIBER_SWEEP_AT`, so with `SUBSCRIBERS = 100` the list is wiped and
// re-appended to 36 separate times before any `set` happens. Falsifying the
// prune as `retain(|_| false)` therefore failed on the FIRST `set`
//
//     effect 1 (stopped=false) should have run 2 time(s) after one set
//     left: 1  right: 2
//
// and not on the second as I had written in a comment here. Corrected below.
//
// The second `set` is kept anyway, because it is a genuine end-to-end check
// that the subscriber list is intact after repeated writes — a prune that only
// misbehaves later, on the flush path rather than at registration, would be
// caught by it and not by the first.

use std::cell::Cell;
use std::rc::Rc;

use velox_core::signal::{Signal, effect};

/// Comfortably above `SUBSCRIBER_SWEEP_AT` (64), so the sweep actually fires
/// while these effects are reading. If this were below the threshold the sweep
/// would never run and the test would be vacuous.
const SUBSCRIBERS: usize = 100;

#[test]
fn the_sweep_discards_dead_subscribers_and_keeps_live_ones_across_repeated_writes() {
    let sig = Rc::new(Signal::new(0u32));
    let runs: Vec<Rc<Cell<u32>>> = (0..SUBSCRIBERS).map(|_| Rc::new(Cell::new(0))).collect();

    let mut handles = Vec::with_capacity(SUBSCRIBERS);
    for r in &runs {
        let (s, counter) = (sig.clone(), r.clone());
        handles.push(effect(move || {
            s.get();
            counter.set(counter.get() + 1);
        }));
    }

    // Every effect ran once on registration.
    for r in &runs {
        assert_eq!(r.get(), 1, "the effect runs once on registration");
    }

    // Stop every third effect, but keep the handle alive. A stopped-but-alive
    // effect still has a live `Weak` in the subscriber list, so the sweep must
    // *keep* it — it is skipped by id in `flush_queue`, not by being pruned.
    // Pruning it here would be wrong, and the `set(1)` assertions below are what
    // would notice.
    for (i, h) in handles.iter().enumerate() {
        if i % 3 == 0 {
            h.stop();
        }
    }

    sig.set(1);

    // Every effect that is not stopped must have run; every stopped one must
    // not have.
    for (i, r) in runs.iter().enumerate() {
        let expected = if i % 3 == 0 { 1 } else { 2 };
        assert_eq!(
            r.get(),
            expected,
            "effect {i} (stopped={}) should have run {expected} time(s) after one set",
            i % 3 == 0
        );
    }

    // A second write, to confirm the subscriber list is still intact rather
    // than truncated by anything that runs on the flush path. See the note at
    // the top of this file: the *live-discarding* breakage above is already
    // caught by `set(1)`, because the sweep fires during registration.
    sig.set(2);

    for (i, r) in runs.iter().enumerate() {
        let expected = if i % 3 == 0 { 1 } else { 3 };
        assert_eq!(
            r.get(),
            expected,
            "effect {i} (stopped={}) should have run {expected} time(s) after two sets; \
             a live subscriber was discarded by the sweep",
            i % 3 == 0
        );
    }
}

#[test]
fn dropped_subscribers_stop_receiving_notifications() {
    // The complementary case: a subscriber whose handle is gone has a dead
    // `Weak`, and those are exactly what the sweep is allowed to drop. Dropping
    // a subscriber must not disturb the ones that are still alive.
    let sig = Rc::new(Signal::new(0u32));

    let live_counter = Rc::new(Cell::new(0u32));
    let dead_counter = Rc::new(Cell::new(0u32));

    let live_handle = {
        let (s, c) = (sig.clone(), live_counter.clone());
        effect(move || {
            s.get();
            c.set(c.get() + 1);
        })
    };
    let dead_handle = {
        let (s, c) = (sig.clone(), dead_counter.clone());
        effect(move || {
            s.get();
            c.set(c.get() + 1);
        })
    };

    assert_eq!(live_counter.get(), 1);
    assert_eq!(dead_counter.get(), 1);

    drop(dead_handle);

    sig.set(1);
    assert_eq!(live_counter.get(), 2, "the live subscriber still runs");
    assert_eq!(
        dead_counter.get(),
        1,
        "the dropped subscriber does not run again"
    );

    drop(live_handle);
}
