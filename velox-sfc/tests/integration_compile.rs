use std::fs;
use std::process::Command;
use std::path::PathBuf;

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
    let tpl_src = sfc.template.as_ref().map(|t| t.content.as_str()).unwrap_or("");
    let render_fn = velox_sfc::compile_template_to_rs(tpl_src, name).expect("compile tpl");
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

    let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let tmp = std::env::temp_dir().join(format!("velox_integration_{}", unique));
    let proj = tmp.join("app_crate");
    let src = proj.join("src");
    fs::create_dir_all(&src).expect("create tmp project");

    // Write Cargo.toml pointing to workspace crates by absolute path
    let cargo_toml = format!(r#"[package]
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
        crate_base.join("velox-renderer").display());
    fs::write(proj.join("Cargo.toml"), cargo_toml).expect("write Cargo.toml");

    // Write generated module and main
    fs::write(src.join("App.rs"), module_code).expect("write App.rs");
    let main = r#"include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/App.rs"));
fn main() { let _ = app::render(); }
"#;
    fs::write(src.join("main.rs"), main).expect("write main.rs");

    // Run cargo build in the temp project
    let out = Command::new("cargo").arg("build").current_dir(&proj).output().expect("cargo build failed to spawn");
    if !out.status.success() {
        panic!("cargo build failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    }
}
