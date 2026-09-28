//! Requirement 1 of the R-1e brief, pinned as a property of the source instead of
//! of a rendered frame.
//!
//! The brief asks that a grep for the answerability idiom return hits only inside
//! the new predicate and its call sites, and that "a second notion anywhere is a
//! regression". Both are statements about which function decides, not about what
//! it decides, so no rendered output can witness them: this reads
//! `velox-sfc/src/template_codegen.rs` and checks the decision's location.
//!
//! What it does NOT do, and cannot: it does not check that the one predicate is
//! *right*. `velox-sfc/src/template_codegen.rs`'s own unit tests do that, by
//! differential against the pre-R-1e behaviour, and the run-a-generated-module
//! harness in `integration_compile.rs` does it again from the outside. This file
//! is a guard against a second copy appearing, which those two cannot see.

use std::collections::BTreeSet;
use std::path::PathBuf;

fn source() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/template_codegen.rs");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

/// The unit-test module's opening line. Everything at or after it is test code:
/// a fixture that calls a decider to pin its verdict is evidence, not a second
/// notion, and the guard is about codegen.
fn unit_test_boundary(lines: &[&str]) -> usize {
    let boundaries: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.trim() == "mod tests {")
        .map(|(index, _)| index)
        .collect();
    assert_eq!(
        boundaries.len(),
        1,
        "expected exactly one `mod tests {{`, found {} — the guard below would \
         scan the wrong region of the file if this ever stops being true",
        boundaries.len()
    );
    boundaries[0]
}

/// A line that can hold a decision. Comments are excluded: every decider is named
/// in prose somewhere, and a doc comment explaining what a helper is for is not a
/// second notion of it.
fn is_code(line: &str) -> bool {
    let trimmed = line.trim_start();
    !(trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*'))
}

/// The function a line sits in, or `None` at module scope.
fn enclosing_fn(lines: &[&str], target: usize) -> Option<String> {
    // `target + 1`, so the line itself is considered: a `fn` line is its own
    // enclosing function, and treating a definition as belonging to whatever was
    // declared above it is how a guard like this one ends up believing the
    // predicate's own helpers are a second notion.
    for line in lines.iter().take(target + 1).rev() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("pub ")
            && rest.starts_with("fn ")
        {
            return function_name(rest);
        }
        if let Some(rest) = trimmed.strip_prefix("fn ") {
            return function_name(rest);
        }
    }
    None
}

fn function_name(after_fn: &str) -> Option<String> {
    let name: String = after_fn
        .trim_start_matches("fn ")
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() { None } else { Some(name) }
}

/// The decision primitives, each with the only functions allowed to reach for it.
/// A primitive's own definition is exempt without being listed.
///
/// The two shape tests — `member_path` and `is_bare_identifier` — are asked only
/// by `answerable`: deciding which shape a key is belongs to the one place that
/// decides, and a second asker is a second notion. The two lookups are narrower.
/// `getter_method_name` is also read by `has_state_getter`, which is a different
/// question and stays: the loop-family report asks whether a `State` has a bare
/// getter, not whether a key can be rendered. `accessor_method_name` is read by
/// `member_path_call`, which has to know the root accessor to build a chain at
/// all, and returns `None` when there is not one — which is how the predicate
/// learns a chain is unanswerable.
const DECIDERS: [(&str, &[&str]); 5] = [
    ("member_path(", &["answerable", "member_path_call"]),
    ("is_bare_identifier", &["answerable"]),
    ("getter_method_name", &["answerable", "has_state_getter"]),
    ("accessor_method_name", &["member_path_call"]),
    ("member_path_call", &["answerable"]),
];

/// The four functions that used to decide for themselves, each of which must now
/// only read the predicate's verdict.
const READERS: [&str; 4] = [
    "unresolvable_key_reason",
    "key_is_answerable",
    "resolve_getter_call",
    "keep_answerable_interpolation_keys",
];

#[test]
fn an_answerability_primitive_is_reached_for_only_from_the_predicate() {
    let source = source();
    let lines: Vec<&str> = source.lines().collect();
    let boundary = unit_test_boundary(&lines);

    for (needle, allowed) in DECIDERS {
        let mut callers: BTreeSet<String> = BTreeSet::new();
        for (index, line) in lines.iter().enumerate().take(boundary) {
            if !is_code(line) || !line.contains(needle) {
                continue;
            }
            let caller = enclosing_fn(&lines, index)
                .unwrap_or_else(|| panic!("`{needle}` is called at module scope: {line}"));
            if caller == needle.trim_end_matches('(') {
                continue;
            }
            if !allowed.contains(&caller.as_str()) {
                panic!(
                    "`{needle}` is used by `{caller}`, which is not one of {allowed:?}. \
                     That is answerability decided a second time, in a function the \
                     one predicate does not reach; `{caller}` must call `answerable`."
                );
            }
            callers.insert(caller);
        }
        assert!(
            !callers.is_empty(),
            "`{needle}` has no caller at all — if the predicate stopped using it, \
             say so in its doc comment rather than leaving it dead code"
        );
    }
}

#[test]
fn every_function_that_used_to_decide_for_itself_only_reads_the_predicate() {
    let source = source();
    let lines: Vec<&str> = source.lines().collect();

    for reader in READERS {
        let start = lines
            .iter()
            .position(|line| line.starts_with(&format!("fn {reader}")))
            .unwrap_or_else(|| panic!("`fn {reader}` is gone; the guard is stale"));
        let mut depth = 0usize;
        let mut entered = false;
        let mut body = String::new();
        for line in &lines[start..] {
            depth += line.matches('{').count();
            depth -= line.matches('}').count();
            entered |= line.contains('{');
            body.push_str(line);
            body.push('\n');
            if entered && depth == 0 {
                break;
            }
        }

        assert!(
            body.contains("answerable("),
            "`{reader}` does not ask `answerable` at all"
        );
        for (needle, _) in DECIDERS {
            assert!(
                !body.contains(needle),
                "`{reader}` still reaches for `{needle}`, so it answers the question \
                 itself on one path and delegates on another"
            );
        }
    }
}

#[test]
fn the_predicate_is_the_only_function_that_holds_the_whole_decision() {
    let source = source();
    let lines: Vec<&str> = source.lines().collect();
    let boundary = unit_test_boundary(&lines);

    // `member_path(` is left out of this sweep because `member_path_call` calls it,
    // and that is one question asked at a layer below itself; the sweep over the
    // whole file above is where it is checked, and there it is the decider set,
    // not the predicate, that says so.
    let mut elsewhere: Vec<String> = Vec::new();
    for (needle, allowed) in DECIDERS {
        if needle == "member_path(" {
            continue;
        }
        for (index, line) in lines.iter().enumerate().take(boundary) {
            if !is_code(line) || !line.contains(needle) {
                continue;
            }
            let Some(name) = enclosing_fn(&lines, index) else {
                continue;
            };
            if name == needle.trim_end_matches('(') {
                continue;
            }
            if !allowed.contains(&name.as_str()) {
                elsewhere.push(format!("{name} ({needle})"));
            }
        }
    }
    elsewhere.sort();
    elsewhere.dedup();
    assert!(
        elsewhere.is_empty(),
        "{elsewhere:?} reach for a primitive that only `answerable` should hold"
    );
    assert_eq!(
        source.matches("fn answerable(").count(),
        1,
        "there is one predicate, not one plus a near-copy"
    );
    assert!(
        !lines
            .iter()
            .any(|line| line.starts_with("pub fn answerable")),
        "`answerable` is internal to the crate: a second notion of answerability \
         is not something `velox-dom` or a user's component should be able to \
         grow by importing it"
    );
}
