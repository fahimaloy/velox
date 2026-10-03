//! What `velox init` actually writes must agree with what it tells the app to
//! import.
//!
//! This file exists because that agreement was never checked. `init` ships a
//! template `App.vx` that renders `<Confirm>` and `<Modal>`, but wrote only three
//! of the five component files the template carries, so every scaffolded app
//! failed to compile with E0433 "could not find `confirm`/`modal` in `super`"
//! before it ever reached a window. No test ran `init` and looked at the result,
//! which is why it shipped.
//!
//! The assertions below are therefore structural rather than behavioural: they
//! check the *set* of components `init` writes against the set its own `App.vx`
//! imports. That is the invariant that broke, and it is the cheapest thing to
//! check — no compile, no render.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A throwaway project created by really running `velox init`.
struct ScratchProject {
    root: PathBuf,
}

impl ScratchProject {
    fn new(tag: &str) -> Self {
        Self::with_cwd(tag, None)
    }

    /// `cwd`, when given, is the working directory `velox init` runs *from* —
    /// which is a different thing from where it writes. `init` locates the
    /// velox workspace by walking up from the CWD, so the CWD decides which
    /// dependency form the generated `Cargo.toml` gets. Tests that care about
    /// that must set it deliberately.
    fn with_cwd(tag: &str, cwd: Option<&Path>) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("veloxc has a parent")
            .join("target")
            .join(format!("velox-init-scratch-{}-{tag}", std::process::id()));
        // A run that panicked mid-way leaves its directory behind; the name is
        // pid-scoped so that is only ever our own leftovers.
        let _ = fs::remove_dir_all(&root);

        let mut cmd = Command::new(env!("CARGO_BIN_EXE_veloxc"));
        cmd.arg("init").arg(&root);
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        let out = cmd.output().expect("run velox init");
        assert!(
            out.status.success(),
            "velox init failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );

        Self { root }
    }

    fn app_vx(&self) -> String {
        fs::read_to_string(self.root.join("src").join("App.vx")).expect("init wrote an App.vx")
    }

    fn cargo_toml(&self) -> String {
        fs::read_to_string(self.root.join("Cargo.toml")).expect("init wrote a Cargo.toml")
    }
}

impl Drop for ScratchProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Every component path `App.vx` imports, as written on disk relative to `src`.
///
/// Deliberately a text scan rather than a parse: the failure mode being guarded
/// is a file that is referenced but not shipped, and the reference is visible in
/// the source either way. A parser would add a dependency and a second way to be
/// wrong.
fn imported_component_paths(app_vx: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for line in app_vx.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("import ") else {
            continue;
        };
        let Some(from) = rest.split(" from ").nth(1) else {
            continue;
        };
        let from = from
            .trim()
            .trim_end_matches(';')
            .trim_matches(|c| c == '\'' || c == '"');
        if let Some(rel) = from.strip_prefix("./") {
            paths.push(rel.to_string());
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

#[test]
fn every_component_app_vx_imports_is_actually_written() {
    let p = ScratchProject::new("imports");
    let app = p.app_vx();

    let imports = imported_component_paths(&app);
    assert!(
        !imports.is_empty(),
        "no component imports found in the generated App.vx:\n{app}\n\
         If the template's import syntax changed, this test is now vacuous — fix the \
         extractor rather than deleting the assertion."
    );

    let mut missing = Vec::new();
    for rel in &imports {
        let on_disk = p.root.join("src").join(rel);
        if !on_disk.is_file() {
            missing.push(format!("{rel} (App.vx imports it; init did not write it)"));
        }
    }

    assert!(
        missing.is_empty(),
        "velox init produced an app that cannot compile: {} component file(s) are \
         imported by the generated App.vx but absent from the scaffold.\n\
         {}\n\
         Either init must write them, or App.vx must not import them.",
        missing.len(),
        missing.join("\n  ")
    );
}

#[test]
fn init_writes_confirm_and_modal_because_app_vx_renders_them() {
    // The concrete instance, pinned separately so that deleting the import from
    // App.vx (a legitimate alternative fix) does not silently pass this file by
    // making the test above check nothing.
    let p = ScratchProject::new("premium");
    let app = p.app_vx();

    for (component, tag) in [("Confirm", "confirm"), ("Modal", "modal")] {
        assert!(
            app.contains(&format!("<{component}")),
            "App.vx no longer renders <{component}>, so this test's premise is gone; \
             re-check whether init should still scaffold it."
        );
        assert!(
            p.root
                .join("src")
                .join("components")
                .join(format!("{component}.vx"))
                .is_file(),
            "App.vx renders <{component}> but init did not scaffold {component}.vx, so a \
             fresh `velox init` app fails to compile with E0433 \"could not find `{tag}` \
             in `super`\"."
        );
    }
}

#[test]
fn a_scaffold_from_outside_the_checkout_uses_path_dependencies() {
    // The defect this guards: `velox init myapp` is normally run from a scratch
    // directory that is NOT inside a velox checkout. `init` located the workspace
    // by walking up from the CWD, so that walk failed, it fell back to git
    // dependencies pinned at `velox_git_rev()` — the commit this CLI was BUILT
    // at — and on any locally-built binary that commit is on an unpushed branch.
    // The scaffold could not resolve its own dependencies:
    //
    //     error: failed to get `velox-core` as a dependency of package `myapp`
    //
    // It is the first thing a new user hits after `velox init`.
    //
    // This test runs `velox init` with its CWD deliberately set to a directory
    // with no velox checkout above it. That is the whole point: cargo runs tests
    // with the CWD inside the workspace, where the old CWD walk *succeeded*, so
    // a test that does not move the CWD passes against the unfixed code.
    let outside = std::env::temp_dir().join(format!("velox-outside-{}", std::process::id()));
    fs::create_dir_all(&outside).expect("make scratch cwd");
    // Guard the premise: if this directory is somehow inside a checkout, the
    // test is vacuous and would pass against the old behaviour.
    assert!(
        !outside.join("velox-core").exists()
            && !outside
                .canonicalize()
                .unwrap_or_else(|_| outside.clone())
                .join("velox-core")
                .exists(),
        "scratch cwd {outside:?} sits inside a velox checkout; this test proves nothing"
    );

    let p = ScratchProject::with_cwd("outside", Some(&outside));
    let toml = p.cargo_toml();
    let _ = fs::remove_dir_all(&outside);

    assert!(
        !toml.contains("git = "),
        "velox init wrote git-pinned dependencies:\n{toml}\n\
         A binary built from an unpushed commit cannot produce a scaffold that \
         resolves, so this makes every locally-built `velox init` app unbuildable. \
         init must use the workspace it was compiled from when that still exists."
    );
    assert!(
        toml.contains("path = "),
        "velox init wrote neither path nor git dependencies:\n{toml}"
    );
    // The path must point at a crate that exists, or cargo will not resolve it.
    let first = toml
        .split("path = \"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("a path = entry");
    let resolved = if Path::new(first).is_absolute() {
        PathBuf::from(first)
    } else {
        p.root.join(first)
    };
    assert!(
        resolved.join("Cargo.toml").is_file(),
        "generated Cargo.toml points velox-core at {resolved:?}, which has no \
         Cargo.toml — the scaffold still would not resolve."
    );
}

#[test]
fn init_scaffolds_every_component_the_template_ships() {
    // The reverse direction: a component in the template but not written by init
    // is not a bug on its own — the todo set is the default and extras are
    // legitimately `velox add`-only. This test records the counts so that adding a
    // template component and forgetting to decide about it shows up as a diff.
    let template_components = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("templates")
        .join("project")
        .join("src")
        .join("components");
    let mut in_template: Vec<String> = fs::read_dir(&template_components)
        .expect("template components dir exists")
        .map(|e| {
            e.expect("read entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|n| n.ends_with(".vx"))
        .collect();
    in_template.sort();

    let p = ScratchProject::new("counts");
    let mut written: Vec<String> = fs::read_dir(p.root.join("src").join("components"))
        .expect("init created src/components")
        .map(|e| {
            e.expect("read entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|n| n.ends_with(".vx"))
        .collect();
    written.sort();

    let not_written: Vec<&String> = in_template
        .iter()
        .filter(|n| !written.contains(n))
        .collect();

    assert!(
        not_written.is_empty(),
        "the template ships component(s) that init does not scaffold: {not_written:?}.\n\
         That is only correct if every one of them is deliberately `velox add`-only — \
         say so here when it is intentional, so the next reader is not left guessing."
    );
}
