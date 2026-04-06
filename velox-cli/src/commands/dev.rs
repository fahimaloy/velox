use anyhow::Result;
use crossterm::{
    cursor::MoveTo as CursorMoveTo,
    event::{self, Event, KeyCode},
    execute, queue,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{
        disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use serde_json;
use std::io::{self, Stdout, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime};
use velox_renderer::HmrMessage;

/// DevServerUI struct
pub struct DevServerUI {
    _app_name: String,
    port: u16,
    start_time: Instant,
    stdout: Stdout,
}

impl DevServerUI {
    pub fn new(app_name: &str, port: u16) -> Self {
        Self {
            _app_name: app_name.to_string(),
            port,
            start_time: Instant::now(),
            stdout: io::stdout(),
        }
    }

    pub fn print_banner(&mut self) -> Result<()> {
        let elapsed = self.start_time.elapsed().as_millis();
        execute!(self.stdout, EnterAlternateScreen, Clear(ClearType::All))?;
        queue!(
            self.stdout,
            SetForegroundColor(Color::Rgb {
                r: 139,
                g: 92,
                b: 246
            }),
            Print("\n  Velox Dev Server v0.1.0\n".to_string()),
            ResetColor,
            SetForegroundColor(Color::Rgb {
                r: 16,
                g: 185,
                b: 129
            }),
            Print(format!("  ➜  Local:   http://localhost:{}/\n", self.port)),
            ResetColor,
            SetForegroundColor(Color::Rgb {
                r: 245,
                g: 158,
                b: 11
            }),
            Print("  Ready in "),
            ResetColor,
            SetForegroundColor(Color::Rgb {
                r: 59,
                g: 130,
                b: 246
            }),
            Print(format!("{}ms\n\n", elapsed)),
            ResetColor,
            SetForegroundColor(Color::Rgb {
                r: 236,
                g: 72,
                b: 153
            }),
            Print("  ➜ Key bindings: "),
            ResetColor,
            SetForegroundColor(Color::Rgb {
                r: 99,
                g: 102,
                b: 241
            }),
            Print("'r' "),
            ResetColor,
            Print("for reload, "),
            SetForegroundColor(Color::Rgb {
                r: 99,
                g: 102,
                b: 241
            }),
            Print("'h' "),
            ResetColor,
            Print("for hot reload, "),
            SetForegroundColor(Color::Rgb {
                r: 99,
                g: 102,
                b: 241
            }),
            Print("'q' "),
            ResetColor,
            Print("to quit.\n\n")
        )?;
        self.stdout.flush()?;
        Ok(())
    }

    pub fn print_file_change(&mut self, path: &str) -> Result<()> {
        queue!(
            self.stdout,
            CursorMoveTo(0, 8),
            Clear(ClearType::CurrentLine),
            SetForegroundColor(Color::Rgb {
                r: 245,
                g: 158,
                b: 11
            }),
            Print(format!("  📝 File changed: {}\n", path)),
            ResetColor
        )?;
        self.stdout.flush()?;
        Ok(())
    }

    pub fn print_rebuild(&mut self) -> Result<()> {
        queue!(
            self.stdout,
            CursorMoveTo(0, 9),
            Clear(ClearType::CurrentLine),
            SetForegroundColor(Color::Rgb {
                r: 245,
                g: 158,
                b: 11
            }),
            Print("  🔄 Rebuilding..."),
            ResetColor
        )?;
        self.stdout.flush()?;
        Ok(())
    }

    pub fn print_rebuild_success(&mut self) -> Result<()> {
        queue!(
            self.stdout,
            CursorMoveTo(0, 9),
            Clear(ClearType::CurrentLine),
            SetForegroundColor(Color::Rgb {
                r: 16,
                g: 185,
                b: 129
            }),
            Print("  ✅ Rebuild complete ✓"),
            ResetColor
        )?;
        println!();
        self.stdout.flush()?;
        Ok(())
    }

    pub fn print_reload(&mut self, reload_type: &str) -> Result<()> {
        queue!(
            self.stdout,
            CursorMoveTo(0, 10),
            Clear(ClearType::CurrentLine),
            SetForegroundColor(Color::Rgb {
                r: 16,
                g: 185,
                b: 129
            }),
            Print(if reload_type == "hot" {
                "  ♻️  Hot reload complete"
            } else {
                "  🔄 Full reload complete"
            }),
            ResetColor
        )?;
        println!();
        self.stdout.flush()?;
        Ok(())
    }

    pub fn print_error(&mut self, msg: &str) -> Result<()> {
        queue!(
            self.stdout,
            CursorMoveTo(0, 11),
            Clear(ClearType::CurrentLine),
            SetForegroundColor(Color::Rgb {
                r: 239,
                g: 68,
                b: 68
            }),
            Print(format!("  ❌ Error: {}\n", msg)),
            ResetColor
        )?;
        self.stdout.flush()?;
        Ok(())
    }

    pub fn print_reloading(&mut self) -> Result<()> {
        queue!(
            self.stdout,
            CursorMoveTo(0, 10),
            Clear(ClearType::CurrentLine),
            SetForegroundColor(Color::Rgb {
                r: 245,
                g: 158,
                b: 11
            }),
            Print("  🔄 Reloading..."),
            ResetColor
        )?;
        self.stdout.flush()?;
        Ok(())
    }

    pub fn print_reloaded(&mut self) -> Result<()> {
        queue!(
            self.stdout,
            CursorMoveTo(0, 10),
            Clear(ClearType::CurrentLine),
            SetForegroundColor(Color::Rgb {
                r: 16,
                g: 185,
                b: 129
            }),
            Print("  ✅ Reloaded ✓"),
            ResetColor
        )?;
        println!();
        self.stdout.flush()?;
        Ok(())
    }

    pub fn print_quitting(&mut self) -> Result<()> {
        queue!(
            self.stdout,
            CursorMoveTo(0, 11),
            Clear(ClearType::CurrentLine),
            SetForegroundColor(Color::Rgb {
                r: 156,
                g: 163,
                b: 175
            }),
            Print("  👋 Goodbye!\n"),
            ResetColor
        )?;
        self.stdout.flush()?;
        Ok(())
    }

    pub fn print_rebuilding(&mut self) -> Result<()> {
        self.print_rebuild()
    }

    pub fn print_restarted(&mut self) -> Result<()> {
        self.print_reloaded()
    }

    pub fn print_exited_failed(&mut self, msg: &str) -> Result<()> {
        self.print_error(msg)
    }

    pub fn print_exited_success(&mut self) -> Result<()> {
        queue!(
            self.stdout,
            CursorMoveTo(0, 11),
            Clear(ClearType::CurrentLine),
            SetForegroundColor(Color::Rgb {
                r: 16,
                g: 185,
                b: 129
            }),
            Print("  👋 App exited successfully\n"),
            ResetColor
        )?;
        self.stdout.flush()?;
        Ok(())
    }

    pub fn print_reload_failed(&mut self, err: &str) -> Result<()> {
        self.print_error(err)
    }

    pub fn print_restart_failed(&mut self, err: &str) -> Result<()> {
        self.print_error(err)
    }
}

fn send_hmr_message(socket_path: &Path, msg: &HmrMessage) -> Result<()> {
    let mut stream = UnixStream::connect(socket_path)?;
    let json = serde_json::to_vec(msg)?;
    stream.write_all(&json)?;
    Ok(())
}

/// Dev server with hot reload
pub fn dev_app_hmr(pkg: &str, watch_dir: &Path) -> Result<()> {
    // Enable raw mode for non-blocking key input
    enable_raw_mode()?;

    let mut ui = DevServerUI::new(pkg, 3000);
    ui.print_banner()?;

    let (tx, rx) = mpsc::channel::<char>();

    // Spawn raw key reader thread
    thread::spawn(move || loop {
        if let Ok(Event::Key(event)) = event::read() {
            match event.code {
                KeyCode::Char('r') | KeyCode::Char('R') => {
                    let _ = tx.send('r');
                }
                KeyCode::Char('h') | KeyCode::Char('H') => {
                    let _ = tx.send('h');
                }
                KeyCode::Char('q') | KeyCode::Char('Q') => {
                    let _ = tx.send('q');
                    break;
                }
                KeyCode::Esc => {
                    let _ = tx.send('q');
                    break;
                }
                _ => {}
            }
        }
    });

    let socket_path: PathBuf =
        std::env::temp_dir().join(format!("velox-hmr-{}", std::process::id()));
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }

    let mut last = latest_mtime(watch_dir);
    let mut cached_files: Vec<(PathBuf, SystemTime)> = Vec::new();
    let mut cached_dir_mtime: SystemTime = SystemTime::UNIX_EPOCH;
    let mut last_rebuild_time = Instant::now() - DEBOUNCE_DURATION; // Allow immediate first rebuild

    let mut child: Option<Child> = {
        let mut cmd = Command::new("cargo");
        if pkg == "." {
            cmd.arg("run");
        } else {
            cmd.args(["run", "-p", pkg]);
        }
        cmd.arg("--hmr-socket").arg(&socket_path);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        match cmd.spawn() {
            Ok(c) => Some(c),
            Err(e) => {
                eprintln!("[dev] Initial run failed: {}", e);
                ui.print_error(&format!("Initial run failed: {}", e))?;
                None
            }
        }
    };

    loop {
        thread::sleep(Duration::from_millis(300));

        // Handle commands
        if let Ok(cmd) = rx.try_recv() {
            match cmd {
                'r' => {
                    ui.print_reloading()?;
                    if socket_path.exists() {
                        let _ = send_hmr_message(&socket_path, &HmrMessage::FullReload);
                    }
                    if let Some(mut c) = child.take() {
                        let _ = c.kill();
                        let _ = c.wait();
                    }
                    let mut new_cmd = Command::new("cargo");
                    if pkg == "." {
                        new_cmd.arg("run");
                    } else {
                        new_cmd.args(["run", "-p", pkg]);
                    }
                    new_cmd.arg("--hmr-socket").arg(&socket_path);
                    new_cmd
                        .stdin(Stdio::null())
                        .stdout(Stdio::inherit())
                        .stderr(Stdio::inherit());
                    match new_cmd.spawn() {
                        Ok(c) => {
                            child = Some(c);
                            ui.print_reloaded()?;
                        }
                        Err(e) => {
                            ui.print_reload_failed(&e.to_string())?;
                        }
                    }
                }
                'h' => {
                    ui.print_reloading()?;
                    if socket_path.exists() {
                        send_hmr_message(
                            &socket_path,
                            &HmrMessage::HotReload {
                                module_path: "".to_string(),
                            },
                        )?;
                    }
                    ui.print_reload("hot")?;
                }
                'q' => {
                    ui.print_quitting()?;
                    if let Some(mut c) = child.take() {
                        let _ = c.kill();
                        let _ = c.wait();
                    }
                    break;
                }
                _ => {}
            }
        }

        // Handle file changes with debounce
        let now = latest_mtime_cached(watch_dir, &mut cached_files, &mut cached_dir_mtime);

        if now > last && last_rebuild_time.elapsed() >= DEBOUNCE_DURATION {
            last = now;
            last_rebuild_time = Instant::now();
            ui.print_file_change(watch_dir.display().to_string().as_str())?;
            ui.print_rebuild()?;
            if socket_path.exists() {
                send_hmr_message(
                    &socket_path,
                    &HmrMessage::HotReload {
                        module_path: watch_dir.to_string_lossy().to_string(),
                    },
                )?;
            }
            ui.print_rebuild_success()?;
            ui.print_reload("hot")?;

            if child.is_some() {
                continue;
            }

            // If app is not running, restart on change
            ui.print_rebuilding()?;
            let mut new_cmd = Command::new("cargo");
            if pkg == "." {
                new_cmd.arg("run");
            } else {
                new_cmd.args(["run", "-p", pkg]);
            }
            new_cmd.arg("--hmr-socket").arg(&socket_path);
            new_cmd
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit());
            match new_cmd.spawn() {
                Ok(c) => {
                    child = Some(c);
                    ui.print_restarted()?;
                }
                Err(e) => {
                    ui.print_restart_failed(&e.to_string())?;
                }
            }
        }

        // Check if child exited
        if let Some(c) = &mut child {
            if let Some(status) = c.try_wait()? {
                if !status.success() {
                    ui.print_exited_failed(&status.to_string())?;
                } else {
                    ui.print_exited_success()?;
                }
                child = None;
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(ui.stdout, LeaveAlternateScreen)?;
    Ok(())
}

/// File extension filter: only watch these extensions
const WATCHED_EXTENSIONS: &[&str] = &["vx", "rs", "css", "toml", "json"];

/// Directory names to ignore
const IGNORED_DIRS: &[&str] = &[
    "target",
    ".git",
    ".vscode",
    ".idea",
    "node_modules",
    ".qwen",
];

/// File patterns to ignore (by extension or suffix)
const IGNORED_FILE_PATTERNS: &[&str] = &["lock", "pid", "swp"];

/// Debounce duration: minimum time between rebuilds
const DEBOUNCE_DURATION: Duration = Duration::from_millis(500);

/// Check if a file should be watched based on extension
fn is_watched_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| WATCHED_EXTENSIONS.contains(&ext))
        .unwrap_or(false)
}

/// Check if a path component should be ignored
fn should_ignore_component(name: &str) -> bool {
    // Ignore hidden files/directories (starting with .)
    if name.starts_with('.') {
        return true;
    }
    // Ignore specific directories
    if IGNORED_DIRS.contains(&name) {
        return true;
    }
    // Check file patterns
    if let Some(ext) = Path::new(name).extension().and_then(|e| e.to_str()) {
        if IGNORED_FILE_PATTERNS.contains(&ext) {
            return true;
        }
    }
    // Ignore files ending with ~
    if name.ends_with('~') {
        return true;
    }
    false
}

/// Collect all watched file paths and their mtimes from the directory tree.
/// Respects extension filters and ignore patterns.
fn collect_watched_files(dir: &Path) -> Vec<(PathBuf, SystemTime)> {
    fn walk(p: &Path, files: &mut Vec<(PathBuf, SystemTime)>) {
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                let path = e.path();
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                if should_ignore_component(name) {
                    continue;
                }

                if path.is_dir() {
                    walk(&path, files);
                } else if is_watched_extension(&path) {
                    if let Ok(md) = e.metadata() {
                        if let Ok(m) = md.modified() {
                            files.push((path.clone(), m));
                        }
                    }
                }
            }
        }
    }
    let mut files = Vec::new();
    walk(dir, &mut files);
    files
}

/// Get the latest mtime among watched files, using cached file list.
/// Only re-scans if any directory's mtime has changed.
fn latest_mtime_cached(
    dir: &Path,
    cached_files: &mut Vec<(PathBuf, SystemTime)>,
    cached_dir_mtime: &mut SystemTime,
) -> SystemTime {
    // Check if directory tree mtime changed
    let dir_mtime = latest_mtime(dir);

    // Only re-collect if directory structure changed
    if dir_mtime > *cached_dir_mtime {
        *cached_dir_mtime = dir_mtime;
        *cached_files = collect_watched_files(dir);
    }

    cached_files
        .iter()
        .map(|(_, t)| *t)
        .max()
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

fn latest_mtime(dir: &Path) -> SystemTime {
    fn walk(p: &Path, cur: &mut SystemTime) {
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                let path = e.path();
                // Skip ignored directories and hidden dirs
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if should_ignore_component(name) {
                    continue;
                }
                if path.is_dir() {
                    walk(&path, cur);
                } else if let Ok(md) = e.metadata() {
                    if let Ok(m) = md.modified() {
                        if m > *cur {
                            *cur = m;
                        }
                    }
                }
            }
        }
    }
    let mut t = SystemTime::UNIX_EPOCH;
    walk(dir, &mut t);
    t
}

/// Dev server for current project
pub fn dev_current(watch_dir: &Path) -> Result<()> {
    dev_app_hmr(".", watch_dir)
}
