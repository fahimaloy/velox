use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let vx_path = manifest_dir.join("src").join("App.vx");

    if vx_path.exists() {
        velox_cli::build_cmd(&vx_path, Some(&out_dir), velox_cli::EmitMode::Render)
            .expect("failed to compile .vx file");
    }
}
