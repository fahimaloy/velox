// velox-core/benches/signal_read.rs
//
// Benchmark the read path of `Signal::get` — specifically the subscriber
// bookkeeping that `get` does for a read that happens *inside* a live effect.
//
// The subtlety this file exists to get right: `get` only touches
// `subscribers` when `CURRENT_EFFECT` is `Some` (`signal.rs:278`). A read from
// outside any effect falls straight through to `self.value.borrow().clone()`
// and never reaches the subscriber block at all. So `b.iter(|| s.get())` from
// the top level of a bench measures the O(1) early-out, and would "verify" a
// change to the subscriber path by benchmarking code the change never touches.
//
// Every measurement here therefore drives its reads from inside an effect body.
//
// The measured shape is:
//
//   - `SUBSCRIBERS` effects each read `s` once, so `s`'s subscriber vector holds
//     that many live entries.
//   - one probe effect reads `s` `READS_PER_RUN` times per run. Those reads are
//     the work under test: each one walks the subscriber vector to check for a
//     duplicate registration.
//   - `trigger.set` re-runs only the probe effect (the subscriber effects never
//     read `trigger`), so the timed region is dominated by the reads.

use std::hint::black_box;
use std::rc::Rc;

use criterion::{Criterion, criterion_group, criterion_main};
use velox_core::signal::{Signal, effect};

/// Reads performed by the probe effect on each run. Large enough that the
/// subscriber walk dominates the cost of re-running the effect (queue push,
/// borrow, scheduler entry) by a wide margin.
const READS_PER_RUN: usize = 2_000;

fn bench_read_inside_effect(subscribers: usize, c: &mut Criterion) {
    let s = Rc::new(Signal::new(0i64));
    let trigger = Rc::new(Signal::new(0i64));

    // Populate `s`'s subscriber vector with live entries.
    let filler_effects: Vec<_> = (0..subscribers)
        .map(|_| {
            let sg = s.clone();
            effect(move || {
                black_box(sg.get());
            })
        })
        .collect();

    // The probe: reads `s` repeatedly while `CURRENT_EFFECT` is set. It reads
    // `trigger` too, so that a `trigger.set` re-runs it.
    let probe = {
        let sg = s.clone();
        let tg = trigger.clone();
        effect(move || {
            black_box(tg.get());
            let mut acc = 0i64;
            for _ in 0..READS_PER_RUN {
                acc += sg.get();
            }
            black_box(acc);
        })
    };

    c.bench_function(
        &format!("read_inside_effect/subscribers_{subscribers}"),
        |b| {
            b.iter(|| trigger.set(black_box(1i64)));
        },
    );

    drop(probe);
    drop(filler_effects);
}

fn bench_reads(c: &mut Criterion) {
    // 32 subscribers: below the sweep threshold, so the per-read path is
    // exercised without the bounded backstop firing.
    bench_read_inside_effect(32, c);
    // 64 subscribers: at the threshold, so this also shows what the backstop
    // costs when a signal really does have this many live subscribers.
    bench_read_inside_effect(64, c);
}

criterion_group!(benches, bench_reads);
criterion_main!(benches);
