use anyhow::Result;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime};

const IGNORED: &[&str] = &["target", ".git", ".vscode", ".idea"];

pub fn dev_current(watch_dir: &Path) -> Result<()> {
    println!("\n=== Velox Dev Server ===");
    println!("Watching: {}", watch_dir.display());
    println!("Ctrl+C to quit\n");

    let mut child = spawn_app()?;

    if child.is_some() {
        println!("App started\n");
    }

    let mut last_check = SystemTime::now();

    loop {
        thread::sleep(Duration::from_millis(500));

        if has_change(watch_dir, &mut last_check) {
            println!("Changed, rebuilding...\n");
            if let Some(mut c) = child.take() {
                let _ = c.kill();
                let _ = c.wait();
            }
            child = spawn_app()?;
        }

        if let Some(ref mut c) = child {
            if c.try_wait()?.is_some() {
                println!("App exited\n");
                child = None;
            }
        }
    }
}

fn spawn_app() -> Result<Option<Child>> {
    let mut cmd = Command::new("cargo");
    cmd.arg("run");
    cmd.stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    match cmd.spawn() {
        Ok(c) => Ok(Some(c)),
        Err(e) => {
            log::error!("Start failed: {}", e);
            Ok(None)
        }
    }
}

fn has_change(dir: &Path, last_check: &mut SystemTime) -> bool {
    fn walk(p: &Path, t: SystemTime) -> bool {
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                let path = e.path();
                let n = path.file_name().and_then(|x| x.to_str()).unwrap_or("");
                if n.starts_with('.') || IGNORED.contains(&n) {
                    continue;
                }
                if path.is_dir() {
                    if walk(&path, t) {
                        return true;
                    }
                } else if let Ok(md) = e.metadata() {
                    if let Ok(m) = md.modified() {
                        if m > t {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
    if walk(dir, *last_check) {
        *last_check = SystemTime::now();
        true
    } else {
        false
    }
}
