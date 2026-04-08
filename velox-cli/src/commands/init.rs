use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

/// Initialize a new Velox project
pub fn init_project(name: &str) -> Result<PathBuf> {
    let requested_dir = PathBuf::from(name);
    let requested_name = requested_dir
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("invalid project path/name: {name}"))?;

    let package_name = crate::validate_and_normalize_package_name(requested_name)?;
    let project_dir = if requested_dir.components().count() == 1 {
        PathBuf::from(&package_name)
    } else {
        requested_dir
    };

    // Create directory structure
    fs::create_dir_all(&project_dir)?;
    fs::create_dir_all(project_dir.join("src"))?;
    fs::create_dir_all(project_dir.join("assets"))?;

    // Write files from templates
    let cargo_toml = generate_cargo_toml(&package_name);
    fs::write(project_dir.join("Cargo.toml"), cargo_toml)?;

    let main_rs = generate_main_rs();
    fs::write(project_dir.join("src/main.rs"), main_rs)?;

    let app_vx = generate_app_vx();
    fs::write(project_dir.join("src/App.vx"), app_vx)?;

    let readme = generate_readme(&package_name);
    fs::write(project_dir.join("README.md"), readme)?;

    let build_rs = generate_build_rs();
    fs::write(project_dir.join("build.rs"), build_rs)?;

    println!("✅ Created Velox project: {}", project_dir.display());
    println!("📦 To get started:");
    println!("   cd {}", project_dir.display());
    println!("   velox dev");
    println!("   velox build");
    println!("   velox run");

    Ok(project_dir)
}

/// Initialize a new example app inside examples/
pub fn init_app(name: &str) -> Result<PathBuf> {
    let root = PathBuf::from("examples").join(name);
    let src = root.join("src");
    fs::create_dir_all(&src).with_context(|| format!("create {}", src.display()))?;

    let cargo = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[dependencies]
velox-core = {{ path = "../../velox-core" }}
velox-dom = {{ path = "../../velox-dom" }}
velox-style = {{ path = "../../velox-style" }}
velox-renderer = {{ path = "../../velox-renderer" }}

[build-dependencies]
velox-cli = {{ path = "../../velox-cli" }}
"#
    );
    fs::write(root.join("Cargo.toml"), cargo).context("write Cargo.toml")?;

    let app_vx = r#"<template>
  <div class="app">
    <button class="btn" @click="inc">Increment</button>
    <button class="btn" @click="dec">Decrement</button>
    <div class="count">{{ count }}</div>
  </div>
</template>

<script setup>
use std::cell::{Cell, RefCell};
pub struct State { pub count: Cell<i32>, pub title: RefCell<String> }
impl State {
  pub fn new() -> Self { Self { count: Cell::new(0), title: RefCell::new("Velox App".into()) } }
  pub fn inc(&self) { let v = self.count.get()+1; self.count.set(v); }
  pub fn dec(&self) { let v = self.count.get()-1; self.count.set(v); }
}
</script>

<style>
  .app { display: flex; flex-direction: column; padding: 20px; }
  .btn { background: #3478f6; color: white; padding: 10px 20px; margin: 5px; }
  .count { margin-top: 12px; font-size: 24px; }
</style>
"#;
    fs::write(src.join("App.vx"), app_vx).context("write App.vx")?;

    let build_rs = r#"fn main() {
    println!("cargo:rerun-if-changed=src/App.vx");
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/App.vx");
    velox_cli::build_cmd(&input, None, velox_cli::EmitMode::Render).expect("compile App.vx");
}
"#;
    fs::write(root.join("build.rs"), build_rs).context("write build.rs")?;

    let main_rs = r#"use velox_dom::VNode;
use velox_style::Stylesheet;

include!(concat!(env!("OUT_DIR"), "/App.rs"));

fn main() {
    let state = app::script_rs::State::new();
    let vnode = render_with(|name| if name == "count" { state.count.get().to_string() } else { String::new() });
    let sheet = Stylesheet::parse(app::STYLE);
    println!("App rendered successfully!");
}
"#;
    fs::write(src.join("main.rs"), main_rs).context("write main.rs")?;

    Ok(root)
}

fn generate_cargo_toml(name: &str) -> String {
    format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[workspace]

[dependencies]
velox-core = {{ git = "https://github.com/fahimaloy/velox" }}
velox-dom = {{ git = "https://github.com/fahimaloy/velox" }}
velox-style = {{ git = "https://github.com/fahimaloy/velox" }}
velox-renderer = {{ git = "https://github.com/fahimaloy/velox", features = ["skia-native"] }}

[build-dependencies]
velox-cli = {{ git = "https://github.com/fahimaloy/velox" }}
"#
    )
}

fn generate_main_rs() -> String {
    r#"use std::io::Read;
use std::env::args;
use std::thread;
use serde_json;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;
use std::path::PathBuf;
use velox_dom::VNode;
use velox_style::Stylesheet;
use velox_renderer::HmrMessage;

// Platform-specific socket imports
#[cfg(unix)]
use std::os::unix::net::UnixListener;
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, FromRawHandle};
#[cfg(windows)]
use std::fs::OpenOptions;

include!(concat!(env!("OUT_DIR"), "/App.rs"));

fn main() {
  println!("Starting Velox app...");

  let args: Vec<String> = args().collect();
  let hmr_path = if let Some(i) = args.iter().position(|a| a == "--hmr-socket") {
    args.get(i+1).cloned()
  } else {
    None
  };

  if let Some(path_str) = hmr_path {
    #[cfg(unix)]
    {
      let path = PathBuf::from(path_str);
      let listener = UnixListener::bind(&path).expect("bind socket");
      let (tx, rx) = mpsc::channel();
      let rx = std::sync::Arc::new(std::sync::Mutex::new(rx));
      thread::spawn(move || {
        for stream in listener.incoming() {
          if let Ok(mut stream) = stream {
            let mut buffer = Vec::new();
            stream.read_to_end(&mut buffer).ok();
            if let Ok(msg) = serde_json::from_slice::<HmrMessage>(&buffer) {
              tx.send(msg).ok();
            }
          }
        }
      });

      let state = Arc::new(app::script_rs::State::new());

      let make_view = {
        let state = Arc::clone(&state);
        move |_w: u32, _h: u32| -> (VNode, Stylesheet) {
          let vnode = app::render_with_state(Arc::clone(&state), |name| match name {
            "title" => state.title.get(),
            "count" => state.count.get().to_string(),
            _ => String::new(),
          });
          let sheet = Stylesheet::parse(app::STYLE);
          (vnode, sheet)
        }
      };

      let on_event = app::make_on_event(Arc::clone(&state));
      let get_title = {
        let state = Arc::clone(&state);
        move || state.title.get()
      };

      velox_renderer::run_window_vnode_skia("Velox App", make_view, on_event, get_title);
    }
    #[cfg(windows)]
    {
      // On Windows, use named pipes for HMR
      let pipe_name = format!("\\\\.\\pipe\\{}", path_str);
      let (tx, rx) = mpsc::channel();
      let rx = std::sync::Arc::new(std::sync::Mutex::new(rx));
      thread::spawn(move || {
        // Create named pipe server
        use std::io::Read;
        let listener = OpenOptions::new()
          .read(true)
          .write(true)
          .create(true)
          .open(&pipe_name)
          .expect("create named pipe");
        
        let mut buffer = Vec::new();
        let mut stream = listener;
        stream.read_to_end(&mut buffer).ok();
        if let Ok(msg) = serde_json::from_slice::<HmrMessage>(&buffer) {
          tx.send(msg).ok();
        }
      });

      let state = Arc::new(app::script_rs::State::new());

      let make_view = {
        let state = Arc::clone(&state);
        move |_w: u32, _h: u32| -> (VNode, Stylesheet) {
          let vnode = app::render_with_state(Arc::clone(&state), |name| match name {
            "title" => state.title.get(),
            "count" => state.count.get().to_string(),
            _ => String::new(),
          });
          let sheet = Stylesheet::parse(app::STYLE);
          (vnode, sheet)
        }
      };

      let on_event = app::make_on_event(Arc::clone(&state));
      let get_title = {
        let state = Arc::clone(&state);
        move || state.title.get()
      };

      velox_renderer::run_window_vnode_skia("Velox App", make_view, on_event, get_title);
    }
    #[cfg(not(any(unix, windows)))]
    {
      eprintln!("HMR not supported on this platform");
    }
  } else {
    let state = Arc::new(app::script_rs::State::new());

    let make_view = {
      let state = Arc::clone(&state);
      move |_w: u32, _h: u32| -> (VNode, Stylesheet) {
        let vnode = app::render_with_state(Arc::clone(&state), |name| match name {
          "title" => state.title.get().to_string(),
          "count" => state.count.get().to_string(),
          _ => String::new(),
        });
        let sheet = Stylesheet::parse(app::STYLE);
        (vnode, sheet)
      }
    };

    let on_event = app::make_on_event(Arc::clone(&state));
    let get_title = {
      let state = Arc::clone(&state);
      move || state.title.get()
    };

    velox_renderer::run_window_vnode_skia("Velox App", make_view, on_event, get_title);
  }
}
"#
    .to_string()
}

fn generate_app_vx() -> String {
    r#"<template>
  <div class="app">
    <header class="header">
      <h1>{{ title }}</h1>
    </header>
    <main class="content">
      <div class="card">
        <p>Welcome to Velox!</p>
        <p>Current count: {{ count }}</p>
        <button class="btn" @click="handle_click">Click me</button>
      </div>
    </main>
  </div>
</template>

<script setup>
use std::rc::Rc;
use velox_core::signal::Signal;

pub struct State {
  pub count: Rc<Signal<i32>>,
  pub title: Rc<Signal<String>>,
}

impl State {
  pub fn new() -> Self {
    Self {
      count: Rc::new(Signal::new(0)),
      title: Rc::new(Signal::new(String::from("Hello Velox!"))),
    }
  }

  pub fn handle_click(&self) {
    self.count.set(self.count.get() + 1);
    self.title.set(format!("Hello Velox! ({})", self.count.get()));
  }
}
</script>

<style>
.app {
  display: flex;
  flex-direction: column;
  min-height: 100vh;
  margin: 0px;
  padding: 0px;
  background-color: #f5f7fa;
  color: #1f2937;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
}

.header {
  background-color: #1f2937;
  color: #ffffff;
  padding: 24px 32px;
  text-align: center;
}

.header h1 {
  margin: 0px;
  padding: 0px;
  font-size: 28px;
  font-weight: 700;
  color: #ffffff;
}

.content {
  flex: 1;
  display: flex;
  justify-content: center;
  align-items: center;
  padding: 32px;
}

.card {
  background: #ffffff;
  border: 1px solid #e5e7eb;
  border-radius: 8px;
  padding: 32px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1);
  text-align: center;
  max-width: 600px;
}

.card p {
  margin: 0px;
  padding: 0px;
  font-size: 16px;
  color: #374151;
}

.card p + p {
  margin-top: 8px;
}

.btn {
  background-color: #3b82f6;
  color: white;
  border: none;
  border-radius: 6px;
  padding: 12px 24px;
  font-size: 16px;
  font-weight: 500;
  cursor: pointer;
  margin-top: 16px;
  transition: background-color 0.2s ease;
}

.btn:hover {
  background-color: #2563eb;
}

.btn:active {
  background-color: #1d4ed8;
}
</style>
"#
    .to_string()
}

fn generate_readme(name: &str) -> String {
    format!(
        r#"# {name}

A Velox application.

## Getting Started

### Prerequisites

- Rust toolchain (1.70+)
- Velox CLI: `cargo install velox-cli`

### Development

```bash
# Install dependencies
cargo build

# Run in development mode
cargo run

# Build for production
cargo build --release
```

### Project Structure

```
{name}/
├── Cargo.toml
├── build.rs
├── src/
│   ├── main.rs       # Application entry point
│   └── App.vx        # Main component
├── assets/           # Static assets
└── README.md
```

## Documentation

For full documentation, visit: https://velox.dev/docs

## License

MIT
"#
    )
}

fn generate_build_rs() -> String {
    r#"fn main() {
    println!("cargo:rerun-if-changed=src/App.vx");

    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/App.vx");

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());

    // Compile App.vx to Rust
    velox_cli::build_cmd(&input, Some(&out_dir), velox_cli::EmitMode::Render
    ).expect("Failed to compile App.vx");
}
"#
    .to_string()
}
