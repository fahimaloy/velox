use anyhow::Result;
use notify::{EventKind, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};

/// Directory names that never produce a dev-server rebuild.
///
/// `target/` is load-bearing, not cosmetic. `cargo build` writes thousands of
/// files into it, inside the watched tree; a watcher that descends into it burns
/// the inotify watch budget — the finite kernel resource whose exhaustion is this
/// watcher's documented failure mode (see [`DirWatcher`]). The exclusion is
/// asserted *behaviourally* in `tests/watcher_tests.rs`: a real file written
/// under `target/` must produce no event.
const IGNORED: &[&str] = &["target", ".git", ".vscode", ".idea"];

/// How long the dev loop blocks waiting for the next event before it re-checks
/// whether stdin has gone away.
///
/// This is no longer a *poll interval for file changes* — those arrive over the
/// command channel from [`DirWatcher`] and wake the loop immediately. It remains
/// only a backstop for the one thing the channel cannot report: the stdin
/// reader's death (see [`InputSource`]). Input latency and rebuild latency are
/// no longer quantised to this value.
const LOOP_TIMEOUT: Duration = Duration::from_millis(400);

/// How long the dev loop waits for a burst of filesystem events to go quiet
/// before acting on it.
///
/// `notify` reports create + modify + close_write for a single editor save, so
/// without a collapse every save would cost three `cargo build`s. The collapse
/// is a deadline comparison in the event loop, **not** a sleep: the loop blocks
/// on the command channel for exactly the remaining time, so a keystroke or a
/// finished build still wakes it early. The previous implementation slept a flat
/// 150 ms in the change path itself, which blocked command handling.
const DEBOUNCE_WINDOW: Duration = Duration::from_millis(50);

/// True when `path` (as reported by the watcher, relative to the watch root)
/// must not trigger a rebuild.
///
/// A path is excluded when **any** component is a dot-directory (`.git`,
/// `.vscode`, …) or a name in [`IGNORED`]. Checking every component rather than
/// only the last is what makes the exclusion hold for a nested
/// `sub/project/target/`, not just a `target/` sitting at the watch root.
pub fn is_ignored(path: &Path) -> bool {
    path.components().any(|c| match c {
        std::path::Component::Normal(name) => {
            let n = name.to_string_lossy();
            n.starts_with('.') || IGNORED.contains(&n.as_ref())
        }
        // `CurDir` is the empty/no-op component; everything else (`RootDir`,
        // `Prefix`, `ParentDir`) is not a name we can exclude on.
        _ => false,
    })
}

// ---- filesystem watcher ----------------------------------------------------

/// Which SFC block a change landed in.
///
/// The watcher used to report only *that* something changed, so every edit —
/// including a one-property CSS tweak — forced a full `cargo build` and an app
/// restart. The kind of change determines the response, exactly as in Vite, and
/// every ambiguity resolves to the conservative [`ChangeKind::Script`].
///
/// The bias is deliberate and one-directional. Classifying a multi-block or
/// unparseable edit as cheaper would mean swapping a stylesheet while the
/// template that renders it never recompiled: the user would be looking at stale
/// code that *looks* live. Over-reporting as a `Script` change costs a rebuild
/// the user would have got anyway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// Only `<style>` content differs. Rebuilds nothing.
    StyleOnly,
    /// Only `<template>` content differs.
    TemplateOnly,
    /// Script logic changed, or more than one block, or the file does not parse
    /// as an SFC, or it is not an SFC at all. This is the kind that requires a
    /// real `cargo build`, and the fallback for every case that is not provably
    /// style-only or template-only.
    Script,
}

impl ChangeKind {
    /// Fold two observed changes into the one the dev server should act on.
    ///
    /// Collapsing a burst means several edits can land in one debounce window,
    /// and a style edit plus a script edit in that window is a script change.
    /// Ordering is [`ChangeKind::Script`] > `TemplateOnly` > `StyleOnly`, so the
    /// merge can never make a build cheaper than any of its inputs.
    pub fn merge(self, other: ChangeKind) -> ChangeKind {
        use ChangeKind::*;
        match (self, other) {
            (Script, _) | (_, Script) => Script,
            (TemplateOnly, _) | (_, TemplateOnly) => TemplateOnly,
            (StyleOnly, StyleOnly) => StyleOnly,
        }
    }
}

/// Classify a change by which SFC block it landed in.
///
/// Compares the parsed blocks of the previous and current source rather than
/// grepping for a tag, so an unterminated `</templ` mid-keystroke is detected as
/// broken rather than mistaken for a style-only edit.
///
/// Every unparseable or non-SFC input returns [`ChangeKind::Script`] for the
/// reason given on that variant: a half-written file is the *normal* case while
/// an editor is mid-save, not an exception.
pub fn classify_change(before: &str, after: &str) -> ChangeKind {
    use ChangeKind::*;
    let (Ok(b), Ok(a)) = (velox_sfc::parse_sfc(before), velox_sfc::parse_sfc(after)) else {
        return Script;
    };
    fn block(s: &velox_sfc::Sfc) -> (Option<&str>, Option<&str>, Option<&str>, Option<&str>) {
        // A missing block and an empty one must compare equal, so read them
        // through the same `Option<&str>` shape rather than comparing
        // `Option<&StyleBlock>` (which also needs `PartialEq` on the block
        // types) or letting `None` differ from `Some("")`.
        (
            s.style.as_ref().map(|b| b.content.as_str()),
            s.template.as_ref().map(|b| b.content.as_str()),
            s.script_setup.as_ref().map(|b| b.content.as_str()),
            s.script.as_ref().map(|b| b.content.as_str()),
        )
    }
    let (b_style, b_tpl, b_setup, b_script) = block(&b);
    let (a_style, a_tpl, a_setup, a_script) = block(&a);

    let style = b_style != a_style;
    let template = b_tpl != a_tpl;
    let script = b_setup != a_setup || b_script != a_script;

    match (style, template, script) {
        (true, false, false) => StyleOnly,
        (false, true, false) => TemplateOnly,
        // Includes `(false, false, false)`: nothing parsed differently. It is
        // reported as `Script` because a no-op must never suppress the rebuild
        // that a genuinely unparseable neighbour would have forced.
        _ => Script,
    }
}

/// Non-blocking collapse of a burst of filesystem events into one rebuild.
///
/// Holds only a deadline. It never sleeps: the dev loop asks [`ChangeDebouncer::wait`]
/// how long it may block on its command channel and wakes early for a keystroke
/// or a finished build. That is what replaced the old 150 ms `thread::sleep` in
/// the change path, which stalled all command handling.
///
/// `now` is a parameter rather than read from the clock so the whole state
/// machine is testable without sleeping.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ChangeDebouncer {
    /// When the current burst goes quiet, or `None` when no burst is pending.
    deadline: Option<Instant>,
}

impl ChangeDebouncer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one raw event, (re)starting the quiet window.
    pub fn record(&mut self, now: Instant, window: Duration) {
        self.deadline = Some(now + window);
    }

    /// Whether a burst is still being collected.
    pub fn is_pending(&self) -> bool {
        self.deadline.is_some()
    }

    /// Fire the pending rebuild if the burst has gone quiet, consuming it.
    ///
    /// Returns `true` exactly once per burst, which is what makes N raw events
    /// cost one `cargo build`.
    pub fn take_if_due(&mut self, now: Instant) -> bool {
        match self.deadline {
            Some(deadline) if now >= deadline => {
                self.deadline = None;
                true
            }
            _ => false,
        }
    }

    /// How long the loop may block before re-checking this debouncer.
    ///
    /// `None` when there is nothing pending, in which case the caller falls back
    /// to [`LOOP_TIMEOUT`]. Never returns a zero wait, so a caller cannot spin.
    pub fn wait(&self, now: Instant) -> Option<Duration> {
        let deadline = self.deadline?;
        Some(
            deadline
                .saturating_duration_since(now)
                .max(Duration::from_millis(1)),
        )
    }
}

/// What the dev loop does about a filesystem-watcher failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchReaction {
    /// Print the failure and **keep going**. Never fatal: a blind watcher is
    /// still a running dev server the user can restart a build from by hand,
    /// whereas stopping loses the app they had running.
    Report { message: String, fatal: bool },
}

/// Turn a watcher error into what the dev loop does about it.
///
/// Separate from the loop so the contract is assertable: a watcher failure is
/// reported, names its remedy, and is *not* fatal.
///
/// This exists because `notify` introduces a failure the old poll did not have.
/// On Linux it uses inotify, a finite kernel resource; exhausting
/// `fs.inotify.max_user_watches` makes watching a path fail with "No space left
/// on device". The old `read_dir` poll had no such limit. That makes a silently
/// dead watcher — one that claims to hot-reload and never does — a new way for
/// the dev server to lie to the user, so the error is surfaced instead.
pub fn react_to_watch_error(message: &str) -> WatchReaction {
    WatchReaction::Report {
        message: format!(
            "{message}\n\
             \x20    The kernel's inotify watch limit is exhausted, so files may no longer be\n\
             \x20    detected. Raise it (Linux):\n\
             \x20      sudo sysctl -w fs.inotify.max_user_watches=524288\n\
             \x20      sudo sysctl -w fs.inotify.max_user_instances=1024\n\
             \x20    or persist it in /etc/sysctl.d/. The dev server keeps running;\n\
             \x20    press 'r' to rebuild by hand, and check that target/ is outside the watched tree."
        ),
        fatal: false,
    }
}

/// A live filesystem watcher, owned for its whole lifetime.
///
/// Owns the `notify` watcher, which is what holds the inotify instance — dropping
/// it releases those watches, so a `DirWatcher` that goes out of scope stops
/// watching.
///
/// Reports changes as [`DevCmd::FileChanged`] on the dev server's own command
/// channel, so a filesystem event and a keystroke wake the same `recv_timeout`.
/// This is why the watcher needs no polling and no second thread of its own:
/// `notify` runs its event loop internally and calls the handler below.
pub struct DirWatcher {
    _watcher: notify::RecommendedWatcher,
}

impl DirWatcher {
    /// Start watching `root` recursively, reporting to `tx`.
    ///
    /// Returns a `DirWatcher` even when it could not watch `root`: the failure
    /// is delivered as [`DevCmd::WatchError`] on the channel instead, because a
    /// dev server that refuses to start over a watcher problem is a worse
    /// failure than one that says so and keeps running.
    ///
    /// `tx` is cloned into the notify handler and into the event-filter closure,
    /// both of which are `Send`, which is what lets the whole watcher stay
    /// single-threaded from the dev loop's point of view.
    pub fn start(root: &Path, tx: mpsc::Sender<DevCmd>) -> Self {
        // Last-seen content per path, so a change can be classified against
        // what was there before. Shared with the handler; the handler runs on
        // notify's thread.
        let previous: Arc<Mutex<HashMap<PathBuf, String>>> = Arc::new(Mutex::new(HashMap::new()));
        // Seed the cache with what is already on disk, so the *first* save is
        // classified against real prior content rather than as "unknown".
        seed_cache(&previous, root);

        let prev_for_handler = Arc::clone(&previous);
        let tx_for_handler = tx.clone();
        // Events arrive with absolute paths. They are reported (and cached)
        // relative to the watch root, which is what the dev server printed before
        // this watcher existed — and it is load-bearing, not cosmetic: the cache
        // is seeded with relative keys, so an absolute key would miss on every
        // lookup and every change would be classified as the conservative
        // `Script` kind.
        let root_for_handler = root.to_path_buf();

        let mut watcher =
            match notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                let event = match res {
                    Ok(e) => e,
                    // A runtime inotify error (queue overflow, watch removal).
                    // Surfaced, not swallowed: see `react_to_watch_error`.
                    Err(e) => {
                        let _ = tx_for_handler.send(DevCmd::WatchError {
                            message: e.to_string(),
                        });
                        return;
                    }
                };
                // Access events are the watcher reading the tree, not the user
                // editing it. Acting on them would make the dev server rebuild
                // in response to its own `cargo build`.
                if matches!(event.kind, EventKind::Access(_)) {
                    return;
                }
                for path in &event.paths {
                    let Ok(rel) = path.strip_prefix(&root_for_handler) else {
                        continue;
                    };
                    // Checking *every* component is what excludes a nested
                    // `sub/project/target/`, and it is also what keeps a
                    // directory-creation event from leaking: `notify` reports
                    // each newly created parent as an event in its own right, and
                    // creating `target/` reports `target` — which this filter
                    // already rejects.
                    //
                    // An earlier draft also skipped `path.is_dir()`. Mutation
                    // testing showed that guard was not load-bearing (removing
                    // it left every test green — see
                    // `3.1-mutation-results.log`, M2), so it was deleted rather
                    // than shipped as unverified code. It carried a TOCTOU race
                    // of its own: a directory removed again before the check
                    // reads as a file.
                    if is_ignored(rel) {
                        continue;
                    }
                    let kind = classify_observed(&prev_for_handler, rel);
                    if tx_for_handler
                        .send(DevCmd::FileChanged {
                            path: rel.to_path_buf(),
                            kind,
                        })
                        .is_err()
                    {
                        // The dev loop is gone; nothing left to tell.
                        return;
                    }
                }
            }) {
                Ok(w) => w,
                Err(e) => {
                    let _ = tx.send(DevCmd::WatchError {
                        message: format!("Could not start a filesystem watcher: {e}"),
                    });
                    // `InotifyWatcher` is the concrete type on Linux and the
                    // recommended one everywhere; if construction itself failed there
                    // is nothing to hand back, so the dev loop runs without one and
                    // has already been told why.
                    return Self {
                        _watcher: unavailable_watcher(),
                    };
                }
            };

        if let Err(e) = watcher.watch(root, RecursiveMode::Recursive) {
            let _ = tx.send(DevCmd::WatchError {
                message: format!("Could not watch {}: {e}", root.display()),
            });
        }

        Self { _watcher: watcher }
    }
}

/// A no-op watcher, for the case where `notify` could not be constructed at all.
///
/// Exists so [`DirWatcher::start`] always returns something: the failure has
/// already been reported to the user, and a dev server that exits immediately
/// after saying "could not start a filesystem watcher" is less useful than one
/// that stays up and can still be driven by hand with `r`.
fn unavailable_watcher() -> notify::RecommendedWatcher {
    // `recommended_watcher` has failed once already in this process, so this
    // cannot realistically fail; if it somehow does, there is no third option
    // that keeps the caller simpler.
    notify::recommended_watcher(|_| {}).expect("notify watcher construction")
}

/// Read the current on-disk content of every non-ignored file under `root`.
///
/// One walk, at watcher startup only. It exists so the first save after `velox
/// dev` starts is classified against real prior content: without a baseline the
/// first change of any kind would have to be treated as unknown.
fn seed_cache(cache: &Arc<Mutex<HashMap<PathBuf, String>>>, root: &Path) {
    fn walk(cache: &Arc<Mutex<HashMap<PathBuf, String>>>, dir: &Path, root: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(rel) = path.strip_prefix(root) else {
                continue;
            };
            if is_ignored(rel) {
                continue;
            }
            if path.is_dir() {
                walk(cache, &path, root);
            } else if let Ok(content) = std::fs::read_to_string(&path) {
                cache.lock().unwrap().insert(rel.to_path_buf(), content);
            }
        }
    }
    walk(cache, root, root);
}

/// Classify one observed change by diffing the file's new content against its
/// last-seen content, and update the cache either way.
///
/// A file we have never seen (created since startup, or binary/unreadable) is
/// [`ChangeKind::Script`]: there is no prior content to diff against, and the
/// conservative kind is always the safe answer.
fn classify_observed(previous: &Arc<Mutex<HashMap<PathBuf, String>>>, path: &Path) -> ChangeKind {
    let Ok(after) = std::fs::read_to_string(path) else {
        return ChangeKind::Script;
    };
    let mut cache = previous.lock().unwrap();
    match cache.remove(path) {
        Some(before) => {
            let kind = classify_change(&before, &after);
            cache.insert(path.to_path_buf(), after);
            kind
        }
        // New file, or a deletion. Either way there is nothing to diff, so the
        // dev server is told to do the safe thing.
        None => {
            cache.insert(path.to_path_buf(), after);
            ChangeKind::Script
        }
    }
}

// ---- tiny ANSI helpers (no extra deps) ----
fn ansi(code: &str, s: &str) -> String {
    format!("\x1b[{}m{}\x1b[0m", code, s)
}
fn dim(s: &str) -> String {
    ansi("2", s)
}
fn bold(s: &str) -> String {
    ansi("1", s)
}
fn cyan(s: &str) -> String {
    ansi("36", s)
}
fn green(s: &str) -> String {
    ansi("32", s)
}
fn red(s: &str) -> String {
    ansi("31", s)
}
fn yellow(s: &str) -> String {
    ansi("33", s)
}
fn clear_screen() {
    print!("\x1b[2J\x1b[1;1H");
    let _ = std::io::stdout().flush();
}

/// Read the binary name from a project's Cargo.toml so we can target the right
/// binary with `cargo run --bin <name>` (robust even inside a multi-binary
/// workspace where a bare `cargo run` would be ambiguous).
///
/// Prefers the explicit `[[bin]] name` over `[package] name` because the
/// binary name (what cargo actually builds) may differ from the package name
/// (e.g. `velox-example-counter` package with `counter` bin).
fn project_bin_name(project_dir: &Path) -> Option<String> {
    let manifest = project_dir.join("Cargo.toml");
    let content = std::fs::read_to_string(&manifest).ok()?;

    // First pass: look for an explicit [[bin]] name.
    let mut in_bin = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[[bin]]" {
            in_bin = true;
            continue;
        }
        if in_bin && trimmed.starts_with('[') {
            in_bin = false;
        }
        if in_bin
            && trimmed.starts_with("name")
            && let Some(eq) = trimmed.find('=')
        {
            let value = trimmed[eq + 1..].trim().trim_matches('"').to_string();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }

    // Fallback: use [package] name.
    let mut in_package = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[package]" {
            in_package = true;
            continue;
        }
        if in_package && trimmed.starts_with('[') && trimmed != "[package]" {
            break;
        }
        if in_package
            && trimmed.starts_with("name")
            && let Some(eq) = trimmed.find('=')
        {
            let value = trimmed[eq + 1..].trim().trim_matches('"').to_string();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

/// What the dev loop should do about an observed change.
///
/// Split out so the "a style edit must not trigger `cargo build`" rule is a
/// single, named, testable decision rather than an `if` buried in the loop where
/// the next edit to the loop can quietly undo it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeReaction {
    /// A real `cargo build` plus an app restart.
    Rebuild,
    /// A live stylesheet swap: no `cargo build`, no app restart. This is the
    /// single biggest HMR win available, since CSS edits are the most frequent
    /// edit in any UI codebase.
    SwapStylesheet { path: PathBuf },
}

impl ChangeReaction {
    /// Whether this reaction needs a `cargo build` **right now**.
    ///
    /// **This is the task 3.1 / task 3.2 seam, and it is deliberately still
    /// `true` for every kind.**
    ///
    /// The swap that [`ChangeReaction::SwapStylesheet`] asks for needs
    /// `HmrMessage::StyleUpdate`, which lives in `velox-renderer` — outside this
    /// crate, and the whole of task 3.2. Flipping this to `false` for
    /// `StyleOnly` *before* that message exists would not be the HMR win, it
    /// would be a silent regression: CSS edits would stop rebuilding and there
    /// would be nothing to replace them, so editing a `<style>` block would do
    /// nothing at all and look like a broken dev server.
    ///
    /// So the win lands as one line, here, in the same commit that adds the
    /// message. The classification it depends on is complete and tested now, so
    /// that commit is a one-liner rather than a re-derivation.
    pub fn needs_rebuild(self) -> bool {
        let _ = self;
        true
    }
}

/// Decide what a classified change means for the dev loop.
///
/// Pure and one-to-one with [`ChangeKind`], mirroring [`react_to`] for build
/// outcomes: the loop matches on this rather than on the kind, which is what
/// makes "a style edit is not a rebuild" a testable claim instead of a comment.
pub fn react_to_change(path: &Path, kind: ChangeKind) -> ChangeReaction {
    match kind {
        ChangeKind::StyleOnly => ChangeReaction::SwapStylesheet {
            path: path.to_path_buf(),
        },
        // `TemplateOnly` and `Script` both need a real build, so they are not
        // distinguished here. That is honest rather than lazy: task 3.3 is what
        // gives `TemplateOnly` a live rerender, and until it lands both kinds
        // cost exactly the same rebuild, so pretending otherwise here would
        // claim a capability that does not exist.
        ChangeKind::TemplateOnly | ChangeKind::Script => ChangeReaction::Rebuild,
    }
}

/// The dev loop's command channel.
///
/// `pub` so the off-the-loop build plumbing can be tested from
/// `tests/dev_reload_tests.rs`: proving the loop stays responsive while a compile
/// is in flight means driving this channel, not spawning a real `cargo`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DevCmd {
    Reload,
    Clear,
    Quit,
    /// A `cargo build` attempt finished. The build runs on its own thread and
    /// reports back over this same channel, so the dev loop wakes on it
    /// immediately instead of waiting out `LOOP_TIMEOUT` to notice.
    Built(BuildOutcome),
    /// A watched file changed, classified by the SFC block it landed in.
    ///
    /// Arrives from [`DirWatcher`] over this same channel as the keystrokes, so
    /// a save and a keypress wake one `recv_timeout` rather than a save having
    /// to wait out a poll tick. `path` is relative to the watch root.
    ///
    /// `notify` reports create + modify + close_write for one save, so this
    /// arrives several times per save; [`ChangeDebouncer`] collapses the burst
    /// without blocking.
    FileChanged {
        path: PathBuf,
        kind: ChangeKind,
    },
    /// The filesystem watcher failed — most importantly inotify watch
    /// exhaustion. Reported, never fatal: see [`react_to_watch_error`].
    WatchError {
        message: String,
    },
}

/// Tracks the dev loop's only source of user input.
///
/// `rx.recv_timeout` **cannot** use `RecvTimeoutError::Disconnected` as its
/// "the user is gone" signal. The loop holds a sender of its own — it needs one to
/// hand to a [`BuildWorker`], and build results come back on this same channel —
/// and a single live sender keeps an `mpsc` channel out of the disconnected state
/// for as long as the loop exists. So stdin EOF is invisible on the channel: the
/// reader thread returns, nothing is ever delivered again, and the loop just
/// spins on `Timeout` forever with a live child under it.
///
/// Liveness is therefore tracked on the reader's `JoinHandle` directly. This is
/// the whole reason stdin EOF exits the dev server at all; see
/// `a_loop_that_holds_its_own_sender_can_never_see_disconnected` for the
/// channel-level fact this is working around.
pub struct InputSource {
    handle: JoinHandle<()>,
}

impl InputSource {
    pub fn from_handle(handle: JoinHandle<()>) -> Self {
        Self { handle }
    }

    /// True once the reader has stopped: stdin reached EOF, the read errored, or
    /// the user typed `q`.
    ///
    /// Polled on the loop's timeout tick, so the dev server notices within
    /// `LOOP_TIMEOUT` of input going away.
    pub fn is_exhausted(&self) -> bool {
        self.handle.is_finished()
    }
}

/// What the dev loop should do about a build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildAction {
    /// Nothing to do.
    Idle,
    /// Start a `cargo build` now.
    Start,
    /// A build is already running, so this change was *recorded* rather than
    /// acted on. It becomes a single follow-up build when the current one ends.
    AlreadyInFlight,
}

/// Coalesces rapid saves into a single rebuild.
///
/// Every detected change funnels through [`BuildGate::on_change`]. While a build
/// is running a change only sets a flag, so N saves inside one compile produce
/// one follow-up build rather than N competing `cargo build` processes (which
/// would serialise on the target-dir lock anyway).
///
/// This replaces the old "stamp the clock, sleep 150 ms, never re-scan" window,
/// which stamped `last_check` *before* sleeping and therefore guaranteed a second
/// full build for any save landing inside the window. Coalescing is a state
/// machine here, not a sleep, so it covers the whole duration of a build instead
/// of a fixed 150 ms.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BuildGate {
    in_flight: bool,
    pending: bool,
}

impl BuildGate {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a detected file change or a manual reload.
    ///
    /// Returns [`BuildAction::Start`] only when no build is running, so the
    /// caller never has to ask "is one already in flight?" separately.
    pub fn on_change(&mut self) -> BuildAction {
        if self.in_flight {
            self.pending = true;
            BuildAction::AlreadyInFlight
        } else {
            self.in_flight = true;
            BuildAction::Start
        }
    }

    /// Record that the in-flight build finished.
    ///
    /// Called for a failure exactly as for a success: a compile error must never
    /// wedge the gate, or the next save after fixing the error would be
    /// swallowed and the app would never come back.
    pub fn on_build_finished(&mut self) {
        debug_assert!(
            self.in_flight,
            "on_build_finished called with no build in flight"
        );
        self.in_flight = false;
    }

    /// A change recorded while a build was running, consumed exactly once.
    ///
    /// The caller keeps the decision to build in one place: it asks for this,
    /// folds the answer into "a change is wanted", and lets the ordinary
    /// `on_change` path decide.
    pub fn take_pending(&mut self) -> bool {
        std::mem::take(&mut self.pending)
    }

    pub fn is_build_in_flight(&self) -> bool {
        self.in_flight
    }

    pub fn has_pending_change(&self) -> bool {
        self.pending
    }
}

/// The result of one `cargo build` attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildOutcome {
    /// Compiled; the freshly built binary can be run.
    Compiled { elapsed: Duration },
    /// Compilation failed. This is **not** a dev-server failure: the watcher
    /// stays alive, the error is printed, and the next save retries.
    CompileFailed { stderr: String },
    /// `cargo` could not be launched at all.
    InvokeFailed { message: String },
}

/// Classify a finished `cargo build`.
///
/// Split out from the process plumbing so the contract that matters — a non-zero
/// exit is a *compile error*, not a dev-server failure — is assertable without
/// invoking a compiler.
pub fn classify_build(success: bool, stderr: String, elapsed: Duration) -> BuildOutcome {
    if success {
        BuildOutcome::Compiled { elapsed }
    } else {
        BuildOutcome::CompileFailed { stderr }
    }
}

/// What the dev loop should do with a finished build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildReaction {
    /// Start the freshly built app.
    RunApp { elapsed: Duration },
    /// Print the compile diagnostics panel. The loop does *not* stop: the watcher
    /// stays alive and the next save retries.
    PrintBuildError { stderr: String },
    /// Report that cargo itself could not be launched. This is a dev-server
    /// problem, not the user's, and it must not be rendered as a compile error —
    /// there are no diagnostics to show and retrying will not help until cargo
    /// is available again.
    ReportLaunchFailure { message: String },
}

/// Decide what a finished build means for the dev loop.
///
/// Pure, and deliberately one-to-one with [`BuildOutcome`]: the loop matches on
/// this rather than on the outcome, which is what makes "a compile error is not
/// fatal, and is not the same thing as cargo failing to launch" a testable claim
/// instead of a comment.
pub fn react_to(outcome: &BuildOutcome) -> BuildReaction {
    match outcome {
        BuildOutcome::Compiled { elapsed } => BuildReaction::RunApp { elapsed: *elapsed },
        BuildOutcome::CompileFailed { stderr } => BuildReaction::PrintBuildError {
            stderr: stderr.clone(),
        },
        BuildOutcome::InvokeFailed { message } => BuildReaction::ReportLaunchFailure {
            message: message.clone(),
        },
    }
}

/// RAII owner of the dev server's app process.
///
/// `std::process::Child` does **not** kill on drop, so a bare `Option<Child>`
/// leaks the app on every path the dev server does not remember to clean up —
/// including a panic in the command loop, which no amount of `kill()` at each
/// `break` can cover. Dropping this kills the process *and* reaps it, so there
/// is a single cleanup path rather than several that can disagree.
pub struct AppChild(Option<Child>);

impl AppChild {
    pub fn new(child: Child) -> Self {
        Self(Some(child))
    }

    /// True once the process has exited. Reaps it as a side effect, so a later
    /// [`AppChild::shutdown`] is a cheap no-op rather than a double reap.
    pub fn has_exited(&mut self) -> std::io::Result<bool> {
        match self.0.as_mut() {
            Some(c) => Ok(c.try_wait()?.is_some()),
            None => Ok(false),
        }
    }

    /// Kill the process and reap it. Returns the exit status when *this* call
    /// performed the reap, and `None` when there was nothing left to reap.
    ///
    /// Idempotent in the strong sense: the handle is taken, so a second call has
    /// nothing to kill and nothing to wait for. Killing an already-reaped handle
    /// is not harmless — it returns an error on every platform, and relying on
    /// that plus `wait`'s cached status would make "did *this* call do the
    /// reaping?" unanswerable.
    pub fn shutdown(&mut self) -> Option<ExitStatus> {
        let mut c = self.0.take()?;
        let _ = c.kill();
        c.wait().ok()
    }
}

impl Drop for AppChild {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Runs one `cargo build` on its own thread so the dev loop keeps servicing
/// keystrokes, app exits, and further saves while the compiler works.
///
/// The `cargo` process handle is published into `cargo_slot` *before* its
/// output is drained. If the dev server exits mid-compile, [`Drop`] takes the
/// handle, kills `cargo` and reaps it. Leaving the handle on the worker thread
/// would make that impossible: the only owner would be blocked reading a pipe
/// from a process nothing could signal.
pub struct BuildWorker {
    cargo_slot: Arc<Mutex<Option<Child>>>,
    handle: Option<JoinHandle<()>>,
}

impl BuildWorker {
    /// Start a real `cargo build` that reports its result to `tx` — the dev loop's
    /// own command channel, not a private one, so a finished compile wakes the
    /// loop through the same `recv_timeout` that delivers keystrokes.
    fn start(
        project_dir: PathBuf,
        release: bool,
        bin: Option<String>,
        tx: mpsc::Sender<DevCmd>,
    ) -> Self {
        Self::start_with(tx, move |tx, cargo_slot| {
            run_build(project_dir, release, bin, tx, cargo_slot);
        })
    }

    /// Start a worker running an arbitrary build body.
    ///
    /// The body is injected rather than hardcoded so the property that actually
    /// fixed the stall — *the loop does not wait for a compile* — is testable
    /// without invoking a compiler. The body is handed the slot so it can publish
    /// its `Child` under the same shutdown contract [`run_build`] obeys; a
    /// `start_with` body that never publishes one simply has nothing to kill.
    pub fn start_with<F>(tx: mpsc::Sender<DevCmd>, body: F) -> Self
    where
        F: FnOnce(&mpsc::Sender<DevCmd>, &Mutex<Option<Child>>) + Send + 'static,
    {
        let cargo_slot: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
        let slot = Arc::clone(&cargo_slot);
        let handle = thread::spawn(move || {
            body(&tx, &slot);
        });
        Self {
            cargo_slot,
            handle: Some(handle),
        }
    }
}

impl Drop for BuildWorker {
    fn drop(&mut self) {
        if let Some(mut c) = self.cargo_slot.lock().unwrap().take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// One `cargo build`, on the worker thread. Reports the result as a
/// [`DevCmd::Built`] on the dev server's own channel.
fn run_build(
    project_dir: PathBuf,
    release: bool,
    bin: Option<String>,
    tx: &mpsc::Sender<DevCmd>,
    cargo_slot: &Mutex<Option<Child>>,
) {
    let start = SystemTime::now();

    let mut build = Command::new("cargo");
    build.arg("build");
    if release {
        build.arg("--release");
    }
    if let Some(ref name) = bin {
        build.arg("--bin").arg(name);
    }
    build
        .current_dir(&project_dir)
        .stdin(Stdio::null())
        // `cargo build` reports everything on stderr. The previous code piped
        // stdout and then never read it, so it is sent to null rather than into
        // a pipe nobody drains.
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let mut child = match build.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(DevCmd::Built(BuildOutcome::InvokeFailed {
                message: e.to_string(),
            }));
            return;
        }
    };

    let pipe = child.stderr.take();
    // Publish the handle first: from here on `BuildWorker::drop` can kill and
    // reap `cargo` if the dev server shuts down mid-compile.
    *cargo_slot.lock().unwrap() = Some(child);

    let mut stderr = Vec::new();
    if let Some(mut err) = pipe {
        let _ = err.read_to_end(&mut stderr);
    }

    // Reap, unless `BuildWorker::drop` already took the handle and killed it
    // because the dev server is shutting down. In that case there is no result
    // to report and nothing left to run.
    let still_ours = cargo_slot.lock().unwrap().take();
    let Some(mut c) = still_ours else {
        return;
    };
    let Ok(status) = c.wait() else {
        return;
    };

    let outcome = classify_build(
        status.success(),
        String::from_utf8_lossy(&stderr).into_owned(),
        start.elapsed().unwrap_or_default(),
    );
    let _ = tx.send(DevCmd::Built(outcome));
}

/// Whether hot reload is actually available in this `velox dev` run.
///
/// This exists because the honest answer is not knowable from the port number.
/// The listener used to bind on its own thread, so a failure arrived as an
/// `eprintln!` on that thread and stopped there: nothing carried it back, and the
/// banner went on claiming `HMR enabled`. Binding happens synchronously in
/// [`HmrListener::start`] now, so the outcome is an ordinary return value and this
/// carries it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum HmrStatus {
    Available,
    /// The listener could not bind. `reason` is the OS error, kept verbatim so the
    /// banner names the real cause (`port 31313 in use`) instead of shrugging.
    Unavailable {
        reason: String,
    },
}

impl HmrStatus {
    fn is_available(&self) -> bool {
        matches!(self, HmrStatus::Available)
    }

    /// The `➤ HMR:` banner line. Says `unavailable` with the reason rather than
    /// advertising auto-reload that cannot happen.
    fn banner_line(&self, port: u16) -> String {
        match self {
            HmrStatus::Available => {
                format!("port {port} (auto-reload on save)")
            }
            HmrStatus::Unavailable { reason } => {
                format!("port {port} (unavailable: {reason})")
            }
        }
    }

    /// The line printed once the app process exists.
    ///
    /// This is the string that used to lie. It is a function of the bind result
    /// and nothing else, so it cannot say `enabled` unless the listener is
    /// actually listening.
    fn app_started_line(&self) -> String {
        match self {
            HmrStatus::Available => "App started (HMR enabled)".to_string(),
            HmrStatus::Unavailable { reason } => {
                format!("App started (HMR unavailable: {reason})")
            }
        }
    }
}

/// The one-line reason a bind failed, phrased for someone reading a terminal.
///
/// `AddrInUse` is by far the common case and it has a cause the user can act on,
/// so it gets words; anything else passes the OS message through rather than
/// inventing an explanation for it. Only ever reached when
/// [`HmrStatus::Unavailable`] is built, so there is no path on which it produces a
/// reason for a bind that actually succeeded.
fn port_reason(e: &std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::AddrInUse {
        "in use by another process".to_string()
    } else {
        e.to_string()
    }
}

/// Owns the HMR listener thread.
///
/// The listener's `accept()` loop is non-blocking and polls, so it can be given a
/// real shutdown path: `drop` sets the flag and joins. It is joined rather than
/// detached because it owns a bound TCP socket, and a detached listener would
/// keep the port until the process exited.
///
/// The socket is bound **here**, on the caller's thread, and only the already-bound
/// listener is handed to the worker. That is what makes the failure observable: a
/// bind that happened on the worker thread could only ever be reported by printing
/// from it, which is how the banner came to lie. It is also why there is no
/// `is_bound()` probe anywhere — a probe would race the thread, whereas a
/// `Result` is a fact about a call that has already returned.
pub struct HmrListener {
    slot: HmrSlot,
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl HmrListener {
    /// Bind `port` and start accepting on it.
    ///
    /// Returns the `io::Error` from the bind, so the caller can report that HMR is
    /// unavailable and carry on — continuing without hot reload is correct,
    /// claiming to have it is not.
    fn start(port: u16) -> std::io::Result<Self> {
        let listener = std::net::TcpListener::bind(("127.0.0.1", port))?;
        listener.set_nonblocking(true).ok();

        let slot: HmrSlot = Arc::new(Mutex::new(None));
        let shutdown = Arc::new(AtomicBool::new(false));

        let slot_clone = Arc::clone(&slot);
        let shutdown_clone = Arc::clone(&shutdown);
        let handle = thread::spawn(move || {
            // The flag is checked at the top of every iteration, and the only
            // blocking thing in the body is a bounded sleep, so `drop` never
            // waits long to join this thread.
            while !shutdown_clone.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, addr)) => {
                        eprintln!("[velox] HMR client connected: {}", addr);
                        // Store the connected stream so the dev server can send to it.
                        *slot_clone.lock().unwrap() = Some(stream);
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(50));
                    }
                    Err(e) => {
                        eprintln!("[velox] HMR accept error: {}", e);
                        thread::sleep(Duration::from_millis(100));
                    }
                }
            }
        });

        Ok(Self {
            slot,
            shutdown,
            handle: Some(handle),
        })
    }

    pub fn slot(&self) -> &HmrSlot {
        &self.slot
    }
}

impl Drop for HmrListener {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Start the Velox dev server in `project_dir` (builds with `cargo run`,
/// watching `<project_dir>/src` for changes).
///
/// Uses HMR: the dev server starts a TCP listener, spawns the app with
/// `VELOX_HMR=1` and `VELOX_HMR_PORT=<port>`, and on file changes sends
/// a `FullReload` message to the app. The app exits, and the dev server
/// restarts it.
pub fn dev_current(project_dir: &Path, release: bool) -> Result<()> {
    let watch_dir = project_dir.join("src");
    let watch_dir = if watch_dir.exists() {
        watch_dir
    } else {
        project_dir.to_path_buf()
    };

    // Bind the HMR port BEFORE the banner is drawn, so every line the user reads
    // is a statement about the run they are in. The bind is a local socket and
    // takes microseconds; the build that follows is what takes seconds.
    //
    // FOLLOW-UP (deliberately not done here): the port is hardcoded and not
    // configurable. There is no `--hmr-port`, and no override path reaches the app
    // either — `dev.rs` always exports `VELOX_HMR_PORT` from the same constant, so
    // a stray listener on 31313 wedges hot reload for this project with no way out
    // but finding and killing an unknown process. The fix is one flag threaded to
    // both `HmrListener::start` and `VELOX_HMR_PORT`; adding a CLI surface is a
    // separate decision from making the log honest.
    let hmr_port = velox_renderer::DEFAULT_HMR_PORT;
    let (hmr, hmr_status) = match HmrListener::start(hmr_port) {
        Ok(listener) => {
            eprintln!("[velox] HMR dev server listening on 127.0.0.1:{}", hmr_port);
            (Some(listener), HmrStatus::Available)
        }
        Err(e) => (
            None,
            HmrStatus::Unavailable {
                reason: format!("port {hmr_port} {}", port_reason(&e)),
            },
        ),
    };

    print_banner(project_dir, release, &watch_dir, &hmr_status);

    let (tx, rx) = mpsc::channel::<DevCmd>();
    // `tx` stays alive in this scope for the whole function — the loop hands a
    // clone to every `BuildWorker`, and build results come back on this same
    // channel. That is what makes `RecvTimeoutError::Disconnected` unreachable
    // here, so stdin EOF is detected on the reader's own handle instead.
    let stdin = InputSource::from_handle(spawn_stdin_reader(tx.clone()));

    let project = project_dir.to_path_buf();
    let bin = project_bin_name(project_dir);

    // The first build runs on the worker too, so `q` works during a cold
    // compile instead of the loop being unreachable until it finishes.
    println!("{}", dim("⏳ Compiling..."));
    let mut builder = Some(BuildWorker::start(
        project.clone(),
        release,
        bin.clone(),
        tx.clone(),
    ));
    // `None` means "no app is running", which is a normal state: after a
    // compile error, after the user quits the app, and before the first
    // successful build.
    let mut child: Option<AppChild> = None;
    let mut gate = BuildGate::new();
    // The debouncer is the *only* thing that used to be `last_check` plus a
    // 150 ms sleep. It holds a deadline, not a clock: a burst of filesystem
    // events extends it, and the loop wakes when it expires.
    let mut debounce = ChangeDebouncer::new();
    // The change observed in the current burst, folded with
    // [`ChangeKind::merge`] so N edits in one window cost one build and the
    // result is never cheaper than any single edit in it.
    let mut burst: Option<(PathBuf, ChangeKind)> = None;
    let mut crashed = false;

    // Watching starts here and the watcher lives to the end of the loop, because
    // dropping it releases the inotify watches. It is created after the channel
    // so a failure can be reported on the loop's own channel.
    let _watcher = DirWatcher::start(&watch_dir, tx.clone());

    loop {
        // True when something this iteration wants a rebuild for.
        let mut change_requested = false;

        // Block until the next event. A keystroke, a finished build, or a
        // filesystem change all wake the loop straight away.
        //
        // The wait is the *shorter* of the debounce deadline and
        // `LOOP_TIMEOUT`: while a burst is being collected the loop must wake
        // exactly when it goes quiet, and afterwards it only needs to wake
        // periodically to notice that stdin died. Neither branch sleeps.
        let wait = debounce.wait(Instant::now()).unwrap_or(LOOP_TIMEOUT);
        match rx.recv_timeout(wait) {
            Ok(DevCmd::Quit) => {
                // Drop both guards here rather than relying on end-of-scope
                // drops, so the app is gone before we say it is. Dropping is the
                // only kill path: there is no second `kill()` that can disagree.
                drop(child.take());
                drop(builder.take());
                println!("\n{} Dev server stopped.", bold("👋"));
                break;
            }
            Ok(DevCmd::Clear) => {
                clear_screen();
                print_banner(project_dir, release, &watch_dir, &hmr_status);
                if crashed {
                    println!(
                        "{} {}\n",
                        red("✗ App crashed."),
                        dim("Fix the error and save to rebuild.")
                    );
                }
            }
            Ok(DevCmd::Reload) => {
                println!("{}", yellow("↻ Manual reload requested"));
                // A manual reload is a change, but not a *file* change. There is
                // no scan clock to advance any more — the watcher only reports
                // real events, so a manual reload cannot come back as one.
                change_requested = true;
            }
            Ok(DevCmd::FileChanged { path, kind }) => {
                // One raw event. `notify` emits several per save, so this only
                // restarts the quiet window; the build happens once, below,
                // when the window expires.
                debounce.record(Instant::now(), DEBOUNCE_WINDOW);
                burst = Some(match burst {
                    Some((prev_path, prev_kind)) => (prev_path, prev_kind.merge(kind)),
                    None => (path, kind),
                });
            }
            Ok(DevCmd::WatchError { message }) => {
                // Reported, not fatal. The loop keeps running and keeps reading
                // the same channel, so a change that still gets through is
                // still acted on. See `react_to_watch_error` for why stopping
                // would be the worse failure.
                match react_to_watch_error(&message) {
                    // `fatal` is carried on the variant precisely so a future
                    // fatal outcome is representable; today it is always false,
                    // which is the contract the test pins.
                    WatchReaction::Report { message, fatal } => {
                        let label = if fatal {
                            "stopped"
                        } else {
                            "watching continues"
                        };
                        println!(
                            "{} Filesystem watcher error ({label}):\n{message}",
                            red("✗")
                        );
                    }
                }
            }
            Ok(DevCmd::Built(outcome)) => {
                // The worker is finished either way; freeing it here lets the
                // next accepted change start a fresh one.
                drop(builder.take());
                gate.on_build_finished();

                // A save that landed while this compile was running supersedes
                // its result: rebuilding is cheaper than restarting the app on a
                // binary we are about to replace, and a failed build's
                // diagnostics will be reprinted by the build that supersedes it.
                if gate.take_pending() {
                    change_requested = true;
                } else {
                    match react_to(&outcome) {
                        BuildReaction::RunApp { elapsed } => {
                            println!("{} Compiled in {:.1}s", green("✓"), elapsed.as_secs_f32());
                            child =
                                spawn_app(&project, release, bin.clone(), hmr_port, &hmr_status)
                                    .map(AppChild::new);
                            crashed = false;
                        }
                        // Not fatal. The watcher stays alive and the next save
                        // retries — this is the syntax-error path.
                        BuildReaction::PrintBuildError { stderr } => print_build_error(&stderr),
                        BuildReaction::ReportLaunchFailure { message } => {
                            println!("{} Failed to invoke cargo: {}", red("✗"), message);
                        }
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                // The stdin reader is the loop's only source of user input, and
                // the channel cannot tell us when it stops: `tx` is alive in this
                // scope, so `Disconnected` never fires (see `InputSource`). Check
                // the reader's own handle, or a dev server whose stdin has closed
                // spins here forever with a live app under it.
                if stdin.is_exhausted() {
                    break;
                }
            }
            // Unreachable while `tx` is in scope, and kept only as a correct
            // backstop: if the loop ever stops holding a sender, every producer
            // really is gone and stopping is right. `child` and `builder` drop
            // below and are reaped on the way out.
            Err(RecvTimeoutError::Disconnected) => break,
        }

        // Drain the burst: act only once, and only after it has gone quiet. This
        // is the non-blocking collapse. There is no `sleep` here — the loop
        // already blocked on the channel above for exactly the remaining
        // window, so a keystroke or a finished build still interrupts it.
        if debounce.take_if_due(Instant::now())
            && let Some((path, kind)) = burst.take()
        {
            println!(
                "{} {} changed ({kind:?}) — rebuilding",
                yellow("↻"),
                path.display()
            );
            // The rebuild decision goes through `react_to_change` so that
            // task 3.2 turns "a style edit is not a rebuild" into a one-liner
            // here rather than a re-derivation. It returns `true` for every
            // kind today, and that is a deliberate stop, not an oversight — see
            // `ChangeReaction::needs_rebuild` for why flipping it before the
            // `StyleUpdate` message exists would be a silent regression.
            change_requested = react_to_change(&path, kind).needs_rebuild();
        }

        if change_requested && gate.on_change() == BuildAction::Start {
            // Tell the app to reload, then stop waiting for it to oblige. The
            // old code gave it a 2 s grace here, which added up to 2 s to every
            // reload and bought nothing: the `kill()` that followed was
            // unconditional, so the grace only ever affected ordering.
            // Ask the app to exit so the rebuild can replace it. Only when HMR is
            // actually listening: with no listener there is nobody to tell, and
            // asking anyway printed `No HMR client connected — skipping reload`
            // on every single save. The kill below is unconditional, so skipping
            // the ask costs nothing but the noise.
            if let Some(hmr) = hmr.as_ref() {
                send_hmr_reload(hmr.slot());
            }
            // Dropping `AppChild` sends the kill and reaps. No wait loop.
            drop(child.take());
            println!("{}", dim("⏳ Compiling..."));
            builder = Some(BuildWorker::start(
                project.clone(),
                release,
                bin.clone(),
                tx.clone(),
            ));
        }

        // If the app exited, report but keep watching (Vite-like resilience).
        if let Some(ref mut c) = child
            && c.has_exited()?
        {
            println!(
                "{}",
                dim("App exited. Press 'r' to restart, or save a file to rebuild.")
            );
            child = None;
            crashed = true;
        }
    }

    Ok(())
}

/// Shared slot that holds the most recently connected HMR client stream.
/// The app connects to our TCP listener; we store the stream here so we
/// can send messages to the app when files change.
type HmrSlot = std::sync::Arc<std::sync::Mutex<Option<std::net::TcpStream>>>;

/// Send an HMR `FullReload` message to the connected app.
fn send_hmr_reload(slot: &HmrSlot) {
    let mut guard = slot.lock().unwrap();
    if let Some(ref mut stream) = *guard {
        let msg = velox_renderer::HmrMessage::FullReload;
        let json = serde_json::to_string(&msg).unwrap_or_default();
        match stream.write_all(json.as_bytes()) {
            Ok(_) => {
                eprintln!("[velox] Sent FullReload to app");
            }
            Err(e) => {
                eprintln!("[velox] HMR send failed (app may have disconnected): {}", e);
                *guard = None;
            }
        }
    } else {
        eprintln!("[velox] No HMR client connected — skipping reload");
    }
}

fn print_banner(project_dir: &Path, release: bool, watch_dir: &Path, hmr: &HmrStatus) {
    let name = project_dir
        .canonicalize()
        .ok()
        .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
        .unwrap_or_else(|| "velox-app".into());
    println!();
    println!(
        "  {} {}",
        bold(&cyan("⚡ Velox dev server")),
        dim(&format!("v{}", env!("CARGO_PKG_VERSION")))
    );
    println!("  {} {}", bold("➤ Project:"), name);
    println!("  {} {}", bold("➤ Watching:"), watch_dir.display());
    println!(
        "  {} {}",
        bold("➤ HMR:"),
        hmr.banner_line(velox_renderer::DEFAULT_HMR_PORT)
    );
    println!(
        "  {} {}",
        bold("➤ Build:"),
        if release { "release" } else { "debug" }
    );
    println!("  {}", dim("  r: reload   c: clear   q: quit"));
    println!();
}

/// Read dev commands from stdin on a background thread.
///
/// The handle is returned rather than discarded, and it is deliberately **never
/// joined**: `stdin.lock().lines()` blocks in a read on a file descriptor that no
/// portable std API can cancel, so joining it would hang the dev server's exit
/// until the user typed a line. What the handle *is* for is liveness — the loop
/// polls [`InputSource::is_exhausted`] to notice that stdin has gone away. See
/// that type for why the channel cannot report it.
///
/// The thread owns no resource that outlives the process and terminates on its
/// own: `tx.send` fails once the loop's receiver is dropped, `q` breaks it after
/// one send, and an EOF or read error breaks it too.
fn spawn_stdin_reader(tx: mpsc::Sender<DevCmd>) -> JoinHandle<()> {
    thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            match line {
                Ok(l) => {
                    let cmd = match l.trim() {
                        "r" | "reload" => Some(DevCmd::Reload),
                        "c" | "clear" => Some(DevCmd::Clear),
                        "q" | "quit" | "exit" => Some(DevCmd::Quit),
                        _ => None,
                    };
                    if let Some(c) = cmd {
                        let is_quit = matches!(c, DevCmd::Quit);
                        if tx.send(c).is_err() {
                            break;
                        }
                        if is_quit {
                            break;
                        }
                    }
                }
                Err(_) => break,
            }
        }
    })
}

/// Spawn the already-compiled app, and tell it whether hot reload is real.
///
/// Split from the build because the two have different blocking profiles. The
/// build is the slow part and runs on a [`BuildWorker`] thread; this is a single
/// `Command::spawn`, which returns as soon as the process exists and so is safe
/// to call straight from the dev loop.
///
/// `hmr` is the outcome of the listener's bind, not the port it was asked for.
/// When it says unavailable the app is started with `VELOX_HMR=0` so it does not
/// connect to whatever stranger is holding the port, and the printed line names
/// the port and the reason rather than claiming auto-reload.
fn spawn_app(
    project_dir: &Path,
    release: bool,
    bin: Option<String>,
    hmr_port: u16,
    hmr: &HmrStatus,
) -> Option<Child> {
    let mut run = Command::new("cargo");
    run.arg("run");
    if release {
        run.arg("--release");
    }
    if let Some(ref name) = bin {
        run.arg("--bin").arg(name);
    }
    // Enable HMR mode in the app — it will connect to our TCP server — but only
    // if that server is really listening. `hmr_config()` reads `VELOX_HMR == "1"`
    // and nothing else, so "0" is how it is turned off.
    if hmr.is_available() {
        run.env("VELOX_HMR", "1")
            .env("VELOX_HMR_PORT", hmr_port.to_string());
    } else {
        run.env("VELOX_HMR", "0");
    }
    run.current_dir(project_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    match run.spawn() {
        Ok(c) => {
            println!("{}", green(&hmr.app_started_line()));
            Some(c)
        }
        Err(e) => {
            log::error!("Start failed: {}", e);
            println!("{} Failed to start: {}", red("✗"), e);
            None
        }
    }
}

fn print_build_error(stderr: &str) {
    let lines: Vec<&str> = stderr.lines().collect();
    let errors: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| {
            l.contains("error[")
                || l.contains("error:")
                || l.contains("cannot find")
                || l.contains("expected")
        })
        .collect();
    println!();
    println!("{}", red("──────────── ✗ Build failed ────────────"));
    if errors.is_empty() {
        // Print last ~15 lines as a fallback.
        for l in lines.iter().rev().take(15).rev() {
            println!("  {}", dim(l));
        }
    } else {
        for l in errors.iter().take(20) {
            println!("  {}", red(l));
        }
    }
    println!("{}", red("─────────────────────────────────────────"));
    println!("{}", dim("   Edit the file and save to rebuild."));
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `target/` is excluded at the watch root and at any depth.
    ///
    /// The behavioural counterpart lives in `tests/watcher_tests.rs`
    /// (`a_change_under_target_is_not_reported`), which proves the same thing
    /// end-to-end through a real watcher. This one pins the pure predicate, so
    /// a regression names the function rather than showing up as a flaky
    /// timeout.
    #[test]
    fn target_is_excluded_at_every_depth() {
        assert!(is_ignored(Path::new("target/debug/junk.txt")));
        assert!(is_ignored(Path::new("sub/project/target/debug/artifact")));
        assert!(is_ignored(Path::new("target")));
        assert!(!is_ignored(Path::new("src/target_like/keep.txt")));
    }

    #[test]
    fn dot_directories_are_excluded_but_source_files_are_not() {
        assert!(is_ignored(Path::new(".git/HEAD")));
        assert!(is_ignored(Path::new(".vscode/settings.json")));
        assert!(!is_ignored(Path::new("App.vx")));
        assert!(!is_ignored(Path::new("components/only.vx")));
    }

    /// Merging a burst must never make the rebuild cheaper than any edit in it.
    ///
    /// This is the property that keeps a style edit that happened to land in the
    /// same debounce window as a script edit from silently skipping the build.
    #[test]
    fn merging_a_burst_never_cheaper_than_its_worst_input() {
        use ChangeKind::*;
        assert_eq!(StyleOnly.merge(StyleOnly), StyleOnly);
        assert_eq!(StyleOnly.merge(TemplateOnly), TemplateOnly);
        assert_eq!(TemplateOnly.merge(StyleOnly), TemplateOnly);
        assert_eq!(StyleOnly.merge(Script), Script);
        assert_eq!(Script.merge(StyleOnly), Script);
        assert_eq!(Script.merge(TemplateOnly), Script);
        assert_eq!(TemplateOnly.merge(Script), Script);
    }

    /// A style edit is routed to a stylesheet swap, not to a rebuild.
    ///
    /// The routing is pinned even though
    /// [`ChangeReaction::needs_rebuild`] is still `true` for every kind. The
    /// point is that the *classification* is complete and correct today, so
    /// task 3.2 only has to flip the one boolean.
    #[test]
    fn a_style_edit_is_routed_to_a_stylesheet_swap() {
        assert_eq!(
            react_to_change(Path::new("App.vx"), ChangeKind::StyleOnly),
            ChangeReaction::SwapStylesheet {
                path: PathBuf::from("App.vx")
            }
        );
        assert_eq!(
            react_to_change(Path::new("App.vx"), ChangeKind::TemplateOnly),
            ChangeReaction::Rebuild
        );
        assert_eq!(
            react_to_change(Path::new("App.vx"), ChangeKind::Script),
            ChangeReaction::Rebuild
        );
    }

    /// The debouncer must not block the loop it runs on.
    ///
    /// This replaces the removed `scanning_does_not_sleep` test, which pinned
    /// that the old 150 ms sleep was gone from the change path. The property is
    /// the same one — the change path must not sleep — but it is now stated
    /// against the mechanism that replaced the sleep: 10 000 record/check cycles
    /// cost microseconds, where anything that slept even once for 1 ms would
    /// take at least 10 ms.
    #[test]
    fn the_debounce_path_never_sleeps() {
        let mut d = ChangeDebouncer::new();
        let start = Instant::now();
        for i in 0..10_000u32 {
            let now = start + Duration::from_micros(i as u64);
            d.record(now, Duration::from_millis(50));
            let _ = d.take_if_due(now);
        }
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_millis(50),
            "10 000 debounce cycles took {elapsed:?}; the change path must not sleep"
        );
    }

    /// A port nobody is holding, chosen by the kernel. Never hardcode one: a test
    /// that binds 31313 collides with a real `velox dev` on the same machine.
    fn a_free_port() -> u16 {
        let l = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind an ephemeral port");
        l.local_addr().expect("a local addr").port()
    }

    /// The banner must not claim hot reload when the listener could not bind.
    ///
    /// This is the defect: the line read `App started (HMR enabled)` on a run
    /// where the bind had already failed on another thread and printed its own
    /// complaint to stderr. The test drives the real `HmrListener::start` against
    /// a port it is holding, so the `Unavailable` it builds is the one a real run
    /// would build.
    #[test]
    fn a_port_in_use_makes_the_banner_say_unavailable_and_never_enabled() {
        let squatter = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let port = squatter.local_addr().expect("a local addr").port();

        let err = match HmrListener::start(port) {
            Ok(_) => panic!("the port was already bound; the start must fail"),
            Err(e) => e,
        };
        assert_eq!(
            err.kind(),
            std::io::ErrorKind::AddrInUse,
            "expected AddrInUse on a held port, got {err:?}"
        );

        let status = HmrStatus::Unavailable {
            reason: format!("port {port} {}", port_reason(&err)),
        };
        assert!(
            !status.is_available(),
            "a failed bind must not read as available"
        );
        let line = status.app_started_line();
        assert!(
            line.contains("unavailable") && line.contains(&port.to_string()),
            "the started-app line must name the unavailable port and say so: {line:?}"
        );
        assert!(
            !line.contains("enabled"),
            "the started-app line claimed HMR on a run with no listener: {line:?}"
        );
        let banner = status.banner_line(port);
        assert!(
            !banner.contains("auto-reload"),
            "the banner advertised auto-reload with no listener: {banner:?}"
        );
        assert!(
            banner.contains("unavailable") && banner.contains("in use"),
            "the banner must say why hot reload is missing: {banner:?}"
        );
    }

    /// The other direction, so the honest string cannot be achieved by always
    /// claiming failure.
    #[test]
    fn a_free_port_makes_the_banner_say_enabled() {
        let port = a_free_port();
        let listener = HmrListener::start(port).expect("a free port must bind");
        assert!(
            HmrStatus::Available.is_available(),
            "a successful bind must read as available"
        );
        assert_eq!(
            HmrStatus::Available.app_started_line(),
            "App started (HMR enabled)"
        );
        assert!(
            HmrStatus::Available
                .banner_line(port)
                .contains("auto-reload on save"),
            "the banner must advertise auto-reload when the listener is up"
        );
        drop(listener);
    }

    /// The invariant behind test 4 of the task: after the listener is dropped the
    /// port is free again.
    ///
    /// Asserted by re-binding, not by inspecting a flag. A `Drop` that stopped
    /// joining the thread — or that stopped setting the shutdown flag — would keep
    /// the socket until the process exited, and this is the test that notices: it
    /// is the behaviour that makes a wedged port unrecoverable without `--hmr-port`.
    #[test]
    fn dropping_the_listener_frees_the_port() {
        let port = a_free_port();
        let listener = HmrListener::start(port).expect("a free port must bind");
        // Still held: a second bind must fail while the listener is alive.
        assert!(
            std::net::TcpListener::bind(("127.0.0.1", port)).is_err(),
            "the port was free while an HmrListener held it"
        );
        drop(listener);
        std::net::TcpListener::bind(("127.0.0.1", port))
            .expect("the port must be released once the listener is dropped");
    }

    /// `AddrInUse` is the case a user can act on, so it gets words rather than an
    /// errno string. Any other error is passed through untouched: inventing an
    /// explanation for one we do not recognise would be worse than quoting it.
    #[test]
    fn an_addr_in_use_reason_names_the_cause() {
        assert_eq!(
            port_reason(&std::io::Error::from(std::io::ErrorKind::AddrInUse)),
            "in use by another process"
        );
        let other = std::io::Error::other("something else entirely");
        assert_eq!(port_reason(&other), other.to_string());
    }
}
