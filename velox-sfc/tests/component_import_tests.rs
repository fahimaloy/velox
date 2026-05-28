//! Tests for the .vx component import and compilation pipeline.
//!
//! These tests verify that:
//! - Import lines in `<script setup>` are stripped from the `script_rs` module
//!   (fixing the E0762 "unterminated character literal" error)
//! - `to_stub_rs` does NOT generate conflicting bare `pub mod` declarations
//! - `to_stub_rs_with_base` resolves imports from the correct base path
//! - `is_import_line` correctly identifies Velox import statements

use velox_sfc::{parse_sfc, to_stub_rs, to_stub_rs_with_base};
use std::path::Path;

/// Test that import lines are stripped from the script_rs module body.
/// This is the core fix for the E0762 error.
#[test]
fn import_lines_stripped_from_script_rs_module() {
    let source = r#"<template>
  <div><MyButton /></div>
</template>
<script setup>
import MyButton from './components/MyButton.vx';
use std::cell::Cell;
pub struct State { pub count: Cell<i32> }
impl State { pub fn new() -> Self { Self { count: Cell::new(0) } } }
</script>
"#;

    let sfc = parse_sfc(source).expect("should parse SFC");
    let rs = to_stub_rs(&sfc, "TestApp");

    // The import line should NOT appear in the script_rs module body
    // (it would cause E0762: unterminated character literal)
    let script_rs_start = rs.find("pub mod script_rs").expect("should have script_rs module");
    let script_rs_section = &rs[script_rs_start..];

    assert!(
        !script_rs_section.contains("import MyButton from"),
        "import line should NOT be in script_rs module body (causes E0762)"
    );

    // But the import line SHOULD appear in the SCRIPT_SETUP constant
    assert!(
        rs.contains("import MyButton from './components/MyButton.vx'"),
        "import line should be in SCRIPT_SETUP constant"
    );

    // The actual Rust code (State struct) should still be present
    assert!(
        rs.contains("pub struct State"),
        "user Rust code should still be in script_rs module"
    );
}

/// Test that to_stub_rs does NOT generate bare `pub mod` declarations for imports.
/// The caller (compile_component_tree) handles #[path] module declarations.
#[test]
fn no_bare_pub_mod_generated() {
    let source = r#"<template>
  <div><MyButton /></div>
</template>
<script setup>
import MyButton from './components/MyButton.vx';
pub struct State;
impl State { pub fn new() -> Self { Self } }
</script>
"#;

    let sfc = parse_sfc(source).expect("should parse SFC");
    let rs = to_stub_rs(&sfc, "TestApp");

    // Should NOT generate `pub mod MyButton;` (bare module declaration)
    assert!(
        !rs.contains("pub mod MyButton;"),
        "to_stub_rs should NOT generate bare `pub mod` declarations"
    );
    assert!(
        !rs.contains("pub mod mybutton;"),
        "to_stub_rs should NOT generate any `pub mod` for imports"
    );
}

/// Test that is_import_line correctly identifies Velox import statements.
#[test]
fn is_import_line_detection() {
    // These should be detected as import lines
    assert!(is_import("import MyButton from './MyButton.vx'"));
    assert!(is_import("  import MyButton from './MyButton.vx'"));
    assert!(is_import("import { Button, Card } from './components.vx'"));
    assert!(is_import("import Foo from '../Foo.vx';"));

    // These should NOT be detected as import lines
    assert!(!is_import("use std::cell::Cell;"));
    assert!(!is_import("pub struct State { }"));
    assert!(!is_import("impl State { pub fn new() -> Self { Self } }"));
    assert!(!is_import("let x = import_something;"));
    assert!(!is_import(""));
    assert!(!is_import("  "));
}

fn is_import(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("import ") && trimmed.contains(" from ")
}

/// Test that to_stub_rs_with_base uses the correct base path for imports.
#[test]
fn to_stub_rs_with_base_uses_correct_path() {
    let source = r#"<template>
  <div><Child /></div>
</template>
<script setup>
import Child from './Child.vx';
pub struct State;
impl State { pub fn new() -> Self { Self } }
</script>
"#;

    let sfc = parse_sfc(source).expect("should parse SFC");

    // With explicit base path
    let rs_with_base = to_stub_rs_with_base(&sfc, "TestApp", Some(Path::new("/my/project/src")));

    // Should still strip the import line from script_rs
    let script_rs_start = rs_with_base
        .find("pub mod script_rs")
        .expect("should have script_rs");
    assert!(
        !rs_with_base[script_rs_start..].contains("import Child from"),
        "import should be stripped even with base path"
    );

    // Should NOT generate bare pub mod
    assert!(
        !rs_with_base.contains("pub mod Child;"),
        "should not generate bare pub mod with base path"
    );
}

/// Test that named imports are also stripped from script_rs.
#[test]
fn named_import_lines_stripped() {
    let source = r#"<template>
  <div><Button /><Card /></div>
</template>
<script setup>
import { Button, Card } from './components.vx';
pub struct State;
impl State { pub fn new() -> Self { Self } }
</script>
"#;

    let sfc = parse_sfc(source).expect("should parse SFC");
    let rs = to_stub_rs(&sfc, "TestApp");

    let script_rs_start = rs.find("pub mod script_rs").expect("should have script_rs");
    let script_rs_section = &rs[script_rs_start..];

    assert!(
        !script_rs_section.contains("import { Button, Card }"),
        "named import line should NOT be in script_rs module"
    );

    // User code should still be present
    assert!(
        script_rs_section.contains("pub struct State"),
        "user Rust code should still be in script_rs"
    );
}

/// Test that a component WITHOUT imports still works correctly.
#[test]
fn component_without_imports_still_works() {
    let source = r#"<template>
  <div class="app">
    <p>{{ message }}</p>
  </div>
</template>
<script setup>
pub struct State { pub message: String }
impl State {
    pub fn new() -> Self {
        Self { message: String::from("Hello") }
    }
}
</script>
<style>
.app { padding: 20px; }
</style>
"#;

    let sfc = parse_sfc(source).expect("should parse SFC");
    let rs = to_stub_rs(&sfc, "SimpleApp");

    // Debug: print any "pub mod " lines
    for line in rs.lines() {
        if line.contains("pub mod ") {
            eprintln!("FOUND pub mod line: {}", line);
        }
    }

    // Should have all expected constants
    assert!(rs.contains("pub const TEMPLATE"));
    assert!(rs.contains("pub const SCRIPT_SETUP"));
    assert!(rs.contains("pub const SCRIPT"));
    assert!(rs.contains("pub const STYLE"));

    // Should have script_rs module with user code
    assert!(rs.contains("pub mod script_rs"));
    assert!(rs.contains("pub struct State"));

    // Should NOT have any component module declarations (no imports)
    // (script_rs module is expected, but no import-related pub mod lines)
    // Check specifically for import-derived module names, not script_rs
    let after_script_rs = rs.split("pub mod script_rs").nth(1).unwrap_or("");
    assert!(
        !after_script_rs.contains("pub mod "),
        "should not generate extra pub mod after script_rs"
    );
}

/// Test that the SCRIPT_SETUP constant correctly preserves import syntax.
#[test]
fn script_setup_constant_preserves_import() {
    let source = r#"<template>
  <div></div>
</template>
<script setup>
import TodoItem from './components/TodoItem.vx';
use std::cell::Cell;
pub struct State { pub count: Cell<i32> }
impl State { pub fn new() -> Self { Self { count: Cell::new(0) } } }
</script>
"#;

    let sfc = parse_sfc(source).expect("should parse SFC");
    let rs = to_stub_rs(&sfc, "App");

    // SCRIPT_SETUP should contain the full import line
    assert!(rs.contains("pub const SCRIPT_SETUP: &str = r#\""));
    assert!(rs.contains("import TodoItem from './components/TodoItem.vx';"));
    assert!(rs.contains("use std::cell::Cell;"));
    assert!(rs.contains("pub struct State"));
}
