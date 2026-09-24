//! CLI-level behavior of the reactive-idiom lint (`Cell`/`RefCell` in a
//! `<script>` body).
//!
//! The lint is diagnostic only, so this exercises the real binary to pin what
//! users actually observe: warnings are printed but never change the exit
//! code, each occurrence is reported exactly once, and a file that fails to
//! parse reports only the real error.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A temporary `.vx` file that is deleted when the test finishes.
struct ScratchFile {
    path: PathBuf,
}

impl ScratchFile {
    fn new(tag: &str, content: &str) -> Self {
        let path = std::env::temp_dir().join(format!("velox-lint-{tag}-{}.vx", std::process::id()));
        fs::write(&path, content).expect("write scratch .vx");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ScratchFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Run `velox lint <path>` and return `(success, combined output)`.
fn run_lint(path: &Path) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_velox"))
        .arg("lint")
        .arg(path)
        .output()
        .expect("run velox lint");
    let mut output = String::from_utf8_lossy(&out.stdout).into_owned();
    output.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.success(), output)
}

/// Count the lint warning lines in CLI output.
fn warning_lines(output: &str) -> Vec<&str> {
    output.lines().filter(|l| l.contains('⚠')).collect()
}

const THREE_OCCURRENCES: &str = r#"<script setup>
use std::cell::Cell;
use std::cell::RefCell;

pub struct State {
    pub count: Cell<i32>,
    pub shared: RefCell<String>,
}
</script>

<template>
  <div>hi</div>
</template>
"#;

/// The lint is advisory: a file whose only problem is non-reactive storage
/// still exits 0, and every occurrence is reported exactly once (import lines
/// count, and `RefCell` is never double-reported as `Cell`).
#[test]
fn warnings_are_advisory_and_reported_once_per_occurrence() {
    let file = ScratchFile::new("advisory", THREE_OCCURRENCES);
    let (success, output) = run_lint(file.path());

    assert!(
        success,
        "lint warnings must not fail the command, output was:\n{output}"
    );

    let warnings = warning_lines(&output);
    assert_eq!(
        warnings.len(),
        4,
        "each of the four occurrences must be reported once, got:\n{output}"
    );
    // Line numbers within the `<script>` block: 2, 3, 6 and 7.
    for line in [
        "script line 2",
        "script line 3",
        "script line 6",
        "script line 7",
    ] {
        assert!(
            warnings.iter().any(|w| w.contains(line)),
            "expected a warning for {line}, got:\n{output}"
        );
    }
    assert_eq!(
        warnings.iter().filter(|w| w.contains("`RefCell`")).count(),
        2,
        "`RefCell` must be reported once per occurrence and never also as `Cell`:\n{output}"
    );
}

/// A file that does not parse must report the parse error and nothing else:
/// the lint never runs on a file the parser rejected, so a `Cell` in the same
/// file must not produce a warning.
#[test]
fn a_parse_error_stands_alone() {
    let broken = format!("{THREE_OCCURRENCES}\n<template>\n<div>unclosed</div>\n");
    let file = ScratchFile::new("parse-error", &broken);
    let (success, output) = run_lint(file.path());

    assert!(
        !success,
        "a parse error must fail the lint, output was:\n{output}"
    );
    assert!(
        output.contains("Parse error"),
        "the real error must be reported, output was:\n{output}"
    );
    assert!(
        warning_lines(&output).is_empty(),
        "no lint warning may be emitted for a file that failed to parse, output was:\n{output}"
    );
}

/// Prose that mentions `Cell` inside a raw string, a block comment or a Unicode
/// identifier is not code, so the CLI must stay silent and exit 0.
#[test]
fn raw_strings_and_block_comments_are_not_code() {
    let source = r##"<script setup>
pub struct State {
    pub message: &'static str,
    pub label: String,
}

impl State {
    pub fn new() -> Self {
        /* a Cell and a RefCell in a block comment */
        let raw = r#"a" Cell"#;
        let bytes = br"Cell";
        Self { message: raw, label: String::from("Cell") }
    }

    pub fn describe(&self) -> String {
        format!("{} Cellé {}", self.message, self.label)
    }
}
</script>

<template>
  <div>hi</div>
</template>
"##;
    let file = ScratchFile::new("raw-strings", source);
    let (success, output) = run_lint(file.path());

    assert!(success, "output was:\n{output}");
    assert!(
        warning_lines(&output).is_empty(),
        "raw strings, block comments and Unicode identifiers must not warn, output was:\n{output}"
    );
}

/// A multi-byte character immediately before an identifier used to abort the
/// lint process, because the boundary check sliced inside it. The CLI must
/// finish, stay advisory, and report only the real occurrence.
#[test]
fn unicode_before_an_identifier_does_not_panic() {
    let source = r#"<script setup>
pub struct State {
    pub count: std::rc::Rc<velox_core::signal::Signal<i32>>,
}

impl State {
    pub fn new() -> Self {
        Self { count: velox_core::signal!(count = 0) }
    }

    pub fn touch(&self) {
        let éCell = Cell::new(0);
        let other = Cell::new(1);
        let _ = (éCell, other);
    }
}
</script>

<template>
  <div>hi</div>
</template>
"#;
    let file = ScratchFile::new("unicode-boundary", source);
    let (success, output) = run_lint(file.path());

    assert!(
        success,
        "the lint must stay advisory, output was:\n{output}"
    );
    assert!(
        !output.contains("panicked"),
        "the lint must not panic, output was:\n{output}"
    );
    let warnings = warning_lines(&output);
    assert_eq!(
        warnings.len(),
        2,
        "the two standalone `Cell`s are non-reactive storage, but `éCell` is one \
         identifier and must not warn, output was:\n{output}"
    );
    assert!(
        warnings[0].contains("script line 12, column 22"),
        "expected the first Cell at line 12, column 22, got: {:?}",
        warnings[0]
    );
    assert!(
        warnings[1].contains("script line 13, column 21"),
        "expected the second Cell at line 13, column 21, got: {:?}",
        warnings[1]
    );
}
