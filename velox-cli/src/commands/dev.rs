use anyhow::Result;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime};

/// Dev server with hot reload
pub fn dev_app(pkg: &str, watch_dir: &Path) -> Result<()> {
    let mut child: Option<Child>;
    let (tx, rx) = mpsc::channel::<char>();
    
    // Spawn stdin reader
    thread::spawn(move || {
        let mut buf = [0u8; 1];
        let stdin = io::stdin();
        let mut handle = stdin.lock();
        loop {
            if let Ok(n) = handle.read(&mut buf) {
                if n == 0 { break; }
                let ch = buf[0] as char;
                if ch == 'r' || ch == 'R' || ch == 'q' || ch == 'Q' {
                    let _ = tx.send(ch.to_ascii_lowercase());
                }
            } else { break; }
        }
    });
    
    let mut last = latest_mtime(watch_dir);
    
    println!(
        "[dev] Watching {} (press 'r' to reload, 'q' to quit - in CLI or window)",
        watch_dir.display()
    );

    child = match spawn_app(pkg) {
        Ok(c) => Some(c),
        Err(e) => {
            eprintln!("[dev] Initial run failed: {e}");
            eprintln!("[dev] Waiting for file changes or manual reload ('r')...");
            None
        }
    };
    
    loop {
        thread::sleep(Duration::from_millis(300));
        let now = latest_mtime(watch_dir);
        
        // Handle commands
        if let Ok(cmd) = rx.try_recv() {
            match cmd {
                'r' => {
                    println!("[dev] Reloading...");
                    if let Some(mut c) = child.take() {
                        let _ = c.kill();
                        let _ = c.wait();
                    }
                    match spawn_app(pkg) {
                        Ok(c) => {
                            child = Some(c);
                            println!("[dev] ✓ Reloaded");
                        }
                        Err(e) => {
                            eprintln!("[dev] ✗ Reload failed: {e}");
                            eprintln!("[dev] Dev server still running; fix files and press 'r' or save changes.");
                        }
                    }
                }
                'q' => {
                    println!("[dev] Quit requested");
                    if let Some(mut c) = child.take() {
                        let _ = c.kill();
                        let _ = c.wait();
                    }
                    break;
                }
                _ => {}
            }
        }
        
        // Keep running app alive on normal file edits; only hard reload on 'r'.
        if now > last {
            last = now;
            println!("[dev] Change detected");

            if child.is_some() {
                println!("[dev] App is still running; keeping window alive. Press 'r' for hard reload.");
                continue;
            }

            // If app is not running (crashed/exited), try to recover on change.
            print!("[dev] Rebuilding after change");
            io::stdout().flush()?;
            for _ in 0..5 {
                print!(".");
                io::stdout().flush()?;
                thread::sleep(Duration::from_millis(120));
            }
            println!();

            match spawn_app(pkg) {
                Ok(c) => {
                    child = Some(c);
                    println!("[dev] Restarted after change");
                }
                Err(e) => {
                    eprintln!("[dev] Restart after change failed: {e}");
                    eprintln!("[dev] Waiting for next file update...");
                }
            }
        }
        
        // Do not exit dev server when child exits or fails.
        if let Some(c) = &mut child {
            if let Some(status) = c.try_wait()? {
                if !status.success() {
                    eprintln!("[dev] App exited with failure (status: {status}). Waiting for file changes...");
                } else {
                    println!("[dev] App exited successfully. Waiting for file changes...");
                }
                child = None;
            }
        }
    }
    
    Ok(())
}

fn spawn_app(pkg: &str) -> std::io::Result<Child> {
    let mut cmd = Command::new("cargo");
    if pkg == "." {
        cmd.arg("run");
    } else {
        cmd.args(["run", "-p", pkg]);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
}

fn latest_mtime(dir: &Path) -> SystemTime {
    fn walk(p: &Path, cur: &mut SystemTime) {
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                let path = e.path();
                if path.is_dir() {
                    walk(&path, cur);
                } else if let Ok(md) = e.metadata() {
                    if let Ok(m) = md.modified() {
                        if m > *cur { *cur = m; }
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
    dev_app(".", watch_dir)
}
