use std::fs;
use std::path::PathBuf;

#[test]
fn cli_build_emits_stub_file() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let input = PathBuf::from(manifest_dir).join("../examples/todo/src/App.vx");

    let out_dir = PathBuf::from(manifest_dir)
        .join("../target/velox-cli-tests")
        .join(format!("{}-stub", std::process::id()));

    velox_cli::build_cmd(&input, Some(out_dir.as_path()), velox_cli::EmitMode::Stub)
        .expect("build stub");

    let out_file = out_dir.join("App.rs");
    let content = fs::read_to_string(&out_file).expect("read stub output");
    assert!(content.contains("pub const TEMPLATE"), "stub should contain TEMPLATE const");
}

#[test]
fn cli_build_emits_render_fn() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let input = PathBuf::from(manifest_dir).join("../examples/todo/src/App.vx");

    let out_dir = PathBuf::from(manifest_dir)
        .join("../target/velox-cli-tests")
        .join(format!("{}-render", std::process::id()));

    velox_cli::build_cmd(&input, Some(out_dir.as_path()), velox_cli::EmitMode::Render)
        .expect("build render");

    let out_file = out_dir.join("App.rs");
    let content = fs::read_to_string(&out_file).expect("read render output");
    assert!(content.contains("pub fn render()"), "render mode should include render() fn");
}

