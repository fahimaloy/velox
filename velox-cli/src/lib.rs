use anyhow::{Context, Result};
use clap::ValueEnum;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use std::process::{Command, Stdio, Child};
use std::io::{self, Read};
use std::sync::mpsc;
use std::thread;

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum EmitMode {
    Stub,
    Render,
}

/// Build a .vx/.vue file into a Rust module written to `out_dir`.
pub fn build_cmd(input: &Path, out_dir: Option<&Path>, emit: EmitMode) -> Result<()> {
    let src =
        fs::read_to_string(input).with_context(|| format!("failed to read {}", input.display()))?;

    let sfc = velox_sfc::parse_sfc(&src).map_err(|e| anyhow::anyhow!(e))?;

    let name = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("component");

    let mut code = String::new();

    match emit {
        EmitMode::Stub => {
            code.push_str(&velox_sfc::to_stub_rs(&sfc, name));
        }
        EmitMode::Render => {
            let tpl_src = sfc
                .template
                .as_ref()
                .map(|t| t.content.as_str())
                .unwrap_or("");
            let render_fn =
                velox_sfc::compile_template_to_rs(tpl_src, name).map_err(|e| anyhow::anyhow!(e))?;
            // Emit both stub constants and render() in one file
            code.push_str(&velox_sfc::to_stub_rs(&sfc, name));
            code.push_str("\n");
            code.push_str(&render_fn);
            code.push_str("\n");
        }
    }

    let out_dir = out_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("target/velox-gen"));
    fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create {}", out_dir.display()))?;

    let out_path = out_dir.join(format!("{}.rs", name));
    fs::write(&out_path, code)
        .with_context(|| format!("failed to write {}", out_path.display()))?;

    println!("Generated: {}", out_path.display());
    Ok(())
}

/// Create a new example app inside `examples/<name>` with minimal boilerplate.
pub fn init_app(name: &str) -> Result<PathBuf> {
    let root = PathBuf::from("examples").join(name);
    let src = root.join("src");
    fs::create_dir_all(&src).with_context(|| format!("create {}", src.display()))?;

    // Cargo.toml with path deps to workspace crates
    let cargo = format!(r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2024"

[dependencies]
velox-core = {{ path = "../../velox-core" }}
velox-dom = {{ path = "../../velox-dom" }}
velox-style = {{ path = "../../velox-style" }}
velox-renderer = {{ path = "../../velox-renderer", features = ["wgpu"] }}

[build-dependencies]
velox-cli = {{ path = "../../velox-cli" }}
"#);
    fs::write(root.join("Cargo.toml"), cargo).context("write Cargo.toml")?;

    // minimal .vx component for future expansions
    let app_vx = r#"<template>
  <div class="app">0</div>
</template>
<style>
  .app { font-weight: bold; }
</style>
"#;
    fs::write(src.join("App.vx"), app_vx).context("write App.vx")?;

    // build.rs compiles App.vx into OUT_DIR/App.rs
    let build_rs = r#"fn main() {
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/App.vx");
    velox_cli::build_cmd(&input, Some(&std::path::Path::new(&std::env::var("OUT_DIR").unwrap())), velox_cli::EmitMode::Render).expect("compile App.vx");
}
"#;
    fs::write(root.join("build.rs"), build_rs).context("write build.rs")?;

    // main.rs includes generated file and calls render(); applies styles and mounts
    let main_rs = r#"use velox_dom::VNode;
use velox_style::{Stylesheet, apply_styles};
use velox_renderer::Renderer;

include!(concat!(env!("OUT_DIR"), "/App.rs"));

fn main() {
    // Open a basic window
    std::thread::spawn(|| velox_renderer::run_window("Velox App"));

    // Render VNode from compiled template
    let vnode = render();
    // Apply styles from the SFC style block
    let sheet = Stylesheet::parse(STYLE);
    let styled = apply_styles(&vnode, &sheet);

    // Mount using the selected renderer
    let renderer = velox_renderer::new_selected_renderer();
    let tree = renderer.mount(&styled);
    println!("mounted nodes={}, texts={}", tree.node_count, tree.text_count);
}
"#;
    fs::write(src.join("main.rs"), main_rs).context("write main.rs")?;
    // Add to workspace members if present
    if let Err(e) = add_to_workspace_members(&PathBuf::from("Cargo.toml"), &format!("examples/{}", name)) {
        eprintln!("warning: could not update workspace members: {e}");
    }
    Ok(root)
}

/// Run an app package via cargo run -p <pkg>
pub fn run_app(pkg: &str) -> Result<()> {
    let status = Command::new("cargo").args(["run", "-p", pkg]).status()?;
    if !status.success() { anyhow::bail!("app run failed") }
    Ok(())
}

/// Build an app package via cargo build
pub fn build_app(pkg: &str, release: bool) -> Result<()> {
    let mut args = vec!["build", "-p", pkg];
    if release { args.push("--release"); }
    let status = Command::new("cargo").args(&args).status()?;
    if !status.success() { anyhow::bail!("app build failed") }
    Ok(())
}

/// Crude polling-based dev server: runs `cargo run -p <pkg>` and restarts on file changes.
pub fn dev_app(pkg: &str, watch_dir: &Path) -> Result<()> {
    fn latest_mtime(dir: &Path) -> SystemTime {
        fn walk(p: &Path, cur: &mut SystemTime) {
            if let Ok(rd) = std::fs::read_dir(p) {
                for e in rd.flatten() {
                    let path = e.path();
                    if path.is_dir() { walk(&path, cur); }
                    else if let Ok(md) = e.metadata() {
                        if let Ok(m) = md.modified() { if m > *cur { *cur = m; } }
                    }
                }
            }
        }
        let mut t = SystemTime::UNIX_EPOCH;
        walk(dir, &mut t);
        t
    }

    let mut child: Option<Child> = None;
    // command channel: 'r' => full reload, 'q' => quit
    let (tx, rx) = mpsc::channel::<char>();
    thread::spawn(move || {
        // Read single chars from stdin
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
            } else {
                break;
            }
        }
    });
    let mut last = latest_mtime(watch_dir);

    let mut spawn = || -> std::io::Result<Child> {
        Command::new("cargo")
            .args(["run", "-p", pkg])
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
    };

    println!("[dev] Watching {} (press 'r' to reload, 'q' to quit)", watch_dir.display());
    child = Some(spawn()?);
    loop {
        // poll filesystem change
        thread::sleep(Duration::from_millis(300));
        let now = latest_mtime(watch_dir);
        // handle stdin commands non-blocking
        if let Ok(cmd) = rx.try_recv() {
            match cmd {
                'r' => {
                    println!("[dev] Manual reload triggered (r)");
                    if let Some(mut c) = child.take() { let _ = c.kill(); let _ = c.wait(); }
                    print!("[dev] Reloading");
                    io::Write::flush(&mut io::stdout())?;
                    // simple progress dots
                    for _ in 0..5 { print!("."); io::Write::flush(&mut io::stdout())?; thread::sleep(Duration::from_millis(120)); }
                    println!("");
                    child = Some(spawn()?);
                    println!("[dev] Reloaded");
                }
                'q' => {
                    println!("[dev] Quit requested");
                    if let Some(mut c) = child.take() { let _ = c.kill(); let _ = c.wait(); }
                    break;
                }
                _ => {}
            }
        }
        if now > last {
            println!("[dev] Change detected — reloading");
            last = now;
            if let Some(mut c) = child.take() { let _ = c.kill(); let _ = c.wait(); }
            print!("[dev] Rebuilding");
            io::Write::flush(&mut io::stdout())?;
            for _ in 0..5 { print!("."); io::Write::flush(&mut io::stdout())?; thread::sleep(Duration::from_millis(120)); }
            println!("");
            child = Some(spawn()?);
            println!("[dev] Restarted");
        }
        if let Some(c) = &mut child {
            if let Some(status) = c.try_wait()? {
                if !status.success() { anyhow::bail!("dev run exited with failure") } else { break }
            }
        }
    }
    Ok(())
}

fn add_to_workspace_members(cargo_toml: &Path, member: &str) -> Result<()> {
    let mut txt = fs::read_to_string(cargo_toml).context("read workspace Cargo.toml")?;
    if txt.contains(&format!("\"{}\"", member)) {
        return Ok(());
    }
    // naive insertion before closing ] of members array
    if let Some(start) = txt.find("members = [") {
        if let Some(end) = txt[start..].find(']') {
            let insert_pos = start + end;
            let before = &txt[..insert_pos];
            let after = &txt[insert_pos..];
            let new_entry = format!("\n    \"{}\",\n", member);
            txt = format!("{}{}{}", before, new_entry, after);
            fs::write(cargo_toml, txt).context("write workspace Cargo.toml")?;
            return Ok(());
        }
    }
    anyhow::bail!("could not locate workspace members array")
}
