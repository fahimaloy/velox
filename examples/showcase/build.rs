//! Compiles the example's `.vx` file into generated Rust in OUT_DIR.
//!
//! This runs before the example crate itself is compiled, so the generated
//! `app.rs` is always in sync with `src/App.vx`.

use std::env;
use std::path::PathBuf;

fn main() {
    // Re-run if the .vx file changes.
    println!("cargo:rerun-if-changed=src/App.vx");

    let vx_path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("src/App.vx");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    veloxc::build_cmd(&vx_path, Some(&out_dir), veloxc::EmitMode::Render)
        .expect("failed to compile .vx file");
}
