use std::env;
use std::fs;
use std::path::Path;

fn main() {
    // Compile .vx files to Rust
    let out_dir = env::var("OUT_DIR").unwrap();
    let vx_files = ["src/App.vx"];
    
    for file in &vx_files {
        let path = Path::new(file);
        if path.exists() {
            let content = fs::read_to_string(path).unwrap();
            let generated = compile_vx(&content);
            let out_path = Path::new(&out_dir).join(
                path.file_stem().unwrap().to_str().unwrap().to_string() + ".rs"
            );
            fs::write(&out_path, generated).unwrap();
        }
    }
    
    // Rebuild if .vx files change
    for file in &vx_files {
        println!("cargo:rerun-if-changed={}", file);
    }
}

fn compile_vx(content: &str) -> String {
    // Simplified compilation - in real implementation would use velox-sfc
    format!("// Generated from .vx file\n{}", content)
}
