fn main() {
    println!("cargo:rerun-if-changed=src/App.vx");
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/App.vx");
    velox_cli::build_cmd(&input, Some(&std::path::Path::new(&std::env::var("OUT_DIR").unwrap())), velox_cli::EmitMode::Render).expect("compile App.vx");
}
