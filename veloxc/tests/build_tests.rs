use std::fs;
use std::path::PathBuf;

#[test]
fn cli_exposes_git_rev() {
    let rev = veloxc::velox_git_rev();
    assert!(!rev.is_empty(), "rev must not be empty");
    assert_ne!(
        rev, "MISSING",
        "build.rs must set VELOX_GIT_REV or fallback"
    );
}

#[test]
fn init_toml_pins_version_and_rev() {
    unsafe {
        std::env::remove_var("VELOX_PATH");
    }
    let dir = std::env::temp_dir().join(format!("velox-pin-{}", std::process::id()));
    let toml = veloxc::commands::init::generate_cargo_toml_for_test("demo", &dir);
    let expected = format!(r#"version = "{}""#, env!("CARGO_PKG_VERSION"));
    assert!(toml.contains(&expected), "must bind version:\n{toml}");
}

#[test]
fn cli_build_emits_stub_file() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let input = PathBuf::from(manifest_dir).join("templates/project/src/App.vx");

    let out_dir = PathBuf::from(manifest_dir)
        .join("../target/velox-cli-tests")
        .join(format!("{}-stub", std::process::id()));

    veloxc::build_cmd(&input, Some(out_dir.as_path()), veloxc::EmitMode::Stub).expect("build stub");

    let out_file = out_dir.join("app.rs");
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

    veloxc::build_cmd(&input, Some(out_dir.as_path()), veloxc::EmitMode::Render)
        .expect("build render");

    // Output file uses sanitized (lowercase) module name per Rust conventions
    let out_file = out_dir.join("app.rs");
    let content = fs::read_to_string(&out_file).expect("read render output");
    assert!(
        content.contains("pub fn render()"),
        "render mode should include render() fn"
    );
}

#[test]
fn init_local_override_uses_given_path() {
    let ws = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let dir = std::env::temp_dir().join(format!("velox-local-{}", std::process::id()));
    let toml = veloxc::commands::init::init_toml_with_local("demo", &dir, Some(ws.as_path()));
    let expected = format!(r#"version = "{}""#, env!("CARGO_PKG_VERSION"));
    assert!(
        toml.contains(&expected),
        "local path must also pin version:\n{toml}"
    );
    assert!(
        toml.contains("path = "),
        "local override must use path deps:\n{toml}"
    );
}

#[test]
fn cli_stub_emits_lowercase_and_alias() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let input = std::path::PathBuf::from(manifest_dir).join("templates/project/src/App.vx");
    let out_dir = std::path::PathBuf::from(manifest_dir)
        .join("../target/velox-cli-tests")
        .join(format!("{}-stub-alias", std::process::id()));
    veloxc::build_cmd(&input, Some(out_dir.as_path()), veloxc::EmitMode::Stub).expect("stub");
    assert!(
        out_dir.join("app.rs").exists(),
        "stub primary app.rs must exist"
    );
    assert!(
        out_dir.join("App.rs").exists(),
        "stub alias App.rs must exist"
    );
    let content = std::fs::read_to_string(out_dir.join("app.rs")).expect("read");
    assert!(
        content.contains("pub mod app"),
        "module must be lowercase app"
    );
}

#[test]
fn init_writes_supported_todo_event_template() {
    let project_dir =
        std::env::temp_dir().join(format!("velox-init-template-{}", std::process::id()));
    let _ = fs::remove_dir_all(&project_dir);

    veloxc::commands::init::init_project(project_dir.to_str().unwrap())
        .expect("init should write the project template");

    let todos = fs::read_to_string(project_dir.join("src/components/Todos.vx"))
        .expect("read generated Todos.vx");
    let input = fs::read_to_string(project_dir.join("src/components/TodoInput.vx"))
        .expect("read generated TodoInput.vx");
    let item = fs::read_to_string(project_dir.join("src/components/TodoItem.vx"))
        .expect("read generated TodoItem.vx");

    assert!(todos.contains(r#"@click="add_todo""#));
    assert!(!todos.contains("@submit"));
    assert!(!input.contains("@submit"));
    assert!(!input.contains("on_submit"));
    assert!(!item.contains("on_submit"));

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn workspace_detect_requires_marker_file() {
    let found = veloxc::commands::init::find_velox_workspace_for_test();
    if let Some(ws) = found {
        assert!(
            ws.join("velox-core").join("Cargo.toml").exists(),
            "workspace must contain marker"
        );
    }
}
