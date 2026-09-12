use anyhow::Result;
use std::io::{BufRead, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, TryRecvError};
use std::thread;
use std::time::{Duration, SystemTime};

const IGNORED: &[&str] = &["target", ".git", ".vscode", ".idea"];

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

#[derive(Clone)]
enum DevCmd {
    Reload,
    Clear,
    Quit,
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
    spawn_stdin_reader(tx);

    // Start HMR TCP server in a background thread.
    // The app connects to this listener as a client.
    // The connected stream is stored in a shared slot so we can send
    // reload messages to the app when files change.
    let hmr_slot = start_hmr_listener(velox_renderer::DEFAULT_HMR_PORT);

    let _ = hmr_slot; // listener thread is running in background
    let mut child = spawn_app_hmr(project_dir, release, velox_renderer::DEFAULT_HMR_PORT)?;
    let mut last_check = SystemTime::now();
    let mut crashed = false;

    loop {
        // Handle interactive commands from the user.
        match rx.try_recv() {
            Ok(DevCmd::Quit) => {
                if let Some(mut c) = child.take() {
                    let _ = c.kill();
                }
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
                if let Some(mut c) = child.take() {
                    let _ = c.kill();
                    let _ = c.wait();
                }
                child = spawn_app_hmr(project_dir, release, velox_renderer::DEFAULT_HMR_PORT)?;
                crashed = false;
                last_check = SystemTime::now();
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => break,
        }

        // Detect file changes (debounced).
        if let Some(changed) = changed_file(&watch_dir, &mut last_check) {
            println!(
                "{} {} changed — rebuilding",
                yellow("↻"),
                changed.display()
            );

            // Try to send HMR reload to the still-running app.
            // The app will receive FullReload and exit, then we'll restart it.
            send_hmr_reload(&hmr_slot);

            // Wait briefly for the app to exit (the HMR FullReload causes it to exit).
            if let Some(ref mut c) = child {
                // Give the app up to 2 seconds to exit gracefully after HMR.
                for _ in 0..20 {
                    if c.try_wait()?.is_some() {
                        break;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            }

            // Kill if still running and restart.
            if let Some(mut c) = child.take() {
                let _ = c.kill();
                let _ = c.wait();
            }

            child = spawn_app_hmr(project_dir, release, velox_renderer::DEFAULT_HMR_PORT)?;
            crashed = false;
        }

        // If the app exited, report but keep watching (Vite-like resilience).
        if let Some(ref mut c) = child
            && c.try_wait()?.is_some()
        {
            println!(
                "{}",
                dim("App exited. Press 'r' to restart, or save a file to rebuild.")
            );
            child = None;
            crashed = true;
        }

        thread::sleep(Duration::from_millis(400));
    }

    Ok(())
}

/// Shared slot that holds the most recently connected HMR client stream.
/// The app connects to our TCP listener; we store the stream here so we
/// can send messages to the app when files change.
type HmrSlot = std::sync::Arc<std::sync::Mutex<Option<std::net::TcpStream>>>;

/// Start the HMR TCP listener on `port`. Returns an `HmrSlot` that the
/// dev server can use to send messages to the connected app.
///
/// The listener runs in a background thread. When the app connects, the
/// stream is stored in the shared slot. When the app disconnects (or
/// reconnects after a restart), the slot is updated.
fn start_hmr_listener(port: u16) -> HmrSlot {
    let slot: HmrSlot = std::sync::Arc::new(std::sync::Mutex::new(None));

    let slot_clone = std::sync::Arc::clone(&slot);
    thread::spawn(move || {
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

        loop {
            match listener.accept() {
                Ok((stream, addr)) => {
                    eprintln!("[velox] HMR client connected: {}", addr);
                    // Store the connected stream so the dev server can send to it.
                    *slot_clone.lock().unwrap() = Some(stream);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(e) => {
                    eprintln!("[velox] HMR accept error: {}", e);
                    thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }
    });

    slot
}

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
        "  {} {}",
        bold("➤ HMR:"),
        format!("port {} (auto-reload on save)", velox_renderer::DEFAULT_HMR_PORT)
    );
    println!(
        "  {} {}",
        bold("➤ Build:"),
        if release { "release" } else { "debug" }
    );
    println!("  {}", dim("  r: reload   c: clear   q: quit"));
    println!();
}

fn spawn_stdin_reader(tx: mpsc::Sender<DevCmd>) {
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
    });
}

/// Spawn the app with HMR enabled. The app connects to our HMR port
/// and can receive reload messages.
fn spawn_app_hmr(project_dir: &Path, release: bool, hmr_port: u16) -> Result<Option<Child>> {
    let bin = project_bin_name(project_dir);

    // Phase 1: build with piped output so we can surface compile errors
    // in a clean panel (Vite-style) without a crashing window.
    println!("{}", dim("⏳ Compiling..."));
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
        .current_dir(project_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let build_output = match build.output() {
        Ok(o) => o,
        Err(e) => {
            println!("{} Failed to invoke cargo: {}", red("✗"), e);
            return Ok(None);
        }
    };
    let elapsed = start.elapsed().unwrap_or_default().as_secs_f32();

    if !build_output.status.success() {
        let stderr = String::from_utf8_lossy(&build_output.stderr).to_string();
        print_build_error(&stderr);
        return Ok(None);
    }

    println!("{} Compiled in {:.1}s", green("✓"), elapsed);

    // Phase 2: run the freshly built binary, passing HMR env vars.
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
            Ok(Some(c))
        }
        Err(e) => {
            log::error!("Start failed: {}", e);
            println!("{} Failed to start: {}", red("✗"), e);
            Ok(None)
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

/// Returns the first changed file (relative to `dir`) if anything changed
/// since `last_check`, and updates `last_check`. Includes a small debounce.
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
        // Debounce: wait a moment then re-check to coalesce rapid saves.
        thread::sleep(Duration::from_millis(150));
    }
    found
}
