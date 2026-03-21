use anyhow::Result;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime};

/// Dev server with hot reload
pub fn dev_app(pkg: &str, watch_dir: &Path) -> Result<()> {
    let mut child: Option<Child> = None;
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
    
    println!("[dev] Watching {} (press 'r' to reload, 'q' to quit)", watch_dir.display());
    
    child = Some(spawn_app(pkg)?);
    
    loop {
        thread::sleep(Duration::from_millis(300));
        let now = latest_mtime(watch_dir);
        
        // Handle commands
        if let Ok(cmd) = rx.try_recv() {
            match cmd {
                'r' => {
                    println!("[dev] Manual reload triggered");
                    if let Some(mut c) = child.take() {
                        let _ = c.kill();
                        let _ = c.wait();
                    }
                    print!("[dev] Reloading");
                    io::stdout().flush()?;
                    for _ in 0..5 {
                        print!(".");
                        io::stdout().flush()?;
                        thread::sleep(Duration::from_millis(120));
                    }
                    println!();
                    child = Some(spawn_app(pkg)?);
                    println!("[dev] Reloaded");
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
        
        // Check for file changes
        if now > last {
            println!("[dev] Change detected - reloading");
            last = now;
            if let Some(mut c) = child.take() {
                let _ = c.kill();
                let _ = c.wait();
            }
            print!("[dev] Rebuilding");
            io::stdout().flush()?;
            for _ in 0..5 {
                print!(".");
                io::stdout().flush()?;
                thread::sleep(Duration::from_millis(120));
            }
            println!();
            child = Some(spawn_app(pkg)?);
            println!("[dev] Restarted");
        }
        
        // Check if child exited
        if let Some(c) = &mut child {
            if let Some(status) = c.try_wait()? {
                if !status.success() {
                    anyhow::bail!("dev run exited with failure")
                } else {
                    break;
                }
            }
        }
    }
    
    Ok(())
}

fn spawn_app(pkg: &str) -> std::io::Result<Child> {
    Command::new("cargo")
        .args(["run", "-p", pkg])
        .stdin(Stdio::null())
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
