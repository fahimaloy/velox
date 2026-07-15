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

#[derive(Clone)]
enum DevCmd {
    Reload,
    Clear,
    Quit,
}

/// Start the Velox dev server in `project_dir` (builds with `cargo run`,
/// watching `<project_dir>/src` for changes).
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

    let mut child = spawn_app(project_dir, release)?;
    let mut last_check = SystemTime::now();
    let mut crashed = false;

    loop {
        // Handle interactive commands from the user.
        match rx.try_recv() {
            Ok(DevCmd::Quit) => {
                if let Some(mut c) = child.take() {
                    let _ = c.kill();
                }
                println!("\n{} {}", bold("👋"), "Dev server stopped.");
                break;
            }
            Ok(DevCmd::Clear) => {
                clear_screen();
                print_banner(project_dir, release, &watch_dir);
                if crashed {
                    println!("{} {}\n", red("✗ App crashed."), dim("Fix the error and save to rebuild."));
                }
            }
            Ok(DevCmd::Reload) => {
                println!("{}", yellow("↻ Manual reload requested"));
                if let Some(mut c) = child.take() {
                    let _ = c.kill();
                    let _ = c.wait();
                }
                child = spawn_app(project_dir, release)?;
                crashed = false;
                last_check = SystemTime::now();
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => break,
        }

        // Detect file changes (debounced).
        if let Some(changed) = changed_file(&watch_dir, &mut last_check) {
            println!(
                "{} {}",
                yellow("↻"),
                format!("{} changed — rebuilding", changed.display())
            );
            if let Some(mut c) = child.take() {
                let _ = c.kill();
                let _ = c.wait();
            }
            child = spawn_app(project_dir, release)?;
            crashed = false;
        }

        // If the app exited, report but keep watching (Vite-like resilience).
        if let Some(ref mut c) = child {
            if c.try_wait()?.is_some() {
                println!("{}", dim("App exited. Press 'r' to restart, or save a file to rebuild."));
                child = None;
                crashed = true;
            }
        }

        thread::sleep(Duration::from_millis(400));
    }

    Ok(())
}

fn print_banner(project_dir: &Path, release: bool, watch_dir: &Path) {
    let name = project_dir
        .canonicalize()
        .ok()
        .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
        .unwrap_or_else(|| "velox-app".into());
    println!();
    println!("  {} {}", bold(&cyan("⚡ Velox dev server")), dim(&format!("v{}", env!("CARGO_PKG_VERSION"))));
    println!("  {} {}", bold("➤ Project:"), name);
    println!("  {} {}", bold("➤ Watching:"), watch_dir.display());
    println!(
        "  {} {}",
        bold("➤ Build:"),
        if release { "release" } else { "debug" }
    );
    println!(
        "  {}",
        dim("  r: reload   c: clear   q: quit")
    );
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

fn spawn_app(project_dir: &Path, release: bool) -> Result<Option<Child>> {
    // Phase 1: build with piped output so we can surface compile errors
    // in a clean panel (Vite-style) without a crashing window.
    println!("{}", dim("⏳ Compiling..."));
    let start = SystemTime::now();
    let mut build = Command::new("cargo");
    build.arg("build");
    if release {
        build.arg("--release");
    }
    build
        .current_dir(project_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let build_output = match build.output() {
        Ok(o) => o,
        Err(e) => {
            println!("{} {}", red("✗"), format!("Failed to invoke cargo: {}", e));
            return Ok(None);
        }
    };
    let elapsed = start.elapsed().unwrap_or_default().as_secs_f32();

    if !build_output.status.success() {
        let stderr = String::from_utf8_lossy(&build_output.stderr).to_string();
        print_build_error(&stderr);
        return Ok(None);
    }

    println!("{} {}", green("✓"), format!("Compiled in {:.1}s", elapsed));

    // Phase 2: run the freshly built binary with inherited stdio so the
    // GUI window appears and stays alive while we watch for changes.
    let mut run = Command::new("cargo");
    run.arg("run");
    if release {
        run.arg("--release");
    }
    run.current_dir(project_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    match run.spawn() {
        Ok(c) => {
            println!("{}", green("App started"));
            Ok(Some(c))
        }
        Err(e) => {
            log::error!("Start failed: {}", e);
            println!("{} {}", red("✗"), format!("Failed to start: {}", e));
            Ok(None)
        }
    }
}

fn print_build_error(stderr: &str) {
    let lines: Vec<&str> = stderr.lines().collect();
    let errors: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| l.contains("error[") || l.contains("error:") || l.contains("cannot find") || l.contains("expected"))
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
                } else if let Ok(md) = e.metadata() {
                    if let Ok(m) = md.modified() {
                        if m > t {
                            return Some(path.strip_prefix(base).unwrap_or(&path).to_path_buf());
                        }
                    }
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
