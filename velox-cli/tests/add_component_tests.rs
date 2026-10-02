//! End-to-end behaviour of `velox add component`, driven through the real
//! binary so the assertions pin what a user actually observes.
//!
//! These are filesystem tests and they need a directory to scaffold into. The
//! scratch project is created under the workspace's own `target/` (gitignored,
//! and already Cargo's build scratch) rather than `std::env::temp_dir()`, so a
//! test run leaves nothing behind outside the repository and cannot collide
//! with another lane's work in the shared temp directory.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A throwaway Velox project: `src/App.vx` plus whatever the run scaffolds.
/// Deleted when the test finishes, on both the success and panic paths.
struct ScratchProject {
    root: PathBuf,
}

impl ScratchProject {
    fn new(tag: &str) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("velox-cli has a parent")
            .join("target")
            .join(format!("velox-add-scratch-{}-{tag}", std::process::id()));
        // A previous run that panicked mid-way leaves its directory behind;
        // the name is pid-scoped so that is only ever our own leftovers.
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).expect("create scratch src");
        fs::write(root.join("src").join("App.vx"), "<template></template>\n")
            .expect("write scratch App.vx");
        Self { root }
    }

    fn component_path(&self, file: &str) -> PathBuf {
        self.root.join("src").join("components").join(file)
    }

    /// Run `velox add component <args>` with the scratch project as cwd.
    fn add(&self, args: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_velox"))
            .arg("add")
            .arg("component")
            .args(args)
            .current_dir(&self.root)
            .output()
            .expect("run velox add component")
    }
}

impl Drop for ScratchProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn stdout_of(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn generates_a_pascal_case_file_with_named_and_default_slots() {
    let project = ScratchProject::new("basic");

    let out = project.add(&["side-bar"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The existing PascalCase filename behaviour is preserved.
    let path = project.component_path("SideBar.vx");
    assert!(path.exists(), "expected {}", path.display());
    let body = fs::read_to_string(&path).expect("read generated component");

    // A named slot plus the implicit one, so the component is usable as
    // scaffolded rather than being a bare div.
    assert!(body.contains("<slot name=\"header\">"), "{body}");
    assert!(body.contains("<slot>"), "{body}");
    assert!(!body.contains("name=\"default\""), "{body}");

    // Both authoring spellings are reported, so the user knows either works.
    let printed = stdout_of(&out);
    assert!(printed.contains("Slots: default, header"), "{printed}");
    assert!(printed.contains("<template v-slot:header>"), "{printed}");
    assert!(printed.contains("<template #header>"), "{printed}");

    // The pre-existing import-line hint is unchanged.
    assert!(
        printed.contains("import SideBar from './components/SideBar.vx'"),
        "{printed}"
    );
}

#[test]
fn reports_every_slot_it_generated() {
    let project = ScratchProject::new("report");
    let out = project.add(&["card"]);
    assert!(out.status.success());

    let printed = stdout_of(&out);
    let slots_line = printed
        .lines()
        .find(|l| l.contains("Slots:"))
        .expect("a Slots: line");
    let names: Vec<&str> = slots_line
        .trim()
        .trim_start_matches("Slots:")
        .split(',')
        .map(str::trim)
        .collect();
    assert_eq!(names, ["default", "header"]);
}

#[test]
fn slots_flag_chooses_the_named_set() {
    let project = ScratchProject::new("flag");
    let out = project.add(&["card", "--slots", "header,footerBar"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let body = fs::read_to_string(project.component_path("Card.vx")).expect("read component");
    assert!(body.contains("<slot name=\"header\">"), "{body}");
    // `footerBar` is folded to the name the parser registers, so the hint and
    // the `<slot>` agree.
    assert!(body.contains("<slot name=\"footer-bar\">"), "{body}");
    assert!(!body.contains("footerBar"), "{body}");

    let printed = stdout_of(&out);
    assert!(
        printed.contains("Slots: default, header, footer-bar"),
        "{printed}"
    );
    assert!(printed.contains("<template v-slot:header>"), "{printed}");
}

#[test]
fn short_slots_flag_is_accepted() {
    let project = ScratchProject::new("short");
    let out = project.add(&["card", "-s", "aside"]);
    assert!(out.status.success());
    let body = fs::read_to_string(project.component_path("Card.vx")).expect("read component");
    assert!(body.contains("<slot name=\"aside\">"), "{body}");
}

#[test]
fn existing_component_is_still_refused() {
    let project = ScratchProject::new("exists");
    assert!(project.add(&["card"]).status.success());

    let out = project.add(&["card"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("component already exists"), "{err}");
}

#[test]
fn requesting_the_implicit_default_slot_is_refused() {
    let project = ScratchProject::new("default-slot");
    let out = project.add(&["card", "--slots", "default"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("implicit slot"), "{err}");
    assert!(!project.component_path("Card.vx").exists());
}

#[test]
fn empty_slots_list_is_refused() {
    let project = ScratchProject::new("empty-slots");
    let out = project.add(&["card", "--slots", " , "]);
    assert!(!out.status.success());
    assert!(!project.component_path("Card.vx").exists());
}

#[test]
fn invalid_component_name_is_still_refused() {
    let project = ScratchProject::new("bad-name");
    let out = project.add(&["1foo"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("must start with a letter"), "{err}");
}

#[test]
fn project_root_is_found_from_a_subdirectory() {
    let project = ScratchProject::new("subdir");
    let nested = project.root.join("src").join("components");
    fs::create_dir_all(&nested).expect("create nested dir");

    let out = Command::new(env!("CARGO_BIN_EXE_velox"))
        .arg("add")
        .arg("component")
        .arg("card")
        .current_dir(&nested)
        .output()
        .expect("run velox add component");

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(project.component_path("Card.vx").exists());
}

// ===== the no-overwrite guard is atomic and does not follow symlinks ========
//
// `velox add component` must never write through a symlink it did not create,
// and must never truncate a file that appeared between a check and the write.
// Both used to be possible, because the guard was `Path::exists()` followed by
// `fs::write` — two syscalls, both of which resolve symlinks.
//
// Unix-only: planting a symlink needs `std::os::unix::fs::symlink`.

/// A pre-existing file is still refused — the control for the tests below, so
/// they cannot pass merely because the command stopped writing at all.
#[test]
fn a_pre_existing_real_file_is_still_refused() {
    let project = ScratchProject::new("preexisting");
    fs::create_dir_all(project.component_path("Card.vx").parent().unwrap())
        .expect("create components dir");
    fs::write(project.component_path("Card.vx"), "do not clobber\n").unwrap();

    let out = project.add(&["card"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("component already exists"), "{err}");
    assert_eq!(
        fs::read_to_string(project.component_path("Card.vx")).unwrap(),
        "do not clobber\n",
        "an existing component was overwritten"
    );
}

#[cfg(unix)]
#[test]
fn a_dangling_symlink_at_the_component_path_is_refused_and_not_written_through() {
    use std::os::unix::fs::symlink;
    let project = ScratchProject::new("dangling");
    let elsewhere = project.root.join("outside.vx");
    let link = project.component_path("Card.vx");
    fs::create_dir_all(link.parent().unwrap()).expect("create components dir");
    symlink(&elsewhere, &link).expect("plant dangling symlink");

    let out = project.add(&["card"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("component already exists"),
        "a dangling symlink is still a path that exists, and must be refused \
         with the same message: {err}"
    );
    assert!(
        !elsewhere.exists(),
        "the guard resolved the dangling symlink, found no target, and the \
         write then CREATED that target — scaffolding into a path the user \
         never asked for"
    );
}

#[cfg(unix)]
#[test]
fn a_live_symlink_at_the_component_path_is_refused_and_its_target_is_untouched() {
    use std::os::unix::fs::symlink;
    let project = ScratchProject::new("livesymlink");
    let elsewhere = project.root.join("outside.vx");
    fs::write(&elsewhere, "original\n").unwrap();
    let link = project.component_path("Card.vx");
    fs::create_dir_all(link.parent().unwrap()).expect("create components dir");
    symlink(&elsewhere, &link).expect("plant live symlink");

    let out = project.add(&["card"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("component already exists"));
    assert_eq!(
        fs::read_to_string(&elsewhere).unwrap(),
        "original\n",
        "the component was written THROUGH the symlink, truncating the target"
    );
}

#[cfg(unix)]
#[test]
fn a_symlinked_components_directory_is_refused() {
    use std::os::unix::fs::symlink;
    let project = ScratchProject::new("dirlink");
    let elsewhere = project.root.join("elsewhere-components");
    fs::create_dir_all(&elsewhere).expect("create elsewhere");
    symlink(&elsewhere, project.root.join("src").join("components"))
        .expect("plant components dir symlink");

    let out = project.add(&["card"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("symlink"), "{err}");
    assert!(
        !elsewhere.join("Card.vx").exists(),
        "the component was scaffolded through the symlinked directory"
    );
}
