use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Walk up from the current directory looking for a directory that contains
/// the velox workspace marker (velox-core/Cargo.toml). Returns the workspace root.
fn find_velox_workspace() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let mut current = cwd.as_path();
    loop {
        let marker = current.join("velox-core").join("Cargo.toml");
        if marker.exists() {
            return Some(current.to_path_buf());
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => return None,
        }
    }
}

/// Compute the relative path from `from` to `to`.
/// For example, if `from` is `/a/b/c/project` and `to` is `/a/b/velox-core`,
/// the result is `../../velox-core`.
fn compute_relative_path(from: &Path, to: &Path) -> PathBuf {
    // Canonicalize both paths so they share a common absolute prefix
    let from_canonical = from.canonicalize().unwrap_or_else(|_| from.to_path_buf());
    let to_canonical = to.canonicalize().unwrap_or_else(|_| to.to_path_buf());

    // Collect path components, filtering out the RootDir
    let from_comps: Vec<_> = from_canonical
        .components()
        .filter(|c| !matches!(c, std::path::Component::RootDir))
        .collect();
    let to_comps: Vec<_> = to_canonical
        .components()
        .filter(|c| !matches!(c, std::path::Component::RootDir))
        .collect();

    // Find common prefix length
    let mut common_len = 0;
    for (a, b) in from_comps.iter().zip(to_comps.iter()) {
        if a == b {
            common_len += 1;
        } else {
            break;
        }
    }

    // Build relative path: go up from `from`, then down to `to`
    let mut result = PathBuf::new();
    for _ in common_len..from_comps.len() {
        result.push("..");
    }
    for comp in &to_comps[common_len..] {
        result.push(comp);
    }

    if result.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        result
    }
}

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
    let cargo_toml = generate_cargo_toml(&package_name, &project_dir);
    fs::write(project_dir.join("Cargo.toml"), cargo_toml)?;

    let main_rs = generate_main_rs();
    fs::write(project_dir.join("src/main.rs"), main_rs)?;

    let app_vx = generate_app_vx();
    fs::write(project_dir.join("src/App.vx"), app_vx)?;

    fs::create_dir_all(project_dir.join("src/components"))?;
    fs::write(
        project_dir.join("src/components/TodoItem.vx"),
        generate_todo_item_vx(),
    )?;

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
  .app { display: flex; flex-direction: column; padding: 20px; background: #1a1a2e; color: #e6edf3; font-family: system-ui, sans-serif; }
  .btn { background: #3478f6; color: white; padding: 10px 20px; margin: 5px; }
  .count { margin-top: 12px; font-size: 24px; }
</style>
"#;
    fs::write(src.join("App.vx"), app_vx).context("write App.vx")?;

    let build_rs = r#"fn main() {
    // build_cmd in Render mode recursively compiles the input .vx file
    // and all imported components. It emits cargo:rerun-if-changed
    // directives for every .vx file it reads.
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/App.vx");
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    velox_cli::build_cmd(&input, Some(&out_dir), velox_cli::EmitMode::Render).expect("compile App.vx");
}
"#;
    fs::write(root.join("build.rs"), build_rs).context("write build.rs")?;

    let main_rs = r#"use velox_dom::VNode;
use velox_style::Stylesheet;

include!(concat!(env!("OUT_DIR"), "/app.rs"));

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

fn generate_cargo_toml(name: &str, project_dir: &Path) -> String {
    // Try to find the velox workspace root by walking up from CWD
    let workspace_root = find_velox_workspace();

    if let Some(workspace) = &workspace_root {
        // Compute relative paths from project to each velox crate
        let core_path = compute_relative_path(project_dir, &workspace.join("velox-core"));
        let dom_path = compute_relative_path(project_dir, &workspace.join("velox-dom"));
        let style_path = compute_relative_path(project_dir, &workspace.join("velox-style"));
        let renderer_path = compute_relative_path(project_dir, &workspace.join("velox-renderer"));
        let cli_path = compute_relative_path(project_dir, &workspace.join("velox-cli"));

        format!(
            r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[workspace]

[dependencies]
velox-core = {{ path = "{}" }}
velox-dom = {{ path = "{}" }}
velox-style = {{ path = "{}" }}
velox-renderer = {{ path = "{}", features = ["skia-native"] }}
serde_json = "1.0"

[build-dependencies]
velox-cli = {{ path = "{}" }}
"#,
            core_path.display(),
            dom_path.display(),
            style_path.display(),
            renderer_path.display(),
            cli_path.display()
        )
    } else {
        // No workspace found — fall back to git dependencies
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
serde_json = "1.0"

[build-dependencies]
velox-cli = {{ git = "https://github.com/fahimaloy/velox" }}
"#
        )
    }
}

fn generate_main_rs() -> String {
    r#"use std::sync::Arc;
use velox_dom::VNode;
use velox_style::Stylesheet;

include!(concat!(env!("OUT_DIR"), "/app.rs"));

fn main() {
    println!("Starting {}...", env!("CARGO_PKG_NAME"));

    let state = Arc::new(app::script_rs::State::new());

    let make_view = {
        let state = Arc::clone(&state);
        move |_w: u32, _h: u32| -> (VNode, Stylesheet) {
            let vnode = app::render_with_state(Arc::clone(&state), |name| match name {
                "title" => state.title(),
                "counter" => state.counter().to_string(),
                "positive" => state.positive().to_string(),
                _ => String::new(),
            });
            let sheet = Stylesheet::parse(app::STYLE);
            (vnode, sheet)
        }
    };

    let on_event = app::make_on_event(Arc::clone(&state));
    let get_title = {
        let state = Arc::clone(&state);
        move || state.title()
    };

    let _ = velox_renderer::run_window_vnode_skia("Velox App", make_view, on_event, get_title);
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
    <div class="card">
      <p class="count">{{ counter }}</p>
      <p v-if="positive" class="positive">positive</p>
      <p v-else class="neutral">not positive</p>
      <button class="btn" @click="increment">+1</button>
      <button class="btn" @click="decrement">-1</button>
      <button class="btn" @click="reset">Reset</button>
    </div>
    <div class="card">
      <h2>Todo List</h2>
      <TodoItem />
    </div>
  </div>
</template>

<script setup>
import TodoItem from './components/TodoItem.vx'
use std::cell::{Cell, RefCell};

pub struct State {
    pub counter: Cell<i32>,
}

impl State {
    pub fn new() -> Self {
        Self {
            counter: Cell::new(0),
        }
    }

    pub fn title(&self) -> String { String::from("Velox App") }
    pub fn counter(&self) -> i32 { self.counter.get() }
    pub fn positive(&self) -> bool { self.counter.get() > 0 }
    pub fn increment(&self) { self.counter.set(self.counter.get() + 1); }
    pub fn decrement(&self) { self.counter.set(self.counter.get() - 1); }
    pub fn reset(&self) { self.counter.set(0); }
}
</script>

<style>
.app { display: flex; flex-direction: column; width: 100%; height: 100%; background: #1a1a2e; color: #e6edf3; font-family: system-ui, sans-serif; padding: 20px; }
.header { padding: 20px; text-align: center; }
.card { background: #16213e; padding: 24px; border-radius: 12px; text-align: center; margin-bottom: 16px; }
.count { font-size: 48px; font-weight: bold; margin: 0; }
.positive { color: #3fb950; margin: 8px 0; }
.neutral { color: #8b949e; margin: 8px 0; }
.btn { padding: 10px 20px; font-size: 16px; background: #3478f6; color: white; border: none; border-radius: 6px; cursor: pointer; margin: 4px; }
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
    // build_cmd in Render mode recursively compiles the input .vx file
    // and all imported components. It emits cargo:rerun-if-changed
    // directives for every .vx file it reads.
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/App.vx");

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());

    velox_cli::build_cmd(&input, Some(&out_dir), velox_cli::EmitMode::Render
    ).expect("Failed to compile App.vx");
}
"#
    .to_string()
}

fn generate_todo_item_vx() -> String {
    r#"<template>
  <div class="todo-item" :class="{ completed: completed }">
    <input
      type="checkbox"
      class="checkbox"
      :checked="completed"
      @click="on_toggle"
    />
    <span class="todo-text">{{ text }}</span>
    <button class="btn btn-danger btn-small" @click="on_remove">×</button>
  </div>
</template>

<script setup>
// Props: receive todo data from parent component
// :todo="item" passes the todo text
// :completed="true/false" passes completion status
// @toggle and @remove are event handlers

pub struct Props {
    pub todo: String,
    pub completed: bool,
}

pub struct State {
    pub props: Props,
}

impl State {
    pub fn new() -> Self {
        Self {
            props: Props {
                todo: String::new(),
                completed: false,
            }
        }
    }

    pub fn text(&self) -> String {
        self.props.todo.clone()
    }

    pub fn completed(&self) -> bool {
        self.props.completed
    }

    pub fn on_toggle(&self) {}
    pub fn on_remove(&self) {}
}
</script>

<style>
.todo-item { display: flex; align-items: center; justify-content: space-between; padding: 14px 16px; border-bottom: 1px solid #f3f4f6; }
.todo-item.completed .todo-text { text-decoration: line-through; color: #9ca3af; }
.todo-text { font-size: 15px; color: #374151; flex: 1; margin-left: 12px; }
.checkbox { width: 20px; height: 20px; cursor: pointer; accent-color: #3b82f6; }
.btn-small { padding: 4px 10px; font-size: 16px; border-radius: 4px; }
.btn-danger { background-color: #ef4444; color: #ffffff; }
</style>
"#
    .to_string()
}
