fn main() {
    println!("cargo:rerun-if-changed=src/App.vx");
    
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/App.vx");
    
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    
    // Compile App.vx to Rust
    velox_cli::build_cmd(&input, Some(&out_dir), velox_cli::EmitMode::Render
    ).expect("Failed to compile App.vx");
}
