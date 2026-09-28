use anyhow::Result;
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

const IGNORED: &[&str] = &["target", ".git", ".vscode", ".idea"];

/// How long the dev loop blocks waiting for the next event before it gives up
/// and re-scans for file changes. It is a *timeout*, not a poll interval: a
/// keystroke on stdin or a finished build wakes the loop immediately, so input
/// latency and rebuild latency are no longer quantised to this value.
const LOOP_TIMEOUT: Duration = Duration::from_millis(400);

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

/// Owns the HMR listener thread.
///
/// The listener's `accept()` loop is non-blocking and polls, so it can be given a
/// real shutdown path: `drop` sets the flag and joins. It is joined rather than
/// detached because it owns a bound TCP socket, and a detached listener would
/// keep the port until the process exited.
pub struct HmrListener {
    slot: HmrSlot,
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl HmrListener {
    fn start(port: u16) -> Self {
        let slot: HmrSlot = Arc::new(Mutex::new(None));
        let shutdown = Arc::new(AtomicBool::new(false));

        let slot_clone = Arc::clone(&slot);
        let shutdown_clone = Arc::clone(&shutdown);
        let handle = thread::spawn(move || {
            let listener = match std::net::TcpListener::bind(("127.0.0.1", port)) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!(
                        "[velox] HMR server could not bind port {}: {} (continuing without HMR)",
                        port, e
                    );
                    return;
                }
            };
            listener.set_nonblocking(true).ok();
            eprintln!("[velox] HMR dev server listening on 127.0.0.1:{}", port);

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

        Self {
            slot,
            shutdown,
            handle: Some(handle),
        }
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

    print_banner(project_dir, release, &watch_dir);

    let (tx, rx) = mpsc::channel::<DevCmd>();
    // `tx` stays alive in this scope for the whole function — the loop hands a
    // clone to every `BuildWorker`, and build results come back on this same
    // channel. That is what makes `RecvTimeoutError::Disconnected` unreachable
    // here, so stdin EOF is detected on the reader's own handle instead.
    let stdin = InputSource::from_handle(spawn_stdin_reader(tx.clone()));

    // Start HMR TCP server in a background thread.
    // The app connects to this listener as a client.
    // The connected stream is stored in a shared slot so we can send
    // reload messages to the app when files change. `hmr` owns the thread and
    // stops it when this function returns.
    let hmr = HmrListener::start(velox_renderer::DEFAULT_HMR_PORT);
    let hmr_port = velox_renderer::DEFAULT_HMR_PORT;

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
    let mut last_check = SystemTime::now();
    let mut crashed = false;

    loop {
        // True when something this iteration wants a rebuild for.
        let mut change_requested = false;

        // Block until the next event. A keystroke or a finished build wakes the
        // loop straight away; `LOOP_TIMEOUT` is only how long we wait before
        // deciding it is time to look for file changes again.
        match rx.recv_timeout(LOOP_TIMEOUT) {
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
                print_banner(project_dir, release, &watch_dir);
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
                // A manual reload is a change, but not a *file* change: advance
                // the scan clock so the build we are about to run does not come
                // back around as a detected change.
                last_check = SystemTime::now();
                change_requested = true;
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
                            child = spawn_app(&project, release, bin.clone(), hmr_port)
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

        if let Some(changed) = changed_file(&watch_dir, &mut last_check) {
            println!("{} {} changed — rebuilding", yellow("↻"), changed.display());
            change_requested = true;
        }

        if change_requested && gate.on_change() == BuildAction::Start {
            // Tell the app to reload, then stop waiting for it to oblige. The
            // old code gave it a 2 s grace here, which added up to 2 s to every
            // reload and bought nothing: the `kill()` that followed was
            // unconditional, so the grace only ever affected ordering.
            send_hmr_reload(hmr.slot());
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

fn print_banner(project_dir: &Path, release: bool, watch_dir: &Path) {
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
        "  {} port {} (auto-reload on save)",
        bold("➤ HMR:"),
        velox_renderer::DEFAULT_HMR_PORT
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

/// Spawn the already-compiled app with HMR enabled.
///
/// Split from the build because the two have different blocking profiles. The
/// build is the slow part and runs on a [`BuildWorker`] thread; this is a single
/// `Command::spawn`, which returns as soon as the process exists and so is safe
/// to call straight from the dev loop.
fn spawn_app(
    project_dir: &Path,
    release: bool,
    bin: Option<String>,
    hmr_port: u16,
) -> Option<Child> {
    let mut run = Command::new("cargo");
    run.arg("run");
    if release {
        run.arg("--release");
    }
    if let Some(ref name) = bin {
        run.arg("--bin").arg(name);
    }
    // Enable HMR mode in the app — it will connect to our TCP server.
    run.env("VELOX_HMR", "1")
        .env("VELOX_HMR_PORT", hmr_port.to_string())
        .current_dir(project_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    match run.spawn() {
        Ok(c) => {
            println!("{}", green("App started (HMR enabled)"));
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

/// Returns the first changed file (relative to `dir`) if anything changed since
/// `last_check`, and advances `last_check` to now.
///
/// There is deliberately no sleep here. The previous version stamped
/// `*last_check` *before* a 150 ms wait and never re-scanned afterwards, so it
/// was not a debounce: it stamped the clock, slept, and a save landing inside
/// that window had `mtime > last_check` on the next scan and therefore *caused*
/// a second full `cargo build` rather than being absorbed. Coalescing now lives
/// in [`BuildGate`], which is a state machine rather than a sleep, so it covers
/// the whole duration of a build instead of a fixed 150 ms.
fn changed_file(dir: &Path, last_check: &mut SystemTime) -> Option<std::path::PathBuf> {
    fn walk(p: &Path, t: SystemTime, base: &Path) -> Option<std::path::PathBuf> {
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                let path = e.path();
                let n = path.file_name().and_then(|x| x.to_str()).unwrap_or("");
                if n.starts_with('.') || IGNORED.contains(&n) {
                    continue;
                }
                if path.is_dir() {
                    if let Some(f) = walk(&path, t, base) {
                        return Some(f);
                    }
                } else if let Ok(md) = e.metadata()
                    && let Ok(m) = md.modified()
                    && m > t
                {
                    return Some(path.strip_prefix(base).unwrap_or(&path).to_path_buf());
                }
            }
        }
        None
    }
    let found = walk(dir, *last_check, dir);
    if found.is_some() {
        *last_check = SystemTime::now();
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    static SEQ: AtomicU32 = AtomicU32::new(0);

    /// A self-deleting temp directory.
    ///
    /// `tempfile` is not a dependency of this crate (and adding one would churn
    /// the lockfile mid-programme), so the handful of lines it would provide are
    /// inlined here instead.
    struct TempTree(PathBuf);

    impl TempTree {
        fn new() -> Self {
            let n = SEQ.fetch_add(1, Ordering::Relaxed);
            let p = std::env::temp_dir().join(format!("velox-dev-fs-{}-{}", std::process::id(), n));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).expect("create temp tree");
            Self(p)
        }

        /// Create `rel` (with any parent directories).
        fn write(&self, rel: &str) {
            let p = self.0.join(rel);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).expect("create parent");
            }
            std::fs::write(&p, b"x").expect("write file");
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A clock that makes every file on disk look freshly changed, so a test is
    /// about *which* files the walk accepts rather than about mtime resolution.
    fn everything_changed() -> SystemTime {
        SystemTime::UNIX_EPOCH
    }

    /// `target/` must be pruned even when it sits directly under the watched
    /// root.
    ///
    /// This is the assertion that can actually fail. It is easy to write a
    /// version of this test that passes for the wrong reason — the dev server
    /// normally watches `<project>/src`, and `target/` lives at the project root,
    /// so the path is never even in scope. Pointing `changed_file` at a root that
    /// *does* contain `target/` puts the name filter on the path it is meant to
    /// guard: delete `"target"` from `IGNORED` and this returns `Some("junk.txt")`.
    #[test]
    fn a_change_under_target_is_ignored() {
        let t = TempTree::new();
        t.write("target/debug/junk.txt");
        let mut last_check = everything_changed();
        assert_eq!(
            changed_file(&t.0, &mut last_check),
            None,
            "files under target/ must not trigger a rebuild"
        );
    }

    #[test]
    fn a_change_under_a_dot_directory_is_ignored() {
        let t = TempTree::new();
        t.write(".git/HEAD");
        t.write(".vscode/settings.json");
        let mut last_check = everything_changed();
        assert_eq!(
            changed_file(&t.0, &mut last_check),
            None,
            "dot-directories must not trigger a rebuild"
        );
    }

    /// The counterpart to the two above: the filter must not be so broad that a
    /// real edit is missed. Without this, deleting `IGNORED` entirely would make
    /// the exclusion tests green by accident.
    #[test]
    fn a_source_file_change_is_reported() {
        let t = TempTree::new();
        t.write("keep.txt");
        let mut last_check = everything_changed();
        assert_eq!(
            changed_file(&t.0, &mut last_check),
            Some(PathBuf::from("keep.txt"))
        );
    }

    #[test]
    fn a_source_file_nested_below_the_root_is_reported_relative_to_it() {
        let t = TempTree::new();
        t.write("components/only.vx");
        let mut last_check = everything_changed();
        assert_eq!(
            changed_file(&t.0, &mut last_check),
            Some(PathBuf::from("components/only.vx"))
        );
    }

    /// Nothing to report means the clock must not move either, or a later scan
    /// would compare against a time that was never used to justify a rebuild.
    #[test]
    fn an_ignored_change_does_not_advance_the_clock() {
        let t = TempTree::new();
        t.write("target/debug/junk.txt");
        let before = everything_changed();
        let mut last_check = before;
        assert_eq!(changed_file(&t.0, &mut last_check), None);
        assert_eq!(last_check, before, "clock advanced with no change found");
    }

    /// The scan must not block the dev loop.
    ///
    /// The removed code slept 150 ms *after* finding a change, on the dev loop
    /// itself, which is what this pins. Three scans of a one-entry tree cost
    /// microseconds, so the bound is orders of magnitude above the real cost
    /// while still being half the one sleep it replaced; three calls with the
    /// sleep restored take at least 450 ms.
    #[test]
    fn scanning_does_not_sleep() {
        let t = TempTree::new();
        t.write("keep.txt");
        let start = std::time::Instant::now();
        for _ in 0..3 {
            let mut last_check = everything_changed();
            let _ = changed_file(&t.0, &mut last_check);
        }
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_millis(150),
            "3 scans took {elapsed:?}; the scan must not sleep"
        );
    }
}
