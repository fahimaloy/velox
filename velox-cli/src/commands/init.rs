use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

/// Initialize a new Velox project
pub fn init_project(name: &str) -> Result<PathBuf> {
    let project_dir = PathBuf::from(name);
    let project_name = project_dir
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("invalid project path/name: {name}"))?;

    // Create directory structure
    fs::create_dir_all(&project_dir)?;
    fs::create_dir_all(project_dir.join("src"))?;
    fs::create_dir_all(project_dir.join("assets"))?;

    // Write files from templates
    let cargo_toml = generate_cargo_toml(project_name);
    fs::write(project_dir.join("Cargo.toml"), cargo_toml)?;
    
    let main_rs = generate_main_rs();
    fs::write(project_dir.join("src/main.rs"), main_rs)?;
    
    let app_vx = generate_app_vx();
    fs::write(project_dir.join("src/App.vx"), app_vx)?;
    
    let readme = generate_readme(project_name);
    fs::write(project_dir.join("README.md"), readme)?;

    let build_rs = generate_build_rs();
    fs::write(project_dir.join("build.rs"), build_rs)?;

    println!("✅ Created Velox project: {}", project_dir.display());
    println!("📦 To get started:");
    println!("   cd {}", project_dir.display());
    println!("   cargo build");
    println!("   cargo run");
    
    Ok(project_dir)
}

/// Initialize a new example app inside examples/
pub fn init_app(name: &str) -> Result<PathBuf> {
    let root = PathBuf::from("examples").join(name);
    let src = root.join("src");
    fs::create_dir_all(&src).with_context(|| format!("create {}", src.display()))?;

    let cargo = format!(r#"[package]
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
"#);
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
    format!(r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[dependencies]
velox-core = {{ git = "https://github.com/fahimaloy/velox" }}
velox-dom = {{ git = "https://github.com/fahimaloy/velox" }}
velox-style = {{ git = "https://github.com/fahimaloy/velox" }}
velox-renderer = {{ git = "https://github.com/fahimaloy/velox" }}

[build-dependencies]
velox-cli = {{ git = "https://github.com/fahimaloy/velox" }}
"#)
}

fn generate_main_rs() -> String {
    r#"use velox_style::Stylesheet;

include!(concat!(env!("OUT_DIR"), "/App.rs"));

fn main() {
    println!("🚀 Starting Velox app...");

    let state = app::script_rs::State::new();
    let _vnode = app::render_with(|name| match name {
        "title" => state.title.borrow().clone(),
        "count" => state.count.get().to_string(),
        _ => String::new(),
    });
    let _sheet = Stylesheet::parse(app::STYLE);

    println!("✅ App rendered successfully!");
}
"#.to_string()
}

fn generate_app_vx() -> String {
    r#"<template>
  <div class="app">
    <header class="header">
      <h1>{{ title }}</h1>
    </header>
    <main class="content">
      <div class="card">
        <p>Welcome to Velox! 🚀</p>
        <button class="btn" @click="handleClick">Click me</button>
      </div>
    </main>
  </div>
</template>

<script setup>
use std::cell::{Cell, RefCell};

pub struct State {
  pub count: Cell<i32>,
  pub title: RefCell<String>,
}

impl State {
  pub fn new() -> Self {
    Self {
      count: Cell::new(0),
      title: RefCell::new("Hello Velox!".to_string()),
    }
  }

  pub fn handleClick(&self) {
    let v = self.count.get() + 1;
    self.count.set(v);
  }
}
</script>

<style>
.app {
  display: flex;
  flex-direction: column;
  min-height: 100vh;
  background-color: #f5f5f5;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
}

.header {
  background-color: #333;
  color: white;
  padding: 1rem;
  text-align: center;
}

.content {
  flex: 1;
  display: flex;
  justify-content: center;
  align-items: center;
  padding: 2rem;
}

.card {
  background: white;
  border-radius: 8px;
  padding: 2rem;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
  text-align: center;
}

.btn {
  background-color: #007bff;
  color: white;
  border: none;
  border-radius: 4px;
  padding: 0.5rem 1rem;
  font-size: 1rem;
  cursor: pointer;
  margin-top: 1rem;
}

.btn:hover {
  background-color: #0056b3;
}
</style>
"#.to_string()
}

fn generate_readme(name: &str) -> String {
    format!(r#"# {name}

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
"#)
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
"#.to_string()
}
