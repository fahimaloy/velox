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

/// Bare `Cell::new` must warn at the real usage site. The `use` import is
/// deliberately absent: with it, the import alone would satisfy a loose
/// "some warning mentions Cell" assertion and the test would pass even if the
/// usage itself were missed.
#[test]
fn bare_cell_new_emits_warning() {
    let warnings = lint_script("let c = Cell::new(0);");
    assert_eq!(
        warnings.len(),
        1,
        "exactly one warning for the single occurrence, got: {warnings:?}"
    );
    assert!(
        warnings[0].contains("script line 1, column 9"),
        "warning must point at the usage (line 1, column 9), got: {:?}",
        warnings[0]
    );
    assert!(
        warnings[0].contains("`Cell`") && warnings[0].contains("ref!"),
        "warning must name the token and the reactive alternative, got: {:?}",
        warnings[0]
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

// ── Raw strings, lifetimes, Unicode identifiers ─────────────────────────────

/// A raw string may contain `"`, so `r#"a" Cell"#` is ONE literal whose body
/// mentions `Cell`. Treating the embedded quote as the end of a normal string
/// made the scanner read the rest as code (and then misclassify every
/// following line), which produced false warnings.
#[test]
fn raw_string_with_embedded_quote_does_not_warn() {
    let warnings = lint_script("let s = r#\"a\" Cell\"#;\n");
    assert!(
        warnings.is_empty(),
        "a Cell inside a raw string is prose, got: {warnings:?}"
    );
}

/// Every raw-string form Rust accepts, including the byte-string variants, is
/// blanked as a whole: `r"…"`, `r#"…"#`, `r##"…"##`, `br"…"` and `br#"…"#`.
#[test]
fn raw_string_forms_do_not_warn() {
    let script = "\
let a = r\"Cell\";
let b = r#\"RefCell\"#;
let c = r##\"Cell and RefCell\"##;
let d = br\"Cell\";
let e = br#\"RefCell\"#;
let f = b\"Cell\";
";
    let warnings = lint_script(script);
    assert!(
        warnings.is_empty(),
        "raw and byte strings must not warn, got: {warnings:?}"
    );
}

/// A raw string that spans lines keeps later lines out of the scan, and the
/// code after its terminator is still inspected.
#[test]
fn multiline_raw_string_is_blanked_and_following_code_is_still_linted() {
    let script = "\
let doc = r#\"first line
a \" quote and a RefCell mention
last line\"#;
let c = Cell::new(0);
";
    let warnings = lint_script(script);
    assert_eq!(
        warnings.len(),
        1,
        "only the real usage may warn, got: {warnings:?}"
    );
    assert!(
        warnings[0].contains("script line 4, column 9"),
        "the warning must point at line 4, column 9, got: {:?}",
        warnings[0]
    );
}

/// A lifetime is not a char literal. `char_literal_len` used to blank anything
/// between two `'` characters, so a lifetime followed by a usage and a char
/// literal on the same line hid a real `Cell` — a false NEGATIVE.
#[test]
fn lifetime_does_not_hide_a_later_cell_usage() {
    let warnings = lint_script("let c: Foo<'a> = Cell('x');\n");
    assert_eq!(
        warnings.len(),
        1,
        "the Cell after the lifetime must still warn, got: {warnings:?}"
    );
    assert!(
        warnings[0].contains("script line 1, column 18"),
        "the warning must point at the usage (line 1, column 18), got: {:?}",
        warnings[0]
    );
}

/// A `Cell` type parameterized by a lifetime is real code and must warn; the
/// lifetime must not blank the type or the usage.
#[test]
fn cell_with_a_lifetime_parameter_still_warns() {
    let warnings = lint_script("let c: Cell<'b> = Cell::new(0);\n");
    assert_eq!(
        warnings.len(),
        1,
        "a Cell<'b> type must warn, got: {warnings:?}"
    );
    assert!(
        warnings[0].contains("script line 1, column 8"),
        "the warning must point at the type (line 1, column 8), got: {:?}",
        warnings[0]
    );
}

/// `Cellé` is one identifier, not `Cell` followed by a boundary.
#[test]
fn unicode_identifier_does_not_warn() {
    let warnings = lint_script("let Cellé = 1;\nlet naïve = Cellé;\n");
    assert!(
        warnings.is_empty(),
        "a Unicode identifier containing Cell is not non-reactive storage, got: {warnings:?}"
    );
}

// ── Negative coverage ───────────────────────────────────────────────────────

/// Identifiers that merely start or end with the token are different names.
#[test]
fn identifier_boundaries_do_not_warn() {
    let script = "\
let Cellular = 1;
let MyCell = 2;
let Cell2 = 3;
let x = Cellular + MyCell + Cell2;
";
    let warnings = lint_script(script);
    assert!(
        warnings.is_empty(),
        "Cellular/MyCell/Cell2 are not Cell, got: {warnings:?}"
    );
}

/// `Cell`/`RefCell` inside block comments (including a nested one) are prose.
#[test]
fn block_comments_containing_cell_do_not_warn() {
    let script = "\
/* a Cell in a block comment */
/* outer /* a nested RefCell */ still a comment */
let c = 1;
";
    let warnings = lint_script(script);
    assert!(
        warnings.is_empty(),
        "block-comment mentions must not warn, got: {warnings:?}"
    );
}

/// A script that never uses interior mutability is clean, independent of the
/// reactive-idiom examples used by the other test.
#[test]
fn clean_script_does_not_warn() {
    let script = "\
pub struct State {
    pub count: i32,
    pub label: String,
}

impl State {
    pub fn new() -> Self {
        Self { count: 0, label: String::from(\"velox\") }
    }

    pub fn increment(&mut self) {
        self.count += 1;
    }
}
";
    let warnings = lint_script(script);
    assert!(
        warnings.is_empty(),
        "a clean script must not warn, got: {warnings:?}"
    );
}

/// Char literals are literals, and a lifetime-heavy signature is not a char
/// literal: both must be scanned without hiding or inventing matches.
#[test]
fn char_literals_and_lifetimes_are_scanned_correctly() {
    let script = "\
fn f<'a>(ch: char, c: Cell<i32>) -> char {
    let _ = '\\n';
    let _ = '\\u{1F600}';
    let _ = '\"';
    c.get();
    ch
}
";
    let warnings = lint_script(script);
    assert_eq!(
        warnings.len(),
        1,
        "only the Cell type may warn, got: {warnings:?}"
    );
    assert!(
        warnings[0].contains("script line 1, column 23"),
        "the warning must point at the Cell type, got: {:?}",
        warnings[0]
    );
}
