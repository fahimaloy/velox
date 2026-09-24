//! Reactive-idiom lint for `<script>` bodies.
//!
//! `std::cell::Cell`/`RefCell` hold state that Velox's reactivity system
//! cannot observe: mutating them does not notify subscribers, so the UI only
//! updates when something else happens to trigger a redraw. `lint_script`
//! flags such usage and points users at the genuinely reactive primitives
//! (`ref!()` / `signal!()`).
//!
//! NOTE: the task brief originally placed this test at
//! `velox-core/tests/cell_lint.rs`, but the lint lives in `velox-sfc`
//! (the crate that owns script-body knowledge); `velox-core` cannot see it.
//! Per the pre-ruled adaptation, the test lives here instead.

use velox_sfc::lint_script;

/// The brief's required test, verbatim: a `Cell::new` constructor
/// (fully-qualified) must warn, naming both the matched token (`Cell`) and
/// the reactive alternative (`ref!`).
#[test]
fn cell_in_script_emits_warning() {
    let warnings = lint_script("let c = std::cell::Cell::new(0);");
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("Cell") && w.contains("ref!"))
    );
}

/// Bare `Cell::new` (after a `use std::cell::Cell;`) must warn too.
#[test]
fn bare_cell_new_emits_warning() {
    let warnings = lint_script("use std::cell::Cell;\nlet c = Cell::new(0);");
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("Cell") && w.contains("ref!")),
        "expected a Cell warning mentioning ref!, got: {warnings:?}"
    );
}

/// `RefCell` usage must warn, mentioning `ref!`/`signal!` as alternatives.
#[test]
fn refcell_in_script_emits_warning() {
    let warnings = lint_script("let s = std::cell::RefCell::new(String::new());");
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("RefCell") && w.contains("signal!")),
        "expected a RefCell warning mentioning signal!, got: {warnings:?}"
    );
}

/// `RefCell` must be reported exactly once (as `RefCell`), never a second
/// time as the `Cell` substring it ends with.
#[test]
fn refcell_reported_once_not_also_as_cell() {
    let warnings = lint_script("let s = RefCell::new(1);");
    assert_eq!(
        warnings.len(),
        1,
        "RefCell must yield exactly one warning, got: {warnings:?}"
    );
    assert!(warnings[0].contains("RefCell"));
}

/// Reactive idioms must never be flagged: `r#ref!`, `shallow!`, `signal!`,
/// `Signal`, `Ref`, and `ShallowRef` are the idioms the lint teaches.
#[test]
fn reactive_idioms_do_not_warn() {
    let script = "\
use velox_core::signal::Signal;
let count = velox_core::r#ref!(0);
let fast = velox_core::shallow!(true);
let todos = velox_core::signal!(todos = vec![String::from(\"a\")]);
let shared: std::rc::Rc<Signal<i32>> = std::rc::Rc::new(Signal::new(0));
";
    let warnings = lint_script(script);
    assert!(
        warnings.is_empty(),
        "reactive idioms must not warn, got: {warnings:?}"
    );
}

/// Mentions of `Cell`/`RefCell` inside comments or string literals are prose,
/// not code — they must not trigger the lint.
#[test]
fn mentions_in_comments_and_strings_do_not_warn() {
    let script = "\
// Cell is not reactive; RefCell is not either (comment prose).
let msg = \"std::cell::Cell stays inside this string\";
";
    let warnings = lint_script(script);
    assert!(
        warnings.is_empty(),
        "prose mentions must not warn, got: {warnings:?}"
    );
}
