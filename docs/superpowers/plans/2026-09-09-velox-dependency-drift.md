# Velox Dependency-Drift Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `velox init` reproducible by pinning git deps to rev+version and supporting local path override.

**Architecture:** Embed git rev in `velox-cli` at compile time via new `velox-cli/build.rs`; `generate_cargo_toml()` prefers `VELOX_PATH`/`--local`, then workspace path+version, else git+rev+version; dedup dead template; keep app.rs alias with tests.

**Tech Stack:** Rust, Cargo, clap 4 derive, anyhow, velox-cli 0.1.0

**Spec:** `docs/superpowers/specs/2026-09-09-velox-dependency-drift-design.md`

## Global Constraints

- velox crates version is `0.1.0` (velox-cli/Cargo.toml:4).
- Generated projects must keep `[workspace]`, `velox-renderer` with `features = ["skia-native"]`, `serde_json = "1.0"`, build-dep `velox-cli`.
- `src/main.rs` includes `concat!(env!("OUT_DIR"), "/app.rs")` lowercase; codegen primary is `app.rs` + raw-case alias.
- Do not change `run_current` codegen ownership; do not refactor renderer/core/style.

---

### Task 1: Embed git rev in velox-cli

**Files:**
- Create: `velox-cli/build.rs`
- Modify: `velox-cli/Cargo.toml:1-11`
- Test: `velox-cli/tests/build_tests.rs:1-5`

**Interfaces:**
- Consumes: `git rev-parse --short HEAD` at build time (fallback `unknown`).
- Produces: `option_env!("VELOX_GIT_REV") -> Option<&str>`; `pub const VELOX_GIT_REV_FALLBACK: &str = "unknown"`; `pub fn velox_git_rev() -> &'static str` in `velox-cli/src/lib.rs`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn cli_exposes_git_rev() {
    let rev = velox_cli::velox_git_rev();
    assert!(!rev.is_empty(), "rev must not be empty");
    assert_ne!(rev, "MISSING", "build.rs must set VELOX_GIT_REV or fallback");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p velox-cli --test build_tests cli_exposes_git_rev -v`
Expected: FAIL with "no function `velox_git_rev` in `velox_cli`"

- [ ] **Step 3: Write minimal implementation**

Create `velox-cli/build.rs`:
```rust
fn main() {
    let rev = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=VELOX_GIT_REV={}", rev);
    println!("cargo:rerun-if-changed=../../.git/HEAD");
}
```

Append to `velox-cli/src/lib.rs`:
```rust
pub const VELOX_GIT_REV_FALLBACK: &str = "unknown";
pub fn velox_git_rev() -> &'static str {
    option_env!("VELOX_GIT_REV").unwrap_or(VELOX_GIT_REV_FALLBACK)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p velox-cli --test build_tests cli_exposes_git_rev -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add velox-cli/build.rs velox-cli/src/lib.rs velox-cli/tests/build_tests.rs
git commit -m "feat(cli): embed VELOX_GIT_REV at compile time"
```

### Task 2: Pin generate_cargo_toml with rev+version and VELOX_PATH override

**Files:**
- Modify: `velox-cli/src/commands/init.rs:231-289`
- Test: `velox-cli/tests/build_tests.rs`

**Interfaces:**
- Consumes: `crate::velox_git_rev()` from Task 1; `std::env::var("VELOX_PATH")`.
- Produces: `fn generate_cargo_toml(name: &str, project_dir: &Path) -> String` output always contains `version = "0.1.0"` on velox deps; git branch contains `rev="..."`; path branch contains `version = "0.1.0"`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn init_toml_pins_version_and_rev() {
    std::env::remove_var("VELOX_PATH");
    let dir = std::env::temp_dir().join(format!("velox-pin-{}", std::process::id()));
    let toml = velox_cli::commands::init::generate_cargo_toml_for_test("demo", &dir);
    assert!(toml.contains(r#"version = "0.1.0""#), "must bind version:\n{toml}");
}
```

Note: this requires making `generate_cargo_toml` `pub(crate)` for test access; that visibility change is part of Step 3.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p velox-cli --test build_tests init_toml_pins_version_and_rev -v`
Expected: FAIL with "no function `generate_cargo_toml_for_test`" (test helper does not exist yet).

- [ ] **Step 3: Write minimal implementation**

In `velox-cli/src/commands/init.rs`, change `fn generate_cargo_toml` to `pub(crate) fn generate_cargo_toml`, then add:
```rust
pub(crate) fn velox_dep_path(prefix: &str, workspace: &std::path::Path, project_dir: &std::path::Path, leaf: &str) -> String {
    let p = compute_relative_path(project_dir, &workspace.join(leaf));
    format!(r#"{{ path = "{}", version = "0.1.0" }}"#, p.display())
}
```

Replace path branch deps with `version = "0.1.0"` suffix, e.g.:
```rust
velox-core = { path = "{}", version = "0.1.0" }
```

Replace git fallback with:
```rust
let rev = crate::velox_git_rev();
format!(
    "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n\n[dependencies]\nvelox-core = {{ git = \"https://github.com/fahimaloy/velox\", rev = \"{rev}\", version = \"0.1.0\" }}\nvelox-dom = {{ git = \"https://github.com/fahimaloy/velox\", rev = \"{rev}\", version = \"0.1.0\" }}\nvelox-style = {{ git = \"https://github.com/fahimaloy/velox\", rev = \"{rev}\", version = \"0.1.0\" }}\nvelox-renderer = {{ git = \"https://github.com/fahimaloy/velox\", rev = \"{rev}\", version = \"0.1.0\", features = [\"skia-native\"] }}\nserde_json = \"1.0\"\n\n[build-dependencies]\nvelox-cli = {{ git = \"https://github.com/fahimaloy/velox\", rev = \"{rev}\", version = \"0.1.0\" }}\n"
)
```

Prepend override at top of `generate_cargo_toml`:
```rust
if let Ok(local) = std::env::var("VELOX_PATH") {
    let ws = std::path::PathBuf::from(local);
    if ws.join("velox-core").join("Cargo.toml").exists() {
        let core_path = compute_relative_path(project_dir, &ws.join("velox-core"));
        let dom_path = compute_relative_path(project_dir, &ws.join("velox-dom"));
        let style_path = compute_relative_path(project_dir, &ws.join("velox-style"));
        let renderer_path = compute_relative_path(project_dir, &ws.join("velox-renderer"));
        let cli_path = compute_relative_path(project_dir, &ws.join("velox-cli"));
        return format!(
            "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n\n[dependencies]\nvelox-core = {{ path = \"{}\", version = \"0.1.0\" }}\nvelox-dom = {{ path = \"{}\", version = \"0.1.0\" }}\nvelox-style = {{ path = \"{}\", version = \"0.1.0\" }}\nvelox-renderer = {{ path = \"{}\", version = \"0.1.0\", features = [\"skia-native\"] }}\nserde_json = \"1.0\"\n\n[build-dependencies]\nvelox-cli = {{ path = \"{}\", version = \"0.1.0\" }}\n",
            core_path.display(), dom_path.display(), style_path.display(), renderer_path.display(), cli_path.display()
        );
    }
}
```

Add test helper at bottom of `init.rs`:
```rust
#[doc(hidden)]
pub fn generate_cargo_toml_for_test(name: &str, dir: &std::path::Path) -> String {
    generate_cargo_toml(name, dir)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p velox-cli --test build_tests init_toml_pins_version_and_rev -v`
Expected: PASS. Also run: `cargo test -p velox-cli -v` full suite passes.

- [ ] **Step 5: Commit**

```bash
git add velox-cli/src/commands/init.rs velox-cli/tests/build_tests.rs
git commit -m "feat(cli): pin init deps to rev+version with VELOX_PATH override"
```

### Task 3: Add --local flag to velox init

**Files:**
- Modify: `velox-cli/src/bin/main.rs:18-25`
- Modify: `velox-cli/src/commands/init.rs:78-106`
- Test: `velox-cli/tests/build_tests.rs`

**Interfaces:**
- Consumes: `VELOX_PATH` logic from Task 2; `init_project_with_template(name, template)` existing signature.
- Produces: `init_project_with_template_local(name, template, local: Option<&Path>)`; `velox init <name> --local <path>` CLI flag.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn init_local_override_uses_given_path() {
    let ws = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let dir = std::env::temp_dir().join(format!("velox-local-{}", std::process::id()));
    let toml = velox_cli::commands::init::init_toml_with_local("demo", &dir, Some(ws.as_path()));
    assert!(toml.contains(r#"version = "0.1.0""#), "local path must also pin version:\n{toml}");
    assert!(toml.contains("path = "), "local override must use path deps:\n{toml}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p velox-cli --test build_tests init_local_override_uses_given_path -v`
Expected: FAIL with "no function `init_toml_with_local`"

- [ ] **Step 3: Write minimal implementation**

In `velox-cli/src/bin/main.rs`, extend `Init`:
```rust
Init {
    name: String,
    #[arg(long, short = 't', default_value = "default")]
    template: String,
    #[arg(long)]
    local: Option<std::path::PathBuf>,
},
```

Update match arm:
```rust
Commands::Init { name, template, local } => {
    let path = velox_cli::commands::init_project_with_template_local(&name, &template, local.as_deref())?;
    println!("✅ Created Velox project at: {}", path.display());
}
```

In `init.rs`, add:
```rust
pub fn init_project_with_template_local(name: &str, template: &str, local: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = local {
        std::env::set_var("VELOX_PATH", p);
    }
    let out = init_project_with_template(name, template);
    if local.is_some() {
        std::env::remove_var("VELOX_PATH");
    }
    out
}

#[doc(hidden)]
pub fn init_toml_with_local(name: &str, dir: &Path, local: Option<&Path>) -> String {
    if let Some(p) = local {
        std::env::set_var("VELOX_PATH", p);
    }
    let s = generate_cargo_toml(name, dir);
    std::env::remove_var("VELOX_PATH");
    s
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p velox-cli --test build_tests init_local_override_uses_given_path -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add velox-cli/src/bin/main.rs velox-cli/src/commands/init.rs velox-cli/tests/build_tests.rs
git commit -m "feat(cli): velox init --local path override"
```

### Task 4: Harden workspace detection and template dedup

**Files:**
- Modify: `velox-cli/src/commands/init.rs:7-64`
- Delete: `velox-cli/templates/project/Cargo.toml.template`
- Test: `velox-cli/tests/build_tests.rs`

**Interfaces:**
- Consumes: `find_velox_workspace()`, `compute_relative_path()` existing fns.
- Produces: workspace detection verifies `velox-core/Cargo.toml` exists; `compute_relative_path` falls back to absolute path when canonicalize fails; no `.template` file on disk.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn workspace_detect_requires_marker_file() {
    let found = velox_cli::commands::init::find_velox_workspace_for_test();
    if let Some(ws) = found {
        assert!(ws.join("velox-core").join("Cargo.toml").exists(), "workspace must contain marker");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p velox-cli --test build_tests workspace_detect_requires_marker_file -v`
Expected: FAIL with "no function `find_velox_workspace_for_test`"

- [ ] **Step 3: Write minimal implementation**

In `init.rs`, make `find_velox_workspace` `pub(crate)` (already verifies marker at lines 11-13, keep as is) and add:
```rust
#[doc(hidden)]
pub fn find_velox_workspace_for_test() -> Option<std::path::PathBuf> {
    find_velox_workspace()
}
```

Harden `compute_relative_path` fallback: after canonicalize, if either side does not exist, return absolute `to` path:
```rust
if !from.exists() || !to.exists() {
    return to.to_path_buf();
}
```
Insert at top of `compute_relative_path` after canonicalize lines.

Delete dead template:
```bash
git rm velox-cli/templates/project/Cargo.toml.template
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p velox-cli --test build_tests workspace_detect_requires_marker_file -v`
Expected: PASS. Also verify: `ls velox-cli/templates/project/Cargo.toml.template` fails (deleted).

- [ ] **Step 5: Commit**

```bash
git add velox-cli/src/commands/init.rs velox-cli/tests/build_tests.rs
git commit -m "fix(cli): harden workspace detect, drop dead Cargo template"
```

### Task 5: Lock case-safety with alias tests

**Files:**
- Modify: `velox-cli/tests/build_tests.rs:4-22`
- Test: same file

**Interfaces:**
- Consumes: `velox_cli::build_cmd` Render + Stub from `velox-cli/src/lib.rs:62-102`; `sanitize_mod_name` in `velox-cli/src/commands/build.rs:34`.
- Produces: both `app.rs` primary and `App.rs` alias exist with `pub mod app`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn cli_stub_emits_lowercase_and_alias() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let input = std::path::PathBuf::from(manifest_dir).join("templates/project/src/App.vx");
    let out_dir = std::path::PathBuf::from(manifest_dir)
        .join("../target/velox-cli-tests")
        .join(format!("{}-stub-alias", std::process::id()));
    velox_cli::build_cmd(&input, Some(out_dir.as_path()), velox_cli::EmitMode::Stub).expect("stub");
    assert!(out_dir.join("app.rs").exists(), "stub primary app.rs must exist");
    assert!(out_dir.join("App.rs").exists(), "stub alias App.rs must exist");
    let content = std::fs::read_to_string(out_dir.join("app.rs")).expect("read");
    assert!(content.contains("pub mod app"), "module must be lowercase app");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p velox-cli --test build_tests cli_stub_emits_lowercase_and_alias -v`
Expected: FAIL with "no such test" before adding (add test first, run, then if implementation regressed it fails on missing alias).

- [ ] **Step 3: Write minimal implementation**

No source change needed (alias already in `lib.rs:84-97`, `build.rs:294-302`); update old assertion in `cli_build_emits_stub_file` from `out_dir.join("App.rs")` to `out_dir.join("app.rs")`:
```rust
let out_file = out_dir.join("app.rs");
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p velox-cli --test build_tests -v`
Expected: PASS (all 5+ tests including existing render test asserting `app.rs`).

- [ ] **Step 5: Commit**

```bash
git add velox-cli/tests/build_tests.rs
git commit -m "test(cli): lock app.rs lowercase primary plus App.rs alias"
```

### Task 6: Push drift and re-verify clean init

**Files:**
- Modify: none (release step, uses `velox-cli/README` if troubleshooting note needed)
- Test: manual `cargo build` + `timeout 20 velox run` in fresh /tmp project

**Interfaces:**
- Consumes: Tasks 1-5 merged on `alpha`; remote `origin` = `git@github.com:fahimaloy/velox.git`.
- Produces: `origin/alpha` == local `alpha`; fresh `/tmp/test_app2` builds with pinned rev; `OUT_DIR/app.rs` exists.

- [ ] **Step 1: Write the failing check (script)**

```bash
test "$(git rev-parse alpha)" = "$(git rev-parse origin/alpha)" && echo PINNED || echo DRIFT
```

- [ ] **Step 2: Run check to verify it fails**

Run: `git rev-parse alpha; git rev-parse origin/alpha; git status --short | head`
Expected: DRIFT (40-file diff, plus new spec/plan commits ahead).

- [ ] **Step 3: Push minimal implementation**

```bash
git push origin alpha
cargo install --path velox-cli
rm -rf /tmp/test_app2 && velox init test_app2 --local /home/fahimaloy/Projects/personal/velox
cat /tmp/test_app2/Cargo.toml
```

- [ ] **Step 4: Run verification**

Run: `cargo build` in `/tmp/test_app2`
Expected: PASS, `ls target/debug/build/test_app2-*/out/app.rs` exists.
Run: `timeout 20 velox run`
Expected: `Starting test_app2...` with no `couldn't read .../app.rs`.

- [ ] **Step 5: Commit (docs only if needed)**

```bash
git add -A && git status --short
git commit -m "chore: verify pinned init from alpha" || echo "nothing to commit"
```
