//! Depth-cap tests for the template parser.
//!
//! `MAX_TEMPLATE_DEPTH` bounds element nesting so that a hostile or
//! machine-generated `.vx` file fails with an actionable error instead of doing
//! unbounded work. This is deliberately *not* a stack-overflow guard: the parse
//! loop keeps its open elements on a heap `Vec`, so it is iterative. See the
//! constant's doc comment for the full reasoning.

use velox_sfc::template_ast::{AttrKind, Node, TemplateAttr};
use velox_sfc::template_parse::MAX_TEMPLATE_DEPTH;

/// A template with `depth` unclosed `<div>` openers, i.e. exactly `depth`
/// levels of nesting.
///
/// Nothing closes them, so parsing ends by draining the stack and reporting the
/// leftovers as unclosed-tag warnings. That drain is what makes each level a
/// real push onto the stack under test rather than something the parser folds
/// away on the way in.
fn nested_openers(depth: usize) -> String {
    "<div>".repeat(depth)
}

/// Depth of the deepest element nesting in a parsed forest.
fn max_nesting_depth(nodes: &[Node]) -> usize {
    nodes
        .iter()
        .map(|node| match node {
            Node::Element { children, .. } => 1 + max_nesting_depth(children),
            _ => 0,
        })
        .max()
        .unwrap_or(0)
}

fn attr(name: &str, value: &str, kind: AttrKind) -> TemplateAttr {
    TemplateAttr {
        name: name.to_string(),
        value: Some(value.to_string()),
        kind,
    }
}

fn el(tag: &str, children: Vec<Node>) -> Node {
    Node::Element {
        tag: tag.to_string(),
        attrs: Vec::new(),
        children,
        self_closing: false,
    }
}

/// The boundary itself: exactly `MAX_TEMPLATE_DEPTH` levels nest, and one more
/// does not.
///
/// These two cases are the empirical boundary measurement. They are written
/// against the constant rather than a hand-copied `256` so the pair cannot
/// silently drift apart, and so a change to the constant shows up here as a
/// failing test instead of a stale number in a comment.
#[test]
fn nesting_boundary_is_exactly_max_template_depth() {
    let at_cap = velox_sfc::parse_template_to_ast(&nested_openers(MAX_TEMPLATE_DEPTH))
        .expect("nesting exactly at the cap must still parse");

    // Not merely `Ok` — genuinely MAX_TEMPLATE_DEPTH levels deep, so the guard
    // cannot be passing this for some incidental reason.
    assert_eq!(
        max_nesting_depth(&at_cap),
        MAX_TEMPLATE_DEPTH,
        "input at the cap should nest exactly as deep as the cap allows"
    );

    let over_cap = velox_sfc::parse_template_to_ast(&nested_openers(MAX_TEMPLATE_DEPTH + 1))
        .expect_err("nesting one level past the cap must be rejected");

    assert!(
        over_cap.contains("depth"),
        "error past the cap should name the problem as a depth limit, got: {over_cap}"
    );
}

/// One below the cap still parses — a guard that only ever exercised its own
/// boundary would not catch an off-by-one in the permissive direction.
#[test]
fn nesting_just_under_the_cap_parses() {
    let nodes = velox_sfc::parse_template_to_ast(&nested_openers(MAX_TEMPLATE_DEPTH - 1))
        .expect("nesting one level under the cap must parse");

    assert_eq!(max_nesting_depth(&nodes), MAX_TEMPLATE_DEPTH - 1);
}

/// The diagnostic has to be actionable, not just a refusal: it must name the
/// limit so a user hitting it knows what to change.
#[test]
fn depth_error_names_the_limit() {
    let err = velox_sfc::parse_template_to_ast(&nested_openers(MAX_TEMPLATE_DEPTH + 1))
        .expect_err("nesting past the cap must be rejected");

    assert!(
        err.contains("depth"),
        "message should contain `depth`, got: {err}"
    );
    assert!(
        err.contains(&MAX_TEMPLATE_DEPTH.to_string()),
        "message should name the limit ({MAX_TEMPLATE_DEPTH}), got: {err}"
    );
}

/// A realistic bomb — well past the cap, not one level over it — still fails,
/// and fails on the depth guard rather than incidentally (e.g. on some
/// unrelated limit that happens to trip at a similar size).
///
/// 300 rather than 5000 deliberately: `velox_dom::layout`'s `at()` is recursive
/// with a ~2000-line stack frame, so a far deeper template would make this
/// integration test's *other* calls overflow the 8 MB main-thread stack and
/// turn a clean failure into a flaky SIGSEGV. The cap under test is 256, so 300
/// is already unambiguously over it.
#[test]
fn realistic_bomb_depth_fails_on_the_depth_guard() {
    let err = velox_sfc::parse_template_to_ast(&nested_openers(300))
        .expect_err("a 300-deep template must be rejected");

    assert!(
        err.contains("depth") && err.contains(&MAX_TEMPLATE_DEPTH.to_string()),
        "300 levels deep should trip the depth guard naming the limit, got: {err}"
    );
}

/// The guard is a plain, deterministic bound — no dependence on prior state,
/// allocation luck, or hash ordering.
#[test]
fn depth_error_is_deterministic_across_repeats() {
    let source = nested_openers(300);
    let first = velox_sfc::parse_template_to_ast(&source).expect_err("must be rejected");

    for attempt in 1..5 {
        let again = velox_sfc::parse_template_to_ast(&source)
            .expect_err("must be rejected on every repeat");
        assert_eq!(
            first, again,
            "attempt {attempt} produced a different diagnostic than the first"
        );
    }
}

/// The cap is on *nesting*, not on the number of tags. A wide, flat template is
/// not a deep one and must keep working — otherwise the guard would be
/// rejecting legitimate input.
#[test]
fn wide_flat_template_does_not_trip_the_cap() {
    // 5000 sibling elements, all at depth 1: many more tags than the cap, none
    // of them nested. Self-closing elements never reach the stack at all.
    let source = "<br/>".repeat(5000);

    let nodes = velox_sfc::parse_template_to_ast(&source)
        .expect("a wide flat template is not a deep one and must parse");

    assert_eq!(nodes.len(), 5000, "all 5000 siblings should be present");
    assert_eq!(max_nesting_depth(&nodes), 1);
}

/// Regression guard: an ordinary, moderately nested template still produces
/// exactly the AST it produced before the guard existed.
///
/// The expected tree is spelled out rather than captured from a run, so this
/// asserts against a known shape instead of blessing whatever the current
/// parser happens to emit.
#[test]
fn normal_template_ast_is_unchanged() {
    let source = concat!(
        r#"<div class="root">"#,
        "<span>hi</span>",
        r#"<TodoInput :value="draft" @input="set_draft"/>"#,
        "</div>",
    );

    let nodes = velox_sfc::parse_template_to_ast(source).expect("a normal template must parse");

    let expected = vec![Node::Element {
        tag: "div".to_string(),
        attrs: vec![attr("class", "root", AttrKind::Static)],
        children: vec![
            el("span", vec![Node::Text("hi".to_string())]),
            Node::Element {
                tag: "TodoInput".to_string(),
                // The parser strips the `:`/`@` sigil and records it in `kind`,
                // so the stored name is bare.
                attrs: vec![
                    attr("value", "draft", AttrKind::Bind),
                    attr("input", "set_draft", AttrKind::On),
                ],
                children: Vec::new(),
                self_closing: true,
            },
        ],
        self_closing: false,
    }];

    assert_eq!(nodes, expected);
}

/// The second public entry point is guarded too. `parse_template` shares
/// `parse_template_inner` with `parse_template_to_ast`, but that sharing is an
/// implementation detail this test pins, so a future refactor cannot quietly
/// leave one entry point unbounded.
#[test]
fn both_public_entry_points_enforce_the_cap() {
    let source = nested_openers(MAX_TEMPLATE_DEPTH + 1);

    let via_to_ast = velox_sfc::parse_template_to_ast(&source).expect_err("must be rejected");
    let via_parse_template =
        velox_sfc::parse_template(&source, &[]).expect_err("must be rejected here too");

    assert!(via_to_ast.contains("depth"), "got: {via_to_ast}");
    assert!(
        via_parse_template.contains("depth"),
        "got: {via_parse_template}"
    );
}

/// The depth diagnostic survives to the code generator.
///
/// `sfc::template_warnings` discards a template-parse `Err` (see
/// `depth_error_is_not_a_parse_sfc_warning` below), so this is the test that
/// shows the error is not actually lost: the compile path re-parses the
/// template and propagates the `Err` with `?`, which is the path
/// `veloxc`'s build command calls. Without this, the guard could be
/// rejecting input with a message only the parser ever saw.
#[test]
fn compile_path_propagates_the_depth_error() {
    let err = velox_sfc::compile_template_to_rs(&nested_openers(300), "Deep", None)
        .expect_err("compiling a 300-deep template must fail");

    assert!(
        err.contains("depth"),
        "the compile path should surface the depth error, got: {err}"
    );
}

/// Documents what `sfc::parse_sfc` actually does with a depth error, which is
/// the other half of the swallow question.
///
/// It *succeeds*, and it keeps the full raw template text in
/// `Sfc::template.content` — the depth error is not added to `warnings`. That
/// is the documented contract of `template_warnings` (hard template errors are
/// not warnings; they surface from the compile path, proven by the test above),
/// and it is why the `Err(_) => Vec::new()` there is leniency rather than data
/// loss. This test pins that contract so a future change to it is deliberate.
#[test]
fn depth_error_is_not_a_parse_sfc_warning() {
    let deep = nested_openers(300);
    let source = format!("<template>{deep}</template>");

    let sfc = velox_sfc::sfc::parse_sfc(&source).expect("parse_sfc still splits blocks");

    let template = sfc
        .template
        .expect("the template block should have been captured");
    assert_eq!(
        template.content, deep,
        "raw template text must be preserved verbatim, not truncated by the depth guard"
    );
    assert!(
        !sfc.warnings.iter().any(|w| w.contains("depth")),
        "a hard template error is not a warning; got: {:?}",
        sfc.warnings
    );
}
