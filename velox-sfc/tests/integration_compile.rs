use std::fs;
use std::path::PathBuf;
use std::process::Command;

// This integration test writes generated SFC Rust to a temporary Cargo project and
// invokes `cargo build` to ensure the generated code compiles against workspace crates.
// It is ignored by default because it runs an external `cargo` build and is slow.

#[test]
#[ignore]
fn compile_generated_app_crate() {
    // Sample SFC with template and script setup
    let sfc_src = r#"<template>
  <div>
    <button @click="inc">Inc</button>
    <div class="count">{{ count }}</div>
  </div>
</template>
<script setup>
use std::cell::{Cell};
pub struct State { pub count: Cell<i32> }
impl State { pub fn new() -> Self { Self { count: Cell::new(0) } } pub fn inc(&self) { let v = self.count.get()+1; self.count.set(v); } }
</script>
"#;

    // Parse SFC and produce module code (stub + render functions)
    let sfc = velox_sfc::parse_sfc(sfc_src).expect("parse sfc");
    let name = "app";
    let mut module_code = velox_sfc::to_stub_rs(&sfc, name);
    let tpl_src = sfc
        .template
        .as_ref()
        .map(|t| t.content.as_str())
        .unwrap_or("");
    let render_fn = velox_sfc::compile_template_to_rs(tpl_src, name, None).expect("compile tpl");
    if let Some(pos) = module_code.rfind('}') {
        module_code.insert_str(pos, &format!("\n{}\n", render_fn));
    } else {
        module_code.push_str("\n");
        module_code.push_str(&render_fn);
    }

    // Create a temporary project directory
    let crate_base: PathBuf = {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        // workspace root is parent of velox-sfc
        manifest_dir.parent().unwrap().to_path_buf()
    };

    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp = std::env::temp_dir().join(format!("velox_integration_{}", unique));
    let proj = tmp.join("app_crate");
    let src = proj.join("src");
    fs::create_dir_all(&src).expect("create tmp project");

    // Write Cargo.toml pointing to workspace crates by absolute path
    let cargo_toml = format!(
        r#"[package]
name = "velox_integration_test"
version = "0.1.0"
edition = "2021"

[dependencies]
velox-core = {{ path = "{}" }}
velox-dom = {{ path = "{}" }}
velox-style = {{ path = "{}" }}
velox-renderer = {{ path = "{}" }}
"#,
        crate_base.join("velox-core").display(),
        crate_base.join("velox-dom").display(),
        crate_base.join("velox-style").display(),
        crate_base.join("velox-renderer").display()
    );
    fs::write(proj.join("Cargo.toml"), cargo_toml).expect("write Cargo.toml");

    // Write generated module and main
    fs::write(src.join("App.rs"), module_code).expect("write App.rs");
    let main = r#"include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/App.rs"));
fn main() { let _ = app::render(); }
"#;
    fs::write(src.join("main.rs"), main).expect("write main.rs");

    // Run cargo build in the temp project
    let out = Command::new("cargo")
        .arg("build")
        .current_dir(&proj)
        .output()
        .expect("cargo build failed to spawn");
    if !out.status.success() {
        panic!(
            "cargo build failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
#[ignore]
fn compile_component_import_test() {
    // Create temporary directory for component test
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp = std::env::temp_dir().join(format!("velox_component_test_{}", unique));
    fs::create_dir_all(&tmp).expect("create temp dir");

    // Create Button.vx component
    let button_src = r#"<template>
  <button :class="buttonClass" @click="onClick">
    {{ label }}
  </button>
</template>
<script setup>
pub struct State { pub label: String }
impl State { pub fn new(label: String) -> Self { Self { label } } pub fn onClick(&self) { println!("Button clicked!"); } }
</script>
<style>
button { padding: 8px 16px; background: blue; color: white; border: none; }
</style>"#;
    fs::write(tmp.join("Button.vx"), button_src).expect("write Button.vx");

    // Create App.vx that imports Button
    let app_src = r#"<template>
  <div class="app">
    <h1>My App</h1>
    <Button label="Click me!" />
  </div>
</template>
<script setup>
import Button from './Button.vx';
pub struct State { pub count: u32 }
impl State { pub fn new() -> Self { Self { count: 0 } } }
</script>
<style>
.app { padding: 20px; }
</style>"#;
    fs::write(tmp.join("App.vx"), app_src).expect("write App.vx");

    // Parse and compile App.vx with component resolver
    let sfc = velox_sfc::parse_sfc(app_src).expect("parse App SFC");
    let name = "app";

    // Create resolver with correct base path
    let mut resolver = velox_sfc::ComponentResolver::new(tmp.clone());
    if let Some(script_setup) = &sfc.script_setup {
        resolver.parse_imports(&script_setup.content);
    }

    // Compile template with component support
    let tpl_src = sfc
        .template
        .as_ref()
        .map(|t| t.content.as_str())
        .unwrap_or("");
    let render_fn = velox_sfc::compile_template_to_rs(tpl_src, name, Some(&resolver))
        .expect("compile template");

    // Generate stub with imports
    let mut stub = velox_sfc::to_stub_rs(&sfc, name);

    // Add component module import
    if !resolver.component_names().is_empty() {
        for comp_name in resolver.component_names() {
            // Load and compile the component
            let _ = resolver.load_component(&comp_name); // This will compile Button.vx

            // Add mod declaration to stub
            let mod_line = format!("    pub mod {};\n", comp_name);
            if let Some(pos) = stub.find("pub const STYLE") {
                stub.insert_str(pos, &mod_line);
            }
        }
    }

    // Combine stub and render function
    let mut module_code = String::new();
    if let Some(pos) = stub.rfind('}') {
        module_code.push_str(&stub[..pos]);
        module_code.push_str("\n");
        module_code.push_str(&render_fn);
        module_code.push_str("\n");
        module_code.push_str(&stub[pos..]);
    } else {
        module_code.push_str(&stub);
        module_code.push_str("\n");
        module_code.push_str(&render_fn);
    }

    // Create temp cargo project for testing
    let proj = tmp.join("test_project");
    let src = proj.join("src");
    fs::create_dir_all(&src).expect("create test project");

    // Create Cargo.toml
    let crate_base: PathBuf = {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest_dir.parent().unwrap().to_path_buf()
    };

    let cargo_toml = format!(
        r#"[package]
name = "component_test"
version = "0.1.0"
edition = "2021"

[dependencies]
velox-core = {{ path = "{}" }}
velox-dom = {{ path = "{}" }}
velox-style = {{ path = "{}" }}
"#,
        crate_base.join("velox-core").display(),
        crate_base.join("velox-dom").display(),
        crate_base.join("velox-style").display()
    );

    fs::write(proj.join("Cargo.toml"), cargo_toml).expect("write Cargo.toml");

    // Write generated code
    fs::write(src.join("app.rs"), module_code).expect("write app.rs");
    let main_rs = r#"mod app;
use velox_dom::VNode;

fn main() {
    let vnode = app::render();
    println!("Component compiled successfully!");
}
"#;
    fs::write(src.join("main.rs"), main_rs).expect("write main.rs");

    // Run cargo build
    let out = Command::new("cargo")
        .arg("build")
        .current_dir(&proj)
        .output()
        .expect("cargo build failed");
    if !out.status.success() {
        panic!(
            "Component build failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    println!("✅ Component integration test passed!");
}
