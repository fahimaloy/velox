//! Repro for R-C1 / C-H7: HMR deadlock due to Mutex<Receiver> held across blocking recv().
//!
//! These tests are deliberately **failing before the fix** and passing after.
//! They inspect the source of `velox-renderer/src/lib.rs` and `velox-renderer/src/hmr.rs`
//! for the known buggy patterns, plus a live deadlock simulation.

use std::fs;
use std::path::Path;

fn lib_rs() -> String {
    // Try a few relative locations: when run via `cargo test -p velox-renderer`,
    // cwd is the crate dir; when run via workspace, cwd is the workspace root.
    for p in [
        "src/lib.rs",
        "velox-renderer/src/lib.rs",
        "../velox-renderer/src/lib.rs",
    ] {
        if Path::new(p).exists() {
            return fs::read_to_string(p).unwrap();
        }
    }
    panic!("cannot find velox-renderer/src/lib.rs");
}

fn hmr_rs() -> String {
    for p in [
        "src/hmr.rs",
        "velox-renderer/src/hmr.rs",
        "../velox-renderer/src/hmr.rs",
    ] {
        if Path::new(p).exists() {
            return fs::read_to_string(p).unwrap();
        }
    }
    panic!("cannot find velox-renderer/src/hmr.rs");
}

/// R-C1: Mutex<Receiver> held across blocking recv() in the HMR forwarding thread.
/// Before fix lib.rs contains:
///   while let Ok(rx) = hmr_rx_for_thread.lock() {
///       if rx.recv().is_ok() { ... }
/// This holds the MutexGuard across a blocking recv(), deadlocking the main thread's try_recv().
#[test]
fn hmr_deadlock_mutex_held_across_recv() {
    let src = lib_rs();
    // Look for the buggy pattern: lock() guard held across recv()
    let has_bug = src.contains("hmr_rx_for_thread.lock()") && src.contains("rx.recv()")
        // The fixed code moves Receiver ownership to the HMR thread and uses
        // `hmr_rx.recv()` without a Mutex, or uses try_recv polling, or replaces the Mutex content.
        // If we see the thread spawning with `while let Ok(rx) = hmr_rx_for_thread.lock()` then recv, it's buggy.
        && src.contains("while let Ok(rx) = hmr_rx_for_thread.lock()");
    assert!(
        !has_bug,
        "BUG R-C1: HMR thread still holds Mutex<Receiver> across blocking recv() — deadlock. Found `while let Ok(rx) = hmr_rx_for_thread.lock()` + `rx.recv()`"
    );
}

/// The main-loop side holds the same Mutex across try_recv() (`hmr_rx_for_loop.lock()`).
/// After the fix, the main loop should receive HmrMessage directly via Event::UserEvent(HmrMessage)
/// and NOT call `hmr_rx_for_loop.lock()` / `try_recv()`.
#[test]
fn hmr_main_thread_mutex_try_recv() {
    let src = lib_rs();
    // After fix, there should be no `hmr_rx_for_loop.lock()` in the UserEvent handler.
    // The correct pattern is `Event::UserEvent(msg)` or `Event::UserEvent(HmrMessage)`.
    let has_guard_try_recv = src.contains("hmr_rx_for_loop.lock()") && src.contains("try_recv()");
    assert!(
        !has_guard_try_recv,
        "BUG R-C1: main thread still uses Mutex<Receiver>::try_recv() via `hmr_rx_for_loop.lock()` — should forward HmrMessage via EventLoopProxy::send_event(HmrMessage) instead"
    );
}

/// HotReload handler must recompute **all** targets including input_targets,
/// and must call w.request_redraw(). Before fix, with_hmr's recompute_targets
/// only took click+hover and HotReload missed input_targets + sometimes redraw.
#[test]
fn hmr_hot_reload_recomputes_input_targets_and_redraw() {
    let src = lib_rs();
    // Find the with_hmr section: look for run_window_vnode_skia_with_hmr
    let Some(start) = src.find("run_window_vnode_skia_with_hmr") else {
        panic!("cannot find run_window_vnode_skia_with_hmr");
    };
    let slice = &src[start..];
    // recompute_targets is now unified at module scope (single definition) after 1B fix,
    // so check either local slice or global src for the unified helper with input_targets.
    let with_hmr_has_input = slice.contains("input_targets: &mut Vec<crate::events::InputTarget>")
        || slice.contains("input_targets: &mut Vec<events::InputTarget>")
        || src.contains("fn recompute_targets")
            && src.contains("input_targets: &mut Vec<crate::events::InputTarget>");
    assert!(
        with_hmr_has_input,
        "BUG R-M3: with_hmr recompute_targets missing input_targets — should recompute click+hover+input"
    );
    // HotReload arm should call request_redraw
    // Look for HotReload match arm and ensure it contains request_redraw
    let Some(hot_pos) = slice.find("HmrMessage::HotReload") else {
        panic!("cannot find HmrMessage::HotReload handler in with_hmr");
    };
    // HotReload arm plus its recompute can be ~3-4k chars; use a larger window.
    let end = (hot_pos + 6000).min(slice.len());
    let hot_slice = &slice[hot_pos..end];
    assert!(
        hot_slice.contains("request_redraw"),
        "BUG: HotReload handler missing w.request_redraw() — HMR update would not trigger a frame"
    );
    // And it should recompute input_targets (collect_input_targets or recompute_targets with input)
    assert!(
        hot_slice.contains("input_targets") || hot_slice.contains("recompute_targets"),
        "BUG: HotReload handler missing input_targets recompute"
    );
}

/// HMR forwarding thread must be wrapped in catch_unwind so a panic doesn't kill the app.
#[test]
fn hmr_thread_has_catch_unwind() {
    let lib = lib_rs();
    let hmr = hmr_rs();
    // At least one of the HMR-related threads should have catch_unwind.
    // lib.rs's HMR proxy thread and hmr.rs's run_hmr_client thread.
    let lib_has = lib.contains("catch_unwind");
    let hmr_has = hmr.contains("catch_unwind");
    assert!(
        lib_has || hmr_has,
        "BUG: HMR thread missing catch_unwind — a panic in HMR would bring down the app. Expected std::panic::catch_unwind in lib.rs HMR thread or hmr.rs"
    );
    // Specifically, the HMR proxy thread in lib.rs should have it.
    let Some(pos) = lib.find("run_window_vnode_skia_with_hmr") else {
        panic!("cannot find with_hmr fn");
    };
    let slice = &lib[pos..];
    assert!(
        slice.contains("catch_unwind"),
        "BUG: run_window_vnode_skia_with_hmr HMR forwarding thread missing catch_unwind wrapper"
    );
}

/// Live deadlock simulation: the OLD pattern (Mutex<Receiver> held across recv)
/// deadlocks the main thread's try_lock within a short timeout.
/// This test demonstrates the bug exists when using the old pattern, and is
/// useful as a regression guard even after the fix (it simulates the old code).
#[test]
fn live_deadlock_simulation_old_pattern_blocks() {
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::Duration;

    let (tx, rx) = mpsc::channel::<u32>();
    let shared = Arc::new(Mutex::new(rx));
    let shared2 = Arc::clone(&shared);

    // Spawn a thread that holds the lock across blocking recv() — the buggy pattern.
    let t = std::thread::spawn(move || {
        let guard = shared2.lock().unwrap();
        // This blocks holding the lock.
        let _ = guard.recv();
    });

    // Give the thread time to acquire the lock and block on recv().
    std::thread::sleep(Duration::from_millis(100));

    // Main thread tries to acquire the same Mutex without blocking.
    // With the bug, this must fail (deadlock would occur if we used lock()).
    let try_locked = shared.try_lock().is_err();

    // Unblock the holder so the test doesn't leak.
    let _ = tx.send(42);
    let _ = t.join();

    assert!(
        try_locked,
        "expected try_lock to fail when another thread holds Mutex across blocking recv() — this proves the old pattern deadlocks"
    );
}

/// After fix, the correct pattern is: Receiver owned solely by HMR thread,
/// main thread receives via EventLoopProxy without any Mutex.
/// Simulate that: one thread owns Receiver, forwards via channel (proxy), no deadlock.
#[test]
fn live_no_deadlock_new_pattern() {
    use std::sync::mpsc;
    use std::time::Duration;

    let (tx, rx) = mpsc::channel::<u32>();
    let (proxy_tx, proxy_rx) = mpsc::channel::<u32>();

    // HMR thread owns rx exclusively, no Mutex, forwards via proxy.
    let h = std::thread::spawn(move || {
        while let Ok(msg) = rx.recv() {
            let _ = proxy_tx.send(msg);
            if msg == 99 {
                break;
            }
        }
    });

    // Give thread time to block on recv().
    std::thread::sleep(Duration::from_millis(50));

    // Main thread is NOT blocked — it can still send and receive via proxy
    // without contending on a Mutex. No try_lock needed.
    tx.send(1).unwrap();
    let got = proxy_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(got, 1);

    tx.send(99).unwrap();
    h.join().unwrap();
    // Ensure proxy still delivers the terminal message.
    let got2 = proxy_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(got2, 99);
}
