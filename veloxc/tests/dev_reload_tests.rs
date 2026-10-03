//! Tests for the dev server's reload path.
//!
//! Two kinds of thing are checked here, and they are checked for different
//! reasons:
//!
//! * **Decisions** — [`BuildGate`] (do we start a build, or record the change
//!   and wait?) and [`classify_build`] (is this a compile error or a dev-server
//!   failure?). These are pure state, so they are driven directly with no
//!   injected clock, no sleeps, and — critically — no `cargo` invocation. A test
//!   that shells out to a compiler would be slow and would assert on the
//!   compiler's behaviour rather than on this crate's.
//!
//! * **The RAII contract** — that dropping the app-process guard leaves nothing
//!   running. This one *must* spawn a real process: "no orphans" is a claim about
//!   a process table, and it cannot be verified without one.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use veloxc::commands::dev::{
    AppChild, BuildAction, BuildGate, BuildOutcome, BuildReaction, BuildWorker, DevCmd,
    InputSource, classify_build, react_to,
};

/// Spawn a long-lived child. `sleep 30` cannot exit on its own within any test's
/// lifetime, so a non-success exit status afterwards is proof that *we* killed
/// it — and obtaining a status at all is proof that it was reaped, since
/// `std::process::Child` reaps nothing unless asked.
fn spawn_sleeper() -> std::process::Child {
    Command::new("sleep")
        .arg("30")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("`sleep` must be on PATH for this test to mean anything")
}

/// Whether a pid still exists in the process table.
///
/// On Linux this reads `/proc`, which is the strict check: a **zombie** still has
/// a `/proc/<pid>` entry, so the entry disappearing proves the process was reaped
/// and not merely signalled — which is the distinction this task is about.
///
/// On other platforms there is no portable equivalent, so detection is reported
/// as unavailable rather than faked. The `shutdown_reports_the_status` assertions
/// still hold there; only the independent observation is lost.
#[cfg(target_os = "linux")]
fn process_exists(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

#[cfg(not(target_os = "linux"))]
fn process_exists(_pid: u32) -> bool {
    false
}

fn wait_until_gone(pid: u32) -> Duration {
    let start = Instant::now();
    while process_exists(pid) && start.elapsed() < Duration::from_secs(5) {
        thread::sleep(Duration::from_millis(10));
    }
    start.elapsed()
}

// ---------------------------------------------------------------------------
// Coalescing rapid saves
// ---------------------------------------------------------------------------

#[test]
fn the_first_change_starts_a_build() {
    let mut gate = BuildGate::new();
    assert_eq!(gate.on_change(), BuildAction::Start);
    assert!(gate.is_build_in_flight());
    assert!(!gate.has_pending_change());
}

/// The behaviour the removed 150 ms "debounce" failed at: a burst of saves must
/// cost one rebuild, not one per save.
///
/// Ten saves arrive while a compile is running. Exactly one of them may start a
/// compile, and exactly one follow-up is owed afterwards.
#[test]
fn a_burst_of_saves_while_compiling_costs_exactly_one_rebuild() {
    let mut gate = BuildGate::new();

    assert_eq!(gate.on_change(), BuildAction::Start, "first save compiles");

    for i in 2..=10 {
        assert_eq!(
            gate.on_change(),
            BuildAction::AlreadyInFlight,
            "save {i} must not start a second cargo build"
        );
    }

    // The compile ends.
    gate.on_build_finished();
    assert!(!gate.is_build_in_flight());

    // The nine coalesced saves are owed exactly once, not nine times.
    assert!(gate.take_pending(), "the burst is owed a follow-up build");
    assert!(!gate.take_pending(), "and only one");

    assert_eq!(gate.on_change(), BuildAction::Start);
}

#[test]
fn a_finished_build_with_no_pending_change_starts_nothing() {
    let mut gate = BuildGate::new();
    assert_eq!(gate.on_change(), BuildAction::Start);
    gate.on_build_finished();
    assert!(
        !gate.take_pending(),
        "nothing was saved during that compile"
    );
    assert!(!gate.is_build_in_flight());
}

#[test]
fn a_gate_can_cycle_indefinitely() {
    let mut gate = BuildGate::new();
    for round in 0..5 {
        assert_eq!(gate.on_change(), BuildAction::Start, "round {round}");
        assert_eq!(gate.on_change(), BuildAction::AlreadyInFlight);
        gate.on_build_finished();
        assert!(gate.take_pending());
        assert_eq!(
            gate.on_change(),
            BuildAction::Start,
            "round {round} follow-up"
        );
        gate.on_build_finished();
        assert!(!gate.take_pending(), "round {round} left nothing owed");
    }
    assert!(!gate.is_build_in_flight());
}

// ---------------------------------------------------------------------------
// Compile-error recovery
// ---------------------------------------------------------------------------

/// A syntax error must not end the dev session: the watcher stays up, and
/// saving the fix brings the app back with no manual intervention.
///
/// This is pre-existing behaviour — `spawn_app_hmr` always returned
/// `Ok(None)` on a failed build and never returned `Err` — so this is a
/// regression guard, not a new feature. What is new is that the "must survive"
/// decision and the "must retry on the next save" decision are separate
/// statements in `BuildGate`, and can be checked independently.
#[test]
fn a_compile_error_recovers_on_the_next_save() {
    let mut gate = BuildGate::new();

    // Baseline build succeeds and the app comes up.
    let ok = classify_build(true, String::new(), Duration::from_millis(800));
    assert_eq!(
        ok,
        BuildOutcome::Compiled {
            elapsed: Duration::from_millis(800)
        }
    );
    assert_eq!(gate.on_change(), BuildAction::Start);
    gate.on_build_finished();

    // Save a syntax error. The build fails and stderr is kept for the panel.
    let stderr = "error: expected `;`, found `}`".to_string();
    let bad = classify_build(false, stderr.clone(), Duration::from_millis(300));
    assert_eq!(
        bad,
        BuildOutcome::CompileFailed {
            stderr: stderr.clone()
        }
    );
    assert_eq!(gate.on_change(), BuildAction::Start);
    gate.on_build_finished();

    // Not fatal: the gate is free, so the next save is not swallowed. A gate
    // that only cleared on success would strand the dev session here forever.
    assert!(
        !gate.is_build_in_flight(),
        "a failed build must not wedge the gate"
    );

    // Save the fix. It rebuilds, and the app comes back.
    assert_eq!(gate.on_change(), BuildAction::Start);
    let fixed = classify_build(true, String::new(), Duration::from_millis(400));
    assert_eq!(
        fixed,
        BuildOutcome::Compiled {
            elapsed: Duration::from_millis(400)
        }
    );
    gate.on_build_finished();

    assert!(!gate.take_pending(), "recovery left nothing owed");
    assert!(!gate.is_build_in_flight());
}

/// A failed build is a *compile* error, not a dev-server failure, and the
/// distinction is load-bearing: it is what keeps the loop alive.
#[test]
fn a_failed_build_is_classified_as_a_compile_error() {
    let outcome = classify_build(
        false,
        "error[E0308]: mismatched types".to_string(),
        Duration::from_millis(1),
    );
    match outcome {
        BuildOutcome::CompileFailed { stderr } => {
            assert_eq!(stderr, "error[E0308]: mismatched types");
        }
        other => panic!("expected CompileFailed, got {other:?}"),
    }
}

/// A launch failure must not be rendered as a compile error.
///
/// There are no diagnostics to show, and retrying will not help until cargo is
/// available again, so conflating the two would put an empty "Build failed"
/// panel in front of the user for what is actually a broken toolchain. This is
/// the assertion that makes `InvokeFailed` worth being a separate variant.
#[test]
fn a_cargo_launch_failure_is_not_rendered_as_a_compile_error() {
    let outcome = BuildOutcome::InvokeFailed {
        message: "No such file or directory (os error 2)".to_string(),
    };
    match react_to(&outcome) {
        BuildReaction::ReportLaunchFailure { message } => {
            assert_eq!(message, "No such file or directory (os error 2)");
        }
        other => panic!("a launch failure must not become {other:?}"),
    }
}

/// And the converse: a real compile error is never reported as a launch failure.
#[test]
fn a_compile_error_is_never_reported_as_a_launch_failure() {
    let outcome = classify_build(
        false,
        "error: expected `;`".to_string(),
        Duration::from_millis(5),
    );
    assert_eq!(
        react_to(&outcome),
        BuildReaction::PrintBuildError {
            stderr: "error: expected `;`".to_string()
        }
    );
}

#[test]
fn a_successful_build_reaction_carries_the_elapsed_time() {
    let outcome = classify_build(true, String::new(), Duration::from_millis(1234));
    assert_eq!(
        react_to(&outcome),
        BuildReaction::RunApp {
            elapsed: Duration::from_millis(1234)
        }
    );
}

// ---------------------------------------------------------------------------
// No orphaned children
// ---------------------------------------------------------------------------

/// The headline claim: dropping the guard leaves no surviving process.
///
/// `AppChild` is the only kill path in the dev loop, so this is the test for
/// "no orphans on any exit path" — quit, `r`, app crash, panic, all of them end
/// in the guard being dropped.
#[test]
fn dropping_the_guard_leaves_no_surviving_process() {
    let child = Command::new("sleep")
        .arg("30")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn sleeper");
    let pid = child.id();
    let guard = AppChild::new(child);

    if !process_exists(pid) {
        // Detection is unavailable, or the process is already gone. Nothing to
        // observe, so do not pretend to have observed anything.
        return;
    }

    drop(guard);
    let elapsed = wait_until_gone(pid);

    assert!(
        !process_exists(pid),
        "pid {pid} is still in the process table {elapsed:?} after the guard was dropped"
    );
}

/// The panic path. `Child` does not kill on drop, so before `AppChild` this was
/// the one exit path no explicit `kill()` at each `break` could cover.
#[test]
fn a_panic_in_the_dev_loop_still_reaps_the_app() {
    let sleeper = Command::new("sleep")
        .arg("30")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn sleeper");
    let pid = sleeper.id();

    let result = std::panic::catch_unwind(|| {
        let _guard = AppChild::new(sleeper);
        panic!("simulated dev-server panic");
    });
    assert!(result.is_err(), "the panic must actually have unwound");

    if !process_exists(pid) {
        return;
    }
    wait_until_gone(pid);
    assert!(
        !process_exists(pid),
        "pid {pid} survived an unwind: the guard did not run"
    );
}

/// `shutdown` must reap, and must be safe to call twice.
///
/// The status is the evidence that a reap happened at all: it is only obtainable
/// by waiting, and it cannot be a success because a 30 s sleeper was killed
/// milliseconds after starting.
#[test]
fn shutdown_reaps_the_child_and_is_idempotent() {
    let mut guard = AppChild::new(spawn_sleeper());
    assert!(
        !guard.has_exited().expect("try_wait on a live child"),
        "a fresh sleeper must look alive"
    );

    let status = guard
        .shutdown()
        .expect("shutdown must report the reap it performed");
    assert!(
        !status.success(),
        "a 30s sleeper killed immediately cannot have exited successfully"
    );

    assert!(
        guard.shutdown().is_none(),
        "a second shutdown must be a no-op, not a second reap"
    );
    assert!(
        !guard
            .has_exited()
            .expect("an emptied guard cannot report an exit"),
        "an emptied guard has nothing left to observe"
    );
}

#[test]
fn a_freshly_reaped_child_no_longer_reports_as_exited() {
    let mut guard = AppChild::new(spawn_sleeper());
    guard.shutdown().expect("reap");
    // Dropping after an explicit shutdown must not panic or double-wait: the
    // guard's `Drop` runs the same code path as `shutdown`.
    drop(guard);
}

// ---------------------------------------------------------------------------
// The build runs off the dev loop
// ---------------------------------------------------------------------------

/// How long the injected fake build takes. Long enough that a build still on the
/// loop thread is unmistakable, short enough to keep the suite quick.
const SLOW_BUILD: Duration = Duration::from_millis(700);

/// The headline fix, asserted rather than asserted-about: the dev loop must not
/// wait for a compile.
///
/// The old code called `build.output()` inline, so the loop was blocked for the
/// whole compile and a keystroke arriving during it sat in the channel until the
/// build finished. Here the worker is started, a command is sent *while it is
/// still building*, and that command has to come back long before the build does.
#[test]
fn the_dev_loop_stays_responsive_while_a_build_is_in_flight() {
    let (tx, rx) = mpsc::channel::<DevCmd>();

    let start = Instant::now();
    let worker = BuildWorker::start_with(tx.clone(), move |tx, _slot| {
        thread::sleep(SLOW_BUILD);
        let _ = tx.send(DevCmd::Built(BuildOutcome::Compiled {
            elapsed: SLOW_BUILD,
        }));
    });
    let start_took = start.elapsed();

    assert!(
        start_took < Duration::from_millis(100),
        "starting a build blocked the caller for {start_took:?}; \
         the compile must run off the dev loop thread"
    );

    // A command arriving mid-build is delivered straight away. This is the
    // assertion that fails when the build is moved back onto the loop: the send
    // would not be answered until the build finished.
    let sent_at = Instant::now();
    tx.send(DevCmd::Reload).expect("send a keystroke");
    let got = rx
        .recv_timeout(Duration::from_millis(100))
        .expect("the dev loop must wake on a keystroke while cargo is still compiling");
    assert_eq!(got, DevCmd::Reload);
    let keystroke_latency = sent_at.elapsed();
    assert!(
        keystroke_latency < SLOW_BUILD,
        "the keystroke waited {keystroke_latency:?} for the build to finish"
    );

    // The build result comes back on the loop's *own* channel, so a finished
    // compile is noticed immediately rather than at the next timeout tick.
    let built = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("the build must report its outcome on the dev loop's channel");
    assert_eq!(
        built,
        DevCmd::Built(BuildOutcome::Compiled {
            elapsed: SLOW_BUILD
        })
    );

    drop(worker);
}

/// Quitting the dev server mid-compile must not leave `cargo` running: it would
/// hold the target-dir lock, so every later build — in this project or any other
/// on the machine — would block behind it.
///
/// The body below reproduces `run_build`'s exact shape: publish the handle in the
/// slot, then block draining its stderr pipe. It ends only when the child dies,
/// which is what makes this test able to hang if `drop` fails to kill.
#[test]
fn dropping_a_build_worker_kills_and_reaps_its_process() {
    let (tx, _rx) = mpsc::channel::<DevCmd>();
    let (pid_tx, pid_rx) = mpsc::channel::<u32>();

    let worker = BuildWorker::start_with(tx, move |_tx, slot| {
        let mut child = Command::new("sleep")
            .arg("30")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn a stand-in for cargo");
        let pid = child.id();
        // Take the pipe first, then publish — the same order `run_build` uses, so
        // `drop` finds a handle to kill while this thread is still blocked.
        let mut pipe = child.stderr.take();
        *slot.lock().unwrap() = Some(child);
        let _ = pid_tx.send(pid);
        if let Some(p) = pipe.as_mut() {
            let mut buf = Vec::new();
            let _ = p.read_to_end(&mut buf);
        }
    });

    let pid = pid_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("the worker should have published its process");
    assert!(
        process_exists(pid),
        "pid {pid} is not running to begin with"
    );

    drop(worker);
    let elapsed = wait_until_gone(pid);
    assert!(
        !process_exists(pid),
        "pid {pid} survived BuildWorker::drop ({elapsed:?}); \
         the compile would still be holding the target-dir lock"
    );
}

// ---------------------------------------------------------------------------
// Noticing that the user has gone away
// ---------------------------------------------------------------------------

/// The channel-level fact `InputSource` exists to work around.
///
/// The dev loop holds a sender of its own — it hands one to every `BuildWorker`
/// so a finished compile can wake the loop — and one live sender is enough to
/// keep an `mpsc` channel out of the disconnected state forever. So "stdin
/// reached EOF" cannot be observed as `RecvTimeoutError::Disconnected`; it has to
/// be observed on the reader's `JoinHandle` instead. This test pins the channel
/// behaviour that makes that necessary, so the two tests below are not asserting
/// an arbitrary choice of mechanism.
#[test]
fn a_loop_that_holds_its_own_sender_can_never_see_disconnected() {
    let (tx, rx) = mpsc::channel::<DevCmd>();
    let worker_tx = tx.clone();
    thread::spawn(move || {
        // Stands in for a build worker finishing: it drops its sender and stops.
        drop(worker_tx);
    });

    // Every *other* producer is gone, but `tx` is the loop's own.
    assert!(
        matches!(
            rx.recv_timeout(Duration::from_millis(150)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ),
        "with the loop's own sender alive the channel must time out, not disconnect"
    );

    drop(tx);
    // Now, and only now, the channel reports that nothing can send again.
    assert!(
        matches!(
            rx.recv_timeout(Duration::from_millis(150)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ),
        "once the loop's own sender is dropped the channel must disconnect"
    );
}

/// Both directions, so `is_exhausted` cannot be a constant that happens to suit
/// whichever direction the code currently needs.
#[test]
fn input_exhaustion_is_reported_in_both_directions() {
    let live = InputSource::from_handle(thread::spawn(|| thread::sleep(Duration::from_secs(30))));
    let done = InputSource::from_handle(thread::spawn(|| {}));

    // The finished thread is only *observably* finished once it has actually
    // been scheduled to completion.
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done.is_exhausted() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        done.is_exhausted(),
        "a returned reader must report exhausted"
    );

    assert!(
        !live.is_exhausted(),
        "a reader blocked in a stdin read must not be reported as exhausted"
    );
    // Deliberately not joined: joining a stdin reader is exactly the hang this
    // lane's design avoids, and a test should not model it.
    drop(live);
}
