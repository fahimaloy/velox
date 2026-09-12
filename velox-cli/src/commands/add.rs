use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

/// Walk up from the current directory looking for a Velox project root, defined
/// as the first ancestor directory containing a `src/App.vx` entry point (the
/// conventional project layout produced by `velox init`).
fn find_project_root() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let mut current = cwd.as_path();
    loop {
        if current.join("src").join("App.vx").exists() {
            return Some(current.to_path_buf());
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => return None,
        }
    }
}

/// Split a name into lowercase word tokens, handling separators (`_`, `-`,
/// spaces), digits, and camelCase boundaries. e.g. `"My Counter2"` ->
/// `["my", "counter", "2"]`.
fn split_words(name: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut prev_lower = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            // Start a new word on uppercase following lowercase (camelCase).
            if ch.is_uppercase() && prev_lower && !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            current.push(ch.to_ascii_lowercase());
            prev_lower = ch.is_lowercase();
        } else if !current.is_empty() {
            words.push(std::mem::take(&mut current));
            prev_lower = false;
        } else {
            prev_lower = false;
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// Convert a user-supplied component name into a Rust-safe PascalCase struct
/// name (e.g. `"my-counter"` -> `"MyCounter"`). Falls back to `"Component"` if
/// the input contains no word characters.
fn to_pascal_case(name: &str) -> String {
    let mut out = String::new();
    for word in split_words(name) {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    if out.is_empty() {
        out = "Component".to_string();
    }
    out
}

/// Convert a PascalCase name into a kebab-case CSS class identifier
/// (e.g. `"MyCounter"` -> `"my-counter"`).
fn to_kebab_case(name: &str) -> String {
    let words = split_words(name);
    if words.is_empty() {
        return "component".to_string();
    }
    words.join("-")
}

/// Scaffold a new `.vx` component for the current project.
///
/// Returns the path of the created file on success.
pub fn add_component(name: &str) -> Result<PathBuf> {
    let root = find_project_root().ok_or_else(|| {
        anyhow::anyhow!(
            "not inside a Velox project (no src/App.vx found). Run `velox init <name>` first."
        )
    })?;

    let struct_name = to_pascal_case(name);
    let kebab = to_kebab_case(&struct_name);

    // Reject names whose PascalCase form is not a valid Rust identifier start
    // (e.g. `add component 1foo`).
    let first = struct_name.chars().next().unwrap_or('_');
    if !first.is_ascii_alphabetic() && first != '_' {
        anyhow::bail!("invalid component name '{name}': must start with a letter");
    }

    let components_dir = root.join("src").join("components");
    fs::create_dir_all(&components_dir)
        .with_context(|| format!("create {}", components_dir.display()))?;

    // Match the project convention of PascalCase `.vx` filenames
    // (e.g. `TodoItem.vx`).
    let file_name = format!("{}.vx", struct_name);
    let path = components_dir.join(&file_name);

    if path.exists() {
        anyhow::bail!("component already exists: {}", path.display());
    }

    let content = component_template(&struct_name, &kebab);
    fs::write(&path, content).with_context(|| format!("write {}", path.display()))?;

    println!("✅ Created component: {}", path.display());
    println!("   Import it from a parent component, e.g.:");
    println!("   import {struct_name} from './components/{file_name}'");

    Ok(path)
}

/// Render the default scaffold for a newly created component. The generated
/// component is self-contained (no imports) and is known to compile with the
/// same `main.rs` / codegen contract as the project templates.
fn component_template(struct_name: &str, kebab: &str) -> String {
    let body = r#"<template>
  <div class="__KEBAB__">
    <h3>{{ title }}</h3>
  </div>
</template>

<script setup>
pub struct State {
    pub title: String,
}

impl State {
    pub fn new() -> Self {
        Self {
            title: String::from("__NAME__"),
        }
    }

    pub fn title(&self) -> String {
        self.title.clone()
    }
}
</script>

<style scoped>
.__KEBAB__ {
    padding: 16px;
    border-radius: 10px;
    background: #16213e;
    color: #e6edf3;
}
</style>
"#;
    body.replace("__KEBAB__", kebab).replace("__NAME__", struct_name)
}