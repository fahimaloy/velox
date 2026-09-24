use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Walk up from the current directory looking for a directory that contains
/// the velox workspace marker (velox-core/Cargo.toml). Returns the workspace root.
pub(crate) fn find_velox_workspace() -> Option<PathBuf> {
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

#[doc(hidden)]
pub fn find_velox_workspace_for_test() -> Option<std::path::PathBuf> {
    find_velox_workspace()
}

/// Compute the relative path from `from` to `to`.
/// For example, if `from` is `/a/b/c/project` and `to` is `/a/b/velox-core`,
/// the result is `../../velox-core`.
fn compute_relative_path(from: &Path, to: &Path) -> PathBuf {
    // Canonicalize both paths so they share a common absolute prefix
    let from_canonical = from.canonicalize().unwrap_or_else(|_| from.to_path_buf());
    let to_canonical = to.canonicalize().unwrap_or_else(|_| to.to_path_buf());

    if !from.exists() || !to.exists() {
        return to.to_path_buf();
    }

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

#[allow(dead_code)]
pub(crate) fn velox_dep_path(
    _prefix: &str,
    workspace: &std::path::Path,
    project_dir: &std::path::Path,
    leaf: &str,
) -> String {
    let p = compute_relative_path(project_dir, &workspace.join(leaf));
    format!(r#"{{ path = "{}", version = "0.1.0" }}"#, p.display())
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

    // Write files from the shipped project template so `velox init` and the
    // checked-in template cannot drift into different event contracts.
    let cargo_toml = generate_cargo_toml(&package_name, &project_dir);
    fs::write(project_dir.join("Cargo.toml"), cargo_toml)?;

    let main_rs = generate_main_rs();
    fs::write(project_dir.join("src/main.rs"), main_rs)?;

    let app_vx = generate_app_vx();
    fs::write(project_dir.join("src/App.vx"), app_vx)?;

    fs::create_dir_all(project_dir.join("src/components"))?;
    fs::write(
        project_dir.join("src/components/Todos.vx"),
        generate_todos_vx(),
    )?;
    fs::write(
        project_dir.join("src/components/TodoInput.vx"),
        generate_todo_input_vx(),
    )?;
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

/// Compatibility shim for the drifted `velox init --template` CLI (Task 3 will flesh out template handling).
pub fn init_project_with_template(name: &str, _template: &str) -> Result<PathBuf> {
    init_project(name)
}

pub fn init_project_with_template_local(
    name: &str,
    template: &str,
    local: Option<&Path>,
) -> Result<PathBuf> {
    if let Some(p) = local {
        unsafe {
            std::env::set_var("VELOX_PATH", p);
        }
    }
    let out = init_project_with_template(name, template);
    if local.is_some() {
        unsafe {
            std::env::remove_var("VELOX_PATH");
        }
    }
    out
}

#[doc(hidden)]
pub fn init_toml_with_local(name: &str, dir: &Path, local: Option<&Path>) -> String {
    if let Some(p) = local {
        unsafe {
            std::env::set_var("VELOX_PATH", p);
        }
    }
    let s = generate_cargo_toml(name, dir);
    unsafe {
        std::env::remove_var("VELOX_PATH");
    }
    s
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
  .app { display: flex; flex-direction: column; width: 100%; min-height: 100vh; padding: 20px; background: #1a1a2e; color: #e6edf3; font-family: system-ui, sans-serif; }
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

pub(crate) fn generate_cargo_toml(name: &str, project_dir: &Path) -> String {
    if let Ok(local) = std::env::var("VELOX_PATH") {
        let ws = std::path::PathBuf::from(local);
        if ws.join("velox-core").join("Cargo.toml").exists() {
            let core_path = compute_relative_path(project_dir, &ws.join("velox-core"));
            let dom_path = compute_relative_path(project_dir, &ws.join("velox-dom"));
            let style_path = compute_relative_path(project_dir, &ws.join("velox-style"));
            let renderer_path = compute_relative_path(project_dir, &ws.join("velox-renderer"));
            let cli_path = compute_relative_path(project_dir, &ws.join("velox-cli"));
            return format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n\n[dependencies]\nvelox-core = {{ path = \"{}\", version = \"0.1.0\" }}\nvelox-dom = {{ path = \"{}\", version = \"0.1.0\" }}\nvelox-style = {{ path = \"{}\", version = \"0.1.0\" }}\nvelox-renderer = {{ path = \"{}\", version = \"0.1.0\", features = [\"skia-native\"] }}\nserde_json = \"1.0\"\n\n[build-dependencies]\nvelox-cli = {{ path = \"{}\", version = \"0.1.0\" }}\n",
                core_path.display(),
                dom_path.display(),
                style_path.display(),
                renderer_path.display(),
                cli_path.display()
            );
        }
    }
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
velox-core = {{ path = "{}", version = "0.1.0" }}
velox-dom = {{ path = "{}", version = "0.1.0" }}
velox-style = {{ path = "{}", version = "0.1.0" }}
velox-renderer = {{ path = "{}", version = "0.1.0", features = ["skia-native"] }}
serde_json = "1.0"

[build-dependencies]
velox-cli = {{ path = "{}", version = "0.1.0" }}
"#,
            core_path.display(),
            dom_path.display(),
            style_path.display(),
            renderer_path.display(),
            cli_path.display()
        )
    } else {
        // No workspace found — fall back to git dependencies
        let rev = crate::velox_git_rev();
        format!(
            "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n\n[dependencies]\nvelox-core = {{ git = \"https://github.com/fahimaloy/velox\", rev = \"{rev}\", version = \"0.1.0\" }}\nvelox-dom = {{ git = \"https://github.com/fahimaloy/velox\", rev = \"{rev}\", version = \"0.1.0\" }}\nvelox-style = {{ git = \"https://github.com/fahimaloy/velox\", rev = \"{rev}\", version = \"0.1.0\" }}\nvelox-renderer = {{ git = \"https://github.com/fahimaloy/velox\", rev = \"{rev}\", version = \"0.1.0\", features = [\"skia-native\"] }}\nserde_json = \"1.0\"\n\n[build-dependencies]\nvelox-cli = {{ git = \"https://github.com/fahimaloy/velox\", rev = \"{rev}\", version = \"0.1.0\" }}\n"
        )
    }
}

fn generate_main_rs() -> String {
    include_str!("../../templates/project/src/main.rs").to_string()
}

fn generate_app_vx() -> String {
    include_str!("../../templates/project/src/App.vx").to_string()
}

fn generate_readme(name: &str) -> String {
    include_str!("../../templates/project/README.md").replace("{{project_name}}", name)
}

fn generate_build_rs() -> String {
    include_str!("../../templates/project/build.rs").to_string()
}

fn generate_todos_vx() -> String {
    include_str!("../../templates/project/src/components/Todos.vx").to_string()
}

fn generate_todo_input_vx() -> String {
    include_str!("../../templates/project/src/components/TodoInput.vx").to_string()
}

fn generate_todo_item_vx() -> String {
    include_str!("../../templates/project/src/components/TodoItem.vx").to_string()
}

#[doc(hidden)]
pub fn generate_cargo_toml_for_test(name: &str, dir: &std::path::Path) -> String {
    generate_cargo_toml(name, dir)
}
