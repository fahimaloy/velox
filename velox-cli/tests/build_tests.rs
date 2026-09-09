use std::fs;
use std::path::PathBuf;

#[test]
fn cli_exposes_git_rev() {
    let rev = velox_cli::velox_git_rev();
    assert!(!rev.is_empty(), "rev must not be empty");
    assert_ne!(rev, "MISSING", "build.rs must set VELOX_GIT_REV or fallback");
}

#[test]
fn init_toml_pins_version_and_rev() {
    unsafe { std::env::remove_var("VELOX_PATH"); }
    let dir = std::env::temp_dir().join(format!("velox-pin-{}", std::process::id()));
    let toml = velox_cli::commands::init::generate_cargo_toml_for_test("demo", &dir);
    assert!(toml.contains(r#"version = "0.1.0""#), "must bind version:\n{toml}");
}

#[test]
fn cli_build_emits_stub_file() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let input = PathBuf::from(manifest_dir).join("templates/project/src/App.vx");

    let out_dir = PathBuf::from(manifest_dir)
        .join("../target/velox-cli-tests")
        .join(format!("{}-stub", std::process::id()));

    velox_cli::build_cmd(&input, Some(out_dir.as_path()), velox_cli::EmitMode::Stub)
        .expect("build stub");

    let out_file = out_dir.join("App.rs");
    let content = fs::read_to_string(&out_file).expect("read stub output");
    assert!(
        content.contains("pub const TEMPLATE"),
        "stub should contain TEMPLATE const"
    );
}

#[test]
fn cli_build_emits_render_fn() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let input = PathBuf::from(manifest_dir).join("templates/project/src/App.vx");

    let out_dir = PathBuf::from(manifest_dir)
        .join("../target/velox-cli-tests")
        .join(format!("{}-render", std::process::id()));

    velox_cli::build_cmd(&input, Some(out_dir.as_path()), velox_cli::EmitMode::Render)
        .expect("build render");

    // Output file uses sanitized (lowercase) module name per Rust conventions
    let out_file = out_dir.join("app.rs");
    let content = fs::read_to_string(&out_file).expect("read render output");
    assert!(
        content.contains("pub fn render()"),
        "render mode should include render() fn"
    );
}
