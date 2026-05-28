fn main() {
    // build_cmd in Render mode recursively compiles the input .vx file
    // and all imported components. It emits cargo:rerun-if-changed
    // directives for every .vx file it reads.
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/App.vx");

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());

    velox_cli::build_cmd(&input, Some(&out_dir), velox_cli::EmitMode::Render
    ).expect("Failed to compile App.vx");
}