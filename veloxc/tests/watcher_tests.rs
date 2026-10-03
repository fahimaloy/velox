//! Tests for the dev server's filesystem watcher.
//!
//! The dev server used to poll: every `LOOP_TIMEOUT` (400 ms) it walked the
//! whole watched tree with `read_dir` and compared `mtime`s. These tests pin the
//! three properties that replaced it, each of which is a regression the old poll
//! could not have had:
//!
//! 1. **Latency.** A save is reported in well under the 400 ms poll period, with
//!    no tree walk in the path.
//! 2. **`target/` exclusion.** `cargo build` writes thousands of files into
//!    `target/` inside the watched tree. Watching it is what exhausts inotify
//!    watches, so the exclusion is load-bearing and is asserted here by
//!    *behaviour* (no event fires) rather than by inspecting a filter list.
//! 3. **Collapse.** `notify` emits create+modify+close_write for one save, so a
//!    burst must collapse to one rebuild without blocking the loop.
//!
//! Plus the two things the plan called out as new failure modes: a watcher error
//! must reach the user without killing the dev server, and a change must be
//! *classified* (`ChangeKind`) rather than merely detected.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use velox_cli::commands::dev::{
    ChangeDebouncer, ChangeKind, DevCmd, DirWatcher, WatchReaction, classify_change,
    react_to_watch_error, watch_roots,
};

static SEQ: AtomicU32 = AtomicU32::new(0);

/// A self-deleting temp directory.
///
/// `tempfile` is not a dependency of this crate (adding one would churn the
/// lockfile mid-programme), so the handful of lines it would provide are inlined
/// — the same choice the in-crate tests in `commands::dev` already make.
struct TempTree(PathBuf);

impl TempTree {
    fn new() -> Self {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let p =
            std::env::temp_dir().join(format!("velox-watch-tests-{}-{}", std::process::id(), n));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("create temp tree");
        Self(p)
    }

    /// Create `rel` (with any parent directories).
    fn write(&self, rel: &str, contents: &str) {
        let p = self.0.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(&p, contents).expect("write file");
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A minimal SFC with the given blocks, for classification tests.
fn sfc(template: &str, script: &str, style: &str) -> String {
    format!(
        "<template>\n{template}\n</template>\n<script setup>\n{script}\n</script>\n<style>\n{style}\n</style>\n"
    )
}

// ---------------------------------------------------------------------------
// 1. Latency — the whole point of the change
// ---------------------------------------------------------------------------

/// A save must be reported in well under 100 ms.
///
/// The old implementation could not pass this: it reported a change only on the
/// next `LOOP_TIMEOUT` (400 ms) tick, so the floor was one full poll period.
/// The bound is deliberately far above what a real inotify event costs
/// (single-digit milliseconds) and far below the 400 ms it replaced, so it does
/// not flake on a loaded CI box while still being impossible for a poller.
#[test]
fn a_save_is_reported_in_well_under_100ms() {
    let tree = TempTree::new();
    let (tx, rx) = mpsc::channel();
    let _watcher = DirWatcher::start(tree.path(), tx);

    let start = Instant::now();
    tree.write("App.vx", "<template><p>hi</p></template>");
    let event = rx
        .recv_timeout(Duration::from_millis(500))
        .expect("watcher must report the save");
    let elapsed = start.elapsed();

    let DevCmd::FileChanged { path, .. } = event else {
        panic!("expected FileChanged, got {event:?}");
    };
    assert_eq!(
        path,
        PathBuf::from("App.vx"),
        "event carries the changed path"
    );

    assert!(
        elapsed < Duration::from_millis(100),
        "save reported after {elapsed:?}; the 400 ms poll must be gone, this must be \
         an inotify notification (target: under 100 ms)"
    );
}

/// A modification to an *existing* file fires too, not just creation.
///
/// Without this, deleting the create branch of the filter would make the
/// latency test above still pass while breaking every real edit-after-save
/// cycle, which is the case the dev server exists for.
#[test]
fn modifying_an_existing_file_is_reported() {
    let tree = TempTree::new();
    tree.write("App.vx", "<template><p>v1</p></template>");

    let (tx, rx) = mpsc::channel();
    let _watcher = DirWatcher::start(tree.path(), tx);

    tree.write("App.vx", "<template><p>v2</p></template>");
    let event = rx
        .recv_timeout(Duration::from_millis(500))
        .expect("watcher must report a modification");
    let DevCmd::FileChanged { path, .. } = event else {
        panic!("expected FileChanged, got {event:?}");
    };
    assert_eq!(path, PathBuf::from("App.vx"));
}

// ---------------------------------------------------------------------------
// 2. target/ exclusion — load-bearing, proven by behaviour
// ---------------------------------------------------------------------------

/// Creating a file under `target/` must fire **no** event.
///
/// This is the assertion that can actually fail, and it is why the test writes
/// into the tree rather than asserting on a filter list: the dev server normally
/// watches `<project>/src`, but falls back to the project root when there is no
/// `src/`, and in *any* scaffolded Velox project `target/` sits at the root
/// beside `Cargo.toml`. `cargo build` writes thousands of files there on every
/// rebuild, so an unfiltered watcher both floods the event channel and burns the
/// inotify watch budget — the exact resource whose exhaustion is this task's
/// documented new failure mode.
///
/// **The directories are created before the watcher starts, and a control
/// directory is written to in the same test.** That structure is load-bearing,
/// and the first draft of this test did not have it and passed for the wrong
/// reason — see `3.1-mutation-results.log`, mutation M1: deleting `"target"`
/// from `IGNORED` did **not** turn it red.
///
/// Creating `target/debug/deps/file.rlib` in one burst *after* the watcher
/// starts races inotify's own add-watch step. The file is written before
/// `notify` has installed watches for the directories that were just created, so
/// the write is never observed and the test passes whether or not the filter
/// exists.
///
/// With the tree pre-existing, inotify already holds real watches, and the
/// `keep/` control proves it: a write into a non-ignored pre-existing directory
/// in the same tree, at the same moment, *is* observed. Same timing, same watch
/// coverage — the filter is the only difference.
#[test]
fn a_change_under_target_is_not_reported() {
    let tree = TempTree::new();
    // Materialise the directories so inotify has real watches before the
    // watcher starts, then drop the placeholder files.
    tree.write("target/debug/deps/placeholder", "");
    tree.write("keep/placeholder", "");
    let _ = std::fs::remove_file(tree.path().join("target/debug/deps/placeholder"));
    let _ = std::fs::remove_file(tree.path().join("keep/placeholder"));

    let (tx, rx) = mpsc::channel();
    let _watcher = DirWatcher::start(tree.path(), tx);

    tree.write("target/debug/deps/libfoo-9f8e7d.rlib", "junk");
    tree.write("target/debug/.fingerprint/bar/bin-counter", "junk");

    // 250 ms is many multiples of a real inotify event's latency, and is only
    // there to give a false positive time to arrive.
    let leaked = rx.recv_timeout(Duration::from_millis(250));
    assert!(
        leaked.is_err(),
        "target/ must be excluded, but got {leaked:?}"
    );

    // The control: a pre-existing, non-ignored directory in the same tree. If
    // this does NOT fire, the assertion above proved nothing about the filter.
    tree.write("keep/observed.txt", "x");
    let event = rx
        .recv_timeout(Duration::from_millis(500))
        .expect("control: a write into a watched non-ignored dir must be observed");
    assert!(
        matches!(event, DevCmd::FileChanged { .. }),
        "control expected a FileChanged, got {event:?}"
    );
}

/// A nested `target/` is excluded too, not just one at the watched root.
///
/// `target/` appears at the root of a Velox project, but the fallback watch root
/// and nested workspace layouts put it deeper, and a name check applied only to
/// the last component would silently miss those. Same pre-create-and-control
/// structure as the test above, for the same reason.
#[test]
fn a_nested_target_directory_is_not_reported() {
    let tree = TempTree::new();
    tree.write("sub/project/target/debug/placeholder", "");
    let _ = std::fs::remove_file(tree.path().join("sub/project/target/debug/placeholder"));

    let (tx, rx) = mpsc::channel();
    let _watcher = DirWatcher::start(tree.path(), tx);

    tree.write("sub/project/target/debug/artifact.txt", "junk");

    let leaked = rx.recv_timeout(Duration::from_millis(250));
    assert!(
        leaked.is_err(),
        "a nested target/ must be excluded, but got {leaked:?}"
    );

    // Control: a write beside the excluded tree is observed.
    tree.write("sub/project/observed.txt", "x");
    let event = rx
        .recv_timeout(Duration::from_millis(500))
        .expect("control: a write beside the excluded tree must be observed");
    assert!(
        matches!(event, DevCmd::FileChanged { .. }),
        "control expected a FileChanged, got {event:?}"
    );
}

/// `.git` and dot-directories stay excluded, and a real source file still gets
/// through — together, so the exclusion tests above cannot pass by filtering
/// everything.
///
/// Pre-creates the dot-directories before the watcher starts, for the same reason
/// as `a_change_under_target_is_not_reported`: writing `.git/HEAD` into a
/// directory that did not exist yet races inotify's add-watch step, so the write
/// is never observed and this test passes with the dotfile filter deleted.
/// Mutation M12 in `3.1-mutation-results.log` is exactly that false green.
#[test]
fn ignored_paths_are_excluded_but_source_files_are_not() {
    let tree = TempTree::new();
    tree.write(".git/placeholder", "");
    tree.write(".vscode/placeholder", "");
    tree.write(".cache/placeholder", "");
    let _ = std::fs::remove_file(tree.path().join(".git/placeholder"));
    let _ = std::fs::remove_file(tree.path().join(".vscode/placeholder"));
    let _ = std::fs::remove_file(tree.path().join(".cache/placeholder"));

    let (tx, rx) = mpsc::channel();
    let _watcher = DirWatcher::start(tree.path(), tx);

    tree.write(".git/HEAD", "ref: refs/heads/main");
    tree.write(".vscode/settings.json", "{}");
    // `.cache` is deliberately NOT one of the names in `IGNORED`. Without it
    // this test only exercised the three hard-coded names — and deleting the
    // general dot-directory rule from `is_ignored` left it green (mutation M12
    // in `3.1-mutation-results.log`), because `IGNORED` lists ".git" and
    // ".vscode" as literal strings. This is what pins the *rule* rather than
    // the list, which is the part that has to keep working for the next
    // dot-directory nobody thought to enumerate (`.venv`, `.turbo`, `.cache`).
    tree.write(".cache/build.bin", "junk");
    let leaked = rx.recv_timeout(Duration::from_millis(250));
    assert!(
        leaked.is_err(),
        "dot-directories must be excluded, got {leaked:?}"
    );

    // Control: the watch root itself is watched, so a real edit is observed.
    tree.write("App.vx", "<template><p>hi</p></template>");
    let event = rx
        .recv_timeout(Duration::from_millis(500))
        .expect("control: a real source edit must still be reported");
    let DevCmd::FileChanged { path, .. } = event else {
        panic!("expected FileChanged, got {event:?}");
    };
    assert_eq!(path, PathBuf::from("App.vx"));
}

// ---------------------------------------------------------------------------
// 3. Debounce — collapse without blocking
// ---------------------------------------------------------------------------

/// A burst of raw events for one save collapses to exactly one rebuild.
///
/// `notify` reports create + modify + close_write for a single editor save, so
/// without a collapse every keystroke-save costs three `cargo build`s. The
/// collapse must be *non-blocking*: the debouncer is a deadline compared
/// against a caller-supplied `Instant`, so this test drives time explicitly
/// instead of sleeping.
#[test]
fn a_burst_collapses_to_one_rebuild() {
    let window = Duration::from_millis(50);
    let t0 = Instant::now();
    let mut d = ChangeDebouncer::new();

    // Three raw events for one save, each resetting the deadline.
    d.record(t0, window);
    d.record(t0 + Duration::from_millis(5), window);
    d.record(t0 + Duration::from_millis(10), window);

    assert!(
        !d.take_if_due(t0 + Duration::from_millis(20)),
        "must not fire while events are still arriving"
    );
    assert!(
        d.take_if_due(t0 + Duration::from_millis(61)),
        "the burst must collapse to one rebuild once quiet for the window"
    );
    assert!(
        !d.take_if_due(t0 + Duration::from_millis(62)),
        "one burst must produce exactly one rebuild, not a repeat"
    );
}

/// A later save, after the first has been rebuilt, rebuilds again — the collapse
/// must not swallow genuine subsequent changes.
#[test]
fn a_second_burst_after_the_first_rebuilds_again() {
    let window = Duration::from_millis(50);
    let t0 = Instant::now();
    let mut d = ChangeDebouncer::new();

    d.record(t0, window);
    assert!(d.take_if_due(t0 + Duration::from_millis(51)));

    // A genuinely new edit, well after the window.
    d.record(t0 + Duration::from_millis(500), window);
    assert!(d.take_if_due(t0 + Duration::from_millis(551)));
}

/// The loop must be able to wait *exactly* the remaining debounce time and no
/// more, so the collapse adds no latency beyond the window itself.
///
/// This is what replaces the old 150 ms `thread::sleep` in the change path: the
/// loop blocks on the channel for the remaining window, and every other source
/// of work (a keystroke, a finished build) still wakes it early.
#[test]
fn wait_returns_only_the_remaining_window() {
    let window = Duration::from_millis(50);
    let t0 = Instant::now();
    let mut d = ChangeDebouncer::new();
    d.record(t0, window);

    let remaining = d
        .wait(t0 + Duration::from_millis(30))
        .expect("armed debouncer must report a remaining wait");
    assert_eq!(remaining, Duration::from_millis(20));

    // Past the deadline the loop must still be told to wake *soon*, not "nothing
    // pending". Returning `None` here would make the caller fall back to
    // `LOOP_TIMEOUT` (400 ms) and delay the rebuild by a whole poll period —
    // reintroducing exactly the latency this task removes. So the bound is
    // clamped to a millisecond, never to "wait forever".
    let past = d
        .wait(t0 + Duration::from_millis(60))
        .expect("a pending burst must always bound the wait");
    assert!(
        past <= Duration::from_millis(1),
        "a pending burst must never yield a long wait, got {past:?}"
    );

    // Once the burst is consumed there is genuinely nothing pending.
    assert!(d.take_if_due(t0 + Duration::from_millis(60)));
    assert_eq!(d.wait(t0 + Duration::from_millis(60)), None);
}

// ---------------------------------------------------------------------------
// 4. Watcher errors must not kill the dev server
// ---------------------------------------------------------------------------

/// An inotify failure is reported to the user and the loop keeps running.
///
/// On Linux `notify` uses inotify, which is a *finite* kernel resource:
/// exhausting `fs.inotify.max_user_watches` makes `watcher.watch()` fail with
/// "No space left on device". The old `read_dir` poll had no such limit, so this
/// is a genuinely new failure mode — and a silently dead watcher is strictly
/// worse than a slow working one, because the dev server would sit there
/// claiming to hot-reload and never reload. The contract is: report, stay alive.
#[test]
fn a_watch_error_is_reported_and_never_stops_the_server() {
    let message = "Failed to watch /home/u/app/src: No space left on device \
                   (inotify instance limit reached)";
    match react_to_watch_error(message) {
        WatchReaction::Report {
            message: reported,
            fatal: false,
        } => {
            assert!(
                reported.contains("No space left on device"),
                "the OS error must reach the user verbatim, got {reported:?}"
            );
            assert!(
                reported.contains("fs.inotify.max_user_watches"),
                "the report must name the remedy, got {reported:?}"
            );
            assert!(
                reported.contains("fs.inotify.max_user_instances"),
                "the report must name the remedy, got {reported:?}"
            );
        }
        other => panic!("a watch error must be non-fatal and must carry the remedy; got {other:?}"),
    }
}

/// The loop's own liveness contract: receiving a `WatchError` on the command
/// channel does not consume the change channel, and a real file change is still
/// delivered afterwards.
///
/// This is the end-to-end version of the previous test — it drives the actual
/// `DevCmd` channel the dev loop reads, so it catches a wiring mistake where the
/// error is reported but the loop then stops listening for changes.
#[test]
fn the_watcher_keeps_delivering_after_an_error_is_reported() {
    let tree = TempTree::new();
    let (tx, rx) = mpsc::channel();
    let _watcher = DirWatcher::start(tree.path(), tx.clone());

    // A watcher error arriving on the loop's own channel.
    tx.send(DevCmd::WatchError {
        message: "synthetic".into(),
    })
    .expect("the error must be sendable on the loop's channel");

    // The loop is still alive: a real change is still reported. The synthetic
    // error is still sitting in the channel, so drain until the change arrives
    // rather than assuming it is next.
    tree.write("App.vx", "<template><p>after</p></template>");
    let deadline = Instant::now() + Duration::from_millis(500);
    let mut saw_change = false;
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(DevCmd::FileChanged { .. }) => {
                saw_change = true;
                break;
            }
            // The synthetic error is still queued; the point of the test is that
            // it does not stop the change from arriving behind it.
            Ok(DevCmd::WatchError { .. }) => continue,
            Ok(other) => panic!("unexpected command before the change: {other:?}"),
            Err(_) => continue,
        }
    }
    assert!(
        saw_change,
        "a change after a watch error must still be reported"
    );
}

// ---------------------------------------------------------------------------
// 5. Classification — a <style> edit must not trigger `cargo build`
// ---------------------------------------------------------------------------

/// A change confined to `<style>` is `StyleOnly`.
///
/// This is the prerequisite for task 3.2 (live stylesheet swap) and the single
/// biggest HMR win available: CSS edits are the most frequent edit in any UI
/// codebase, and today each one costs a full `cargo build` plus an app restart.
#[test]
fn a_style_only_edit_is_classified_as_style_only() {
    let before = sfc("<p>hi</p>", "let n = 0;", "p { color: red; }");
    let after = sfc("<p>hi</p>", "let n = 0;", "p { color: blue; }");
    assert_eq!(classify_change(&before, &after), ChangeKind::StyleOnly);
}

/// A change confined to `<template>` is `TemplateOnly`.
#[test]
fn a_template_only_edit_is_classified_as_template_only() {
    let before = sfc("<p>hi</p>", "let n = 0;", "p { color: red; }");
    let after = sfc("<p>bye</p>", "let n = 0;", "p { color: red; }");
    assert_eq!(classify_change(&before, &after), ChangeKind::TemplateOnly);
}

/// A change confined to `<script>` is `Script`.
///
/// `Script` is the conservative kind: it is what forces a real `cargo build`.
/// Every ambiguity resolves *to* it, never away from it — see the ambiguous and
/// unparseable cases below.
#[test]
fn a_script_only_edit_is_classified_as_script() {
    let before = sfc("<p>hi</p>", "let n = 0;", "p { color: red; }");
    let after = sfc("<p>hi</p>", "let n = 1;", "p { color: red; }");
    assert_eq!(classify_change(&before, &after), ChangeKind::Script);
}

/// An edit touching more than one block is `Script`.
///
/// Reporting this as `StyleOnly` would be the dangerous bug: the dev server
/// would swap a stylesheet while the logic that the template renders never
/// recompiled, and the user would be looking at stale code that *looks* live.
#[test]
fn a_multi_block_edit_falls_back_to_script() {
    let before = sfc("<p>hi</p>", "let n = 0;", "p { color: red; }");
    let after = sfc("<p>bye</p>", "let n = 1;", "p { color: blue; }");
    assert_eq!(classify_change(&before, &after), ChangeKind::Script);

    // Style *and* template, with the script untouched.
    let after = sfc("<p>bye</p>", "let n = 0;", "p { color: blue; }");
    assert_eq!(classify_change(&before, &after), ChangeKind::Script);
}

/// A file that does not parse is `Script`.
///
/// A half-typed `</templ` produces unparseable SFC, and a mid-edit file is the
/// *normal* case, not the exception. Classifying an unparseable file as
/// style-only would mean a broken template silently hot-updates as a CSS
/// change; the conservative kind hands it to the compiler, which reports the
/// real error.
#[test]
fn an_unparseable_file_falls_back_to_script() {
    let before = sfc("<p>hi</p>", "let n = 0;", "p { color: red; }");
    let after = "<template><p>hi</p></templ";
    assert_eq!(classify_change(before.as_str(), after), ChangeKind::Script);
    // Also when only the *old* side is broken.
    let before_broken = "<template><p>hi</p></templ";
    assert_eq!(
        classify_change(before_broken, before.as_str()),
        ChangeKind::Script
    );
}

/// A non-SFC file (a plain `.rs`, `Cargo.toml`, …) is `Script`.
///
/// The watcher watches a whole tree, not just `.vx` files, and any of those can
/// change the build. There is no block to parse, so it always takes the
/// conservative path.
#[test]
fn a_non_sfc_file_falls_back_to_script() {
    assert_eq!(
        classify_change("fn main() {}", "fn main() { /* x */ }"),
        ChangeKind::Script
    );
    assert_eq!(
        classify_change("[package]\nname=\"a\"\n", "[package]\nname=\"b\"\n"),
        ChangeKind::Script
    );
}

/// Whitespace-only churn inside `<style>` is still a style change.
///
/// `cargo fmt`-style reindentation of a style block produces a real text diff
/// with no visual effect, and collapsing it as "no change" would mean an
/// inconsistent code path for a case that is entirely normal in an editor.
#[test]
fn reindented_style_content_is_still_style_only() {
    let before = sfc("<p>hi</p>", "let n = 0;", "p { color: red; }");
    let after = sfc("<p>hi</p>", "let n = 0;", "  p { color: red; }");
    assert_eq!(classify_change(&before, &after), ChangeKind::StyleOnly);
}

// ---------------------------------------------------------------------------
// The roots `velox dev` watches
// ---------------------------------------------------------------------------

/// A scaffolded project watches `src/` AND `assets/`.
///
/// `App.vx` and `Modal.vx` point their `<img src>` at `assets/velox-logo.svg`
/// and `assets/velox-logo.png`, so replacing one of those changes what the app
/// draws. Before this, `velox dev` watched `<project>/src` only, and swapping the
/// one file a user is most likely to swap did nothing at all — no rebuild, no
/// message, no error. Watching the directory is the fix; this pins it so it
/// cannot silently go back to one root.
#[test]
fn a_scaffolded_project_watches_src_and_assets() {
    let tree = TempTree::new();
    tree.write("src/App.vx", "<template><p>hi</p></template>");
    tree.write("assets/velox-logo.svg", "<svg></svg>");
    let roots = watch_roots(tree.path());
    assert_eq!(
        roots,
        vec![tree.path().join("src"), tree.path().join("assets")],
        "the project root is deliberately NOT watched — see watch_roots"
    );
}

/// A project with no `assets/` still watches something.
///
/// `velox init` writes both files, but a user deletes directories. The point of
/// listing `assets/` unconditionally is that the root list stays declarative; the
/// watcher, not the caller, decides which roots exist. Asserting the list still
/// names `assets/` is what keeps that decision in one place — if the caller
/// started filtering, this would fail and the decision would have split.
#[test]
fn the_root_list_does_not_filter_out_a_missing_assets_dir() {
    let tree = TempTree::new();
    tree.write("src/App.vx", "<template><p>hi</p></template>");
    let roots = watch_roots(tree.path());
    assert!(
        roots.iter().any(|r| r.ends_with("assets")),
        "assets/ was dropped from the root list because it does not exist yet; \
         that filtering belongs in DirWatcher::start_roots, not here: {:?}",
        roots
    );
    // …and the watcher must not fall over the absence.
    let (tx, rx) = mpsc::channel();
    let _w = DirWatcher::start_roots(&roots, tx);
    tree.write("src/App.vx", "<template><p>changed</p></template>");
    let got = rx
        .recv_timeout(Duration::from_secs(3))
        .expect("a src/ change is still reported when assets/ is absent");
    match got {
        DevCmd::FileChanged { path, .. } => assert_eq!(path, PathBuf::from("App.vx")),
        other => panic!("expected a FileChanged, got {other:?}"),
    }
}

/// A change under `assets/` is reported, with a path relative to that root.
///
/// This is the actual behaviour the extra root buys, asserted end to end rather
/// than by inspecting a list. It also pins the RELATIVE-ness: with two roots the
/// reported path is ambiguous by construction — `App.vx` could be under either —
/// which is why `print_banner` names every root rather than printing one.
#[test]
fn a_change_under_assets_is_reported_against_the_assets_root() {
    let tree = TempTree::new();
    tree.write("src/App.vx", "<template><p>hi</p></template>");
    tree.write("assets/velox-logo.svg", "<svg version=\"1.1\"></svg>");
    let (tx, rx) = mpsc::channel();
    let _w = DirWatcher::start_roots(&watch_roots(tree.path()), tx);

    // Binary content, because that is what swapping a logo actually is. A file
    // that cannot be read as UTF-8 classifies as `Script`, which is correct: it
    // is not a `.vx` sheet and not CSS, so the conservative kind is right.
    let png: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0xff];
    std::fs::write(tree.path().join("assets/velox-logo.png"), png).expect("write png");

    let got = rx
        .recv_timeout(Duration::from_secs(3))
        .expect("a new file under assets/ is reported");
    match got {
        DevCmd::FileChanged { path, kind } => {
            assert_eq!(
                path,
                PathBuf::from("velox-logo.png"),
                "path is relative to assets/"
            );
            assert_eq!(
                kind,
                ChangeKind::Script,
                "an unreadable binary is not a template"
            );
        }
        other => panic!("expected a FileChanged, got {other:?}"),
    }
}

/// Two roots do not cross-contaminate their content caches.
///
/// `classify_observed` diffs new content against what it last saw under a
/// root-relative key. One shared cache across roots would let `src/App.vx` and
/// `assets/App.vx` collide, and a collision is not a near miss — the second file
/// would be classified against the FIRST file's content and report a confident
/// wrong kind. Editing `assets/App.vx` must therefore classify as `Script`, the
/// same as editing `src/App.vx`, rather than as a style change because it happens
/// to differ from an unrelated template.
#[test]
fn a_same_named_file_under_each_root_classifies_independently() {
    let tree = TempTree::new();
    tree.write(
        "src/App.vx",
        &sfc("<p>a</p>", "let n = 0;", "p { color: red; }"),
    );
    tree.write(
        "assets/App.vx",
        &sfc("<p>b</p>", "let n = 0;", "p { color: blue; }"),
    );
    let (tx, rx) = mpsc::channel();
    let _w = DirWatcher::start_roots(&watch_roots(tree.path()), tx);

    // Change only the TEMPLATE half of assets/App.vx. If the two roots shared a
    // cache, the baseline this is diffed against would be src/App.vx's content.
    std::fs::write(
        tree.path().join("assets/App.vx"),
        sfc("<p>b changed</p>", "let n = 0;", "p { color: blue; }"),
    )
    .expect("write");

    let got = rx
        .recv_timeout(Duration::from_secs(3))
        .expect("the assets/ change is reported");
    match got {
        DevCmd::FileChanged { path, kind } => {
            assert_eq!(path, PathBuf::from("App.vx"));
            assert_eq!(
                kind,
                ChangeKind::Script,
                "a template change must classify as Script even though a same-named file \
                 exists under the other root"
            );
        }
        other => panic!("expected a FileChanged, got {other:?}"),
    }
}
