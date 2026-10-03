//! CLI-level behavior of the CSS honesty lint: declarations that velox parses
//! into the style string but that no reader ever consults.
//!
//! The lint is diagnostic only, so these tests drive the real binary to pin
//! what users actually observe. The load-bearing property is **over-reporting**:
//! a lint that fires on vendor prefixes, custom properties and deliberately
//! declined properties would be noise, not honesty, so the "must not report"
//! cases are as important as the "must report" ones.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A temporary `.vx` file that is deleted when the test finishes.
struct ScratchFile {
    path: PathBuf,
}

impl ScratchFile {
    fn new(tag: &str, content: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("velox-lint-css-{tag}-{}.vx", std::process::id()));
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

/// The `velox-dom` style source, which holds both the table and the arms.
fn style_rs() -> String {
    fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../velox-dom/src/style.rs"),
    )
    .expect("read velox-dom/src/style.rs")
}

/// The CSS-lint warning lines, i.e. those naming a property.
fn css_warnings(output: &str) -> Vec<&str> {
    output
        .lines()
        .filter(|l| l.contains("parsed but never rendered"))
        .collect()
}

/// Wrap declarations in a minimal, parseable SFC.
fn fixture(decls: &str) -> String {
    format!(
        "<template>\n  <div class=\"card\">hi</div>\n</template>\n\n<style>\n.card {{\n{decls}\n}}\n</style>\n"
    )
}

/// Assert that `prop` is reported by the lint on a fixture declaring it.
fn assert_reported(tag: &str, decl: &str, prop: &str) {
    let file = ScratchFile::new(tag, &fixture(decl));
    let (success, output) = run_lint(file.path());
    assert!(
        success,
        "CSS lint warnings are advisory and must not fail the command, output was:\n{output}"
    );
    let warnings = css_warnings(&output);
    assert!(
        warnings.iter().any(|w| w.contains(&format!("`{prop}`"))),
        "`{prop}` is parsed but has no reader, so it must be reported, output was:\n{output}"
    );
}

/// Assert that `decl` produces no CSS warning at all.
fn assert_not_reported(tag: &str, decl: &str) {
    let file = ScratchFile::new(tag, &fixture(decl));
    let (_, output) = run_lint(file.path());
    assert!(
        css_warnings(&output).is_empty(),
        "`{decl}` must not be reported, output was:\n{output}"
    );
}

/// `transition` is armed and stored into `transitions`, but nothing ever plays
/// a transition, so the declaration silently does nothing.
#[test]
fn transition_is_reported() {
    assert_reported(
        "transition",
        "  transition: opacity 0.2s ease;",
        "transition",
    );
}

/// The sharpest form of the defect: `border` and `border-width` ARE read, so
/// `border: 1px solid red` works — but `border-color: red` on its own is
/// half-honoured and silently dropped.
#[test]
fn border_color_is_reported_but_the_border_shorthand_is_not() {
    assert_reported("border-color", "  border-color: red;", "border-color");
    assert_not_reported("border-shorthand", "  border: 1px solid red;");
}

/// `visibility: hidden` hides nothing: no reader in layout or the renderer
/// looks the key up. This is the most user-visible lie in the whole table.
#[test]
fn visibility_hidden_is_reported() {
    assert_reported("visibility", "  visibility: hidden;", "visibility");
}

/// Over-reporting guard. The cascade filters unknown declarations out
/// silently, so flagging them is wrong per the spec being cited — and vendor
/// prefixes, custom properties and deliberately-declined properties would all
/// fire. `float` is declined on purpose, not an oversight.
#[test]
fn unknown_prefixed_and_declared_properties_are_not_reported() {
    assert_not_reported("vendor", "  -webkit-something: yes;");
    assert_not_reported("custom-prop", "  --my-var: 3;");
    assert_not_reported("float", "  float: left;");
}

/// A file with no `<style>` block at all must not panic and must stay silent.
#[test]
fn a_file_without_a_style_block_is_silent() {
    let source = "<template>\n  <div>hi</div>\n</template>\n";
    let file = ScratchFile::new("no-style", source);
    let (success, output) = run_lint(file.path());
    assert!(success, "output was:\n{output}");
    assert!(
        !output.contains("panicked"),
        "the lint must not panic, output was:\n{output}"
    );
    assert!(
        css_warnings(&output).is_empty(),
        "a file with no <style> block has nothing to report, output was:\n{output}"
    );
}

/// A malformed stylesheet is skipped, not fatal: `Stylesheet::parse` drops
/// unparseable rules rather than failing, so the lint must still finish.
#[test]
fn a_malformed_stylesheet_does_not_panic() {
    let source =
        "<template>\n  <div>hi</div>\n</template>\n\n<style>\n.card { color: red\n</style>\n";
    let file = ScratchFile::new("malformed-css", source);
    let (success, output) = run_lint(file.path());
    assert!(success, "output was:\n{output}");
    assert!(
        !output.contains("panicked"),
        "a malformed stylesheet must not panic, output was:\n{output}"
    );
}

/// Every property the table names is actually reported end to end. This is
/// what makes falsification #1 meaningful: with the table emptied, every
/// assertion here goes red rather than the suite passing vacuously.
#[test]
fn every_table_member_is_reported() {
    for (prop, _) in velox_dom::style::ComputedStyle::PARSED_BUT_UNRENDERED {
        let tag = format!("all-{prop}");
        assert_reported(&tag, &format!("  {prop}: red;"), prop);
    }
}

/// Staleness guard. A property listed in the table with no `set_property` arm
/// is a stale entry, and the sibling task that adds arms will create exactly
/// that. The table is only honest while every member still has an arm.
#[test]
fn table_entries_all_have_a_set_property_arm() {
    let src = style_rs();
    for (prop, why) in velox_dom::style::ComputedStyle::PARSED_BUT_UNRENDERED {
        let arm = format!("\"{prop}\" =>");
        assert!(
            src.contains(&arm),
            "`{prop}` is in PARSED_BUT_UNRENDERED ({why}) but has no `set_property` arm; \
             a table entry without an arm is stale — remove it or add the arm"
        );
    }
}

/// The table cites the line of each arm, but the table lives *above* those arms —
/// so editing either one invalidates the other's line numbers. That is not
/// hypothetical: inserting the table shifted all ten by 66 lines. This test
/// re-derives every citation from the source, so a stale one fails loudly here
/// instead of quietly certifying an unrelated line as verified.
#[test]
fn table_cited_line_numbers_point_at_their_arms() {
    let src = style_rs();
    let lines: Vec<&str> = src.lines().collect();
    for (prop, why) in velox_dom::style::ComputedStyle::PARSED_BUT_UNRENDERED {
        let cited = why
            .split("arm at style.rs:")
            .nth(1)
            .unwrap_or_else(|| panic!("`{prop}` note does not cite an arm line: {why}"))
            .split_whitespace()
            .next()
            .expect("cited line number")
            .parse::<usize>()
            .expect("cited line number is a usize");
        let actual = lines
            .get(cited - 1)
            .unwrap_or_else(|| panic!("`{prop}` cites line {cited}, past end of file"));
        assert!(
            actual.contains(&format!("\"{prop}\" =>")),
            "`{prop}` cites style.rs:{cited}, but that line is: {actual:?}. \
             The table sits above the arms it cites, so adding or removing a \
             table row shifts them — re-grep and update every citation."
        );
    }
}
