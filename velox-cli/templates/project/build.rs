use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = env::var("OUT_DIR").unwrap();
    let src_dir = PathBuf::from(&manifest_dir).join("src");

    // Collect all .vx files recursively
    let vx_files = collect_vx_files(&src_dir);

    for vx_path in &vx_files {
        compile_vx_file(vx_path, &src_dir, &out_dir);
    }

    // Rebuild if any .vx file changes
    for vx_path in &vx_files {
        println!("cargo:rerun-if-changed={}", vx_path.display());
    }
}

/// Recursively find all .vx files in a directory
fn collect_vx_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(collect_vx_files(&path));
            } else if path.extension().map_or(false, |ext| ext == "vx") {
                files.push(path);
            }
        }
    }
    files
}

/// Compile a single .vx file to Rust using velox-sfc
fn compile_vx_file(vx_path: &Path, src_dir: &Path, out_dir: &str) {
    let content = match fs::read_to_string(vx_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Warning: Could not read {:?}: {}", vx_path, e);
            return;
        }
    };

    let sfc = match velox_sfc::parse_sfc(&content) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Warning: Could not parse {:?}: {}", vx_path, e);
            return;
        }
    };

    // Determine component name from file stem
    let component_name = vx_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("component")
        .to_string();

    // Generate stub Rust code
    let generated = velox_sfc::to_stub_rs(&sfc, &component_name);

    // Compute relative path from src/ to maintain directory structure
    let relative = vx_path.strip_prefix(src_dir).unwrap_or(vx_path);

    // Create output directory structure
    if let Some(parent) = relative.parent() {
        let out_subdir = PathBuf::from(out_dir).join(parent);
        fs::create_dir_all(&out_subdir).ok();
    }

    // Write generated Rust file
    let out_path = PathBuf::from(out_dir).join(relative.with_extension("rs"));

    if let Err(e) = fs::write(&out_path, &generated) {
        eprintln!("Warning: Could not write {:?}: {}", out_path, e);
    }
}
