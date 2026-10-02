//! A CSS function's ARGUMENTS must survive the declaration parser.
//!
//! ## The defect this locks down
//!
//! `DeclarationParser::parse_value` (`velox-style/src/lib.rs`) collected tokens
//! with `input.next_including_whitespace()` and never called
//! `parse_nested_block`. In cssparser, a function's arguments live in a nested
//! block: after the `Token::Function(name)` is returned, the *next* `next()`
//! call skips the tokenizer to after the matching `)`. So the loop ended with
//! only the opening `(` written, and every function value was truncated at its
//! first parenthesis:
//!
//! | authored | handed to the renderer |
//! |---|---|
//! | `background: rgba(20, 24, 27, 0.34)` | `background: rgba(` |
//! | `color: rgb(34, 197, 94)` | `color: rgb(` |
//! | `width: calc(100% - 8px)` | `width: calc(` |
//! | `color: var(--brand)` | `color: var(` |
//!
//! User-visible consequence: every `rgba()` scrim in the scaffolded templates is
//! a class rule, so every scrim was dropped and a dialog opened with NO dim
//! behind it. An inline `style` attribute worked only because it never goes
//! through this parser — which is why `velox-dom/tests/composer_pixels.rs:33`
//! (invariant #2) routes around it with an inline value.
//!
//! ## Why these assertions read the CASCADE, not the sheet
//!
//! The truncated value is invisible in the sheet text: `Stylesheet::parse`
//! receives a perfectly well-formed `background: rgba(20, 24, 27, 0.34)`. A
//! test that asserted on the sheet would pass with the bug in place and prove
//! nothing — the mistake `velox-style/tests/ua_completeness.rs` and
//! `velox-cli/tests/template_palette_wcag.rs` both make.
//!
//! So every test here goes `Stylesheet::parse` -> `apply_with_cascade` -> read
//! the `style` attribute back off the VNode. That attribute string is EXACTLY
//! what `velox-renderer`'s `parse_style_attr` (`velox-renderer/src/
//! skia_render.rs:561`) reads, so it is the real consumer contract.
//!
//! Run with:
//!   cargo test -p velox-style --test function_values_survive

use velox_dom::{Props, VNode, h};
use velox_style::{Stylesheet, apply_with_cascade};

/// The declaration value the cascade writes for `prop` onto a node carrying
/// `class`. Returns the value the RENDERER sees, i.e. what
/// `parse_style_attr` will be handed.
fn cascaded(css: &str, class: &str, prop: &str) -> String {
    let sheet = Stylesheet::parse(css);
    let node = h("div", Props::new().set("class", class), vec![]);
    let styled = apply_with_cascade(&node, &sheet);
    let VNode::Element { props, .. } = &styled else {
        panic!("expected element, got {styled:?}");
    };
    let style = props
        .attrs
        .get("style")
        .unwrap_or_else(|| panic!("cascade wrote no style attr for .{class}"));
    // The renderer splits on `;` and then on the FIRST `:`.
    style
        .split(';')
        .filter_map(|decl| decl.trim().split_once(':'))
        .find(|(k, _)| k.trim() == prop)
        .map(|(_, v)| v.trim().to_string())
        .unwrap_or_else(|| panic!("no `{prop}` in cascaded style: {style:?}"))
}

// --- The four values named in the defect report, each with a spaced form
// (spaced, because `rgba(0,0,255,0.5)` written tight and `rgba(0, 0, 255,
// 0.5)` written loose must BOTH survive, and the tight form is what hides a
// whitespace-only round-trip bug).

#[test]
fn rgba_arguments_survive_a_class_rule() {
    let v = cascaded(
        ".scrim { background: rgba(20, 24, 27, 0.34); }",
        "scrim",
        "background",
    );
    assert_eq!(
        v, "rgba(20, 24, 27, 0.34)",
        "rgba() arguments were eaten by the declaration parser"
    );
}

#[test]
fn rgb_arguments_survive_a_class_rule() {
    let v = cascaded(".ok { color: rgb(34, 197, 94); }", "ok", "color");
    assert_eq!(
        v, "rgb(34, 197, 94)",
        "rgb() arguments were eaten by the declaration parser"
    );
}

#[test]
fn calc_arguments_survive_a_class_rule() {
    let v = cascaded(".w { width: calc(100% - 8px); }", "w", "width");
    assert_eq!(
        v, "calc(100% - 8px)",
        "calc() arguments were eaten by the declaration parser"
    );
}

#[test]
fn var_arguments_survive_a_class_rule() {
    let v = cascaded(".brand { color: var(--brand); }", "brand", "color");
    assert_eq!(
        v, "var(--brand)",
        "var() arguments were eaten by the declaration parser"
    );
}

// --- Nesting: one function inside another, with a spaced inner expression.
// A fix that only handled ONE level of parentheses, or that re-serialized
// tokens without whitespace, would fail here.

#[test]
fn nested_function_arguments_survive_intact() {
    let v = cascaded(
        ".mix { background: rgba(calc(1 + 2), 0, 0, 1); }",
        "mix",
        "background",
    );
    assert_eq!(
        v, "rgba(calc(1 + 2), 0, 0, 1)",
        "a function nested inside a function did not round-trip"
    );
}

#[test]
fn three_levels_of_nesting_survive_intact() {
    let v = cascaded(
        ".deep { box-shadow: 0 2px 8px rgba(calc(0 + 1), 2, 3, var(--a)); }",
        "deep",
        "box-shadow",
    );
    assert_eq!(
        v, "0 2px 8px rgba(calc(0 + 1), 2, 3, var(--a))",
        "three levels of function nesting did not round-trip"
    );
}

// --- A bare parenthesised block is the same bug wearing a different hat: its
// contents live in the same unconsumed nested block.

#[test]
fn bare_parenthesis_block_is_not_truncated() {
    let v = cascaded(".p { width: (1 + 2); }", "p", "width");
    assert_eq!(
        v, "(1 + 2)",
        "a bare parenthesis block was truncated to `(`"
    );
}

// --- The CONSUMER contract, not just the string. `Color::parse`
// (velox-dom/src/style.rs:220) is what `parse_style_attr` calls on a
// `background` value, and it is what turns the value into painted pixels.
// If this still returns None the scrim still does not dim, however correct
// the round-trip looks.

#[test]
fn cascaded_rgba_is_parseable_by_the_renderer_s_colour_reader() {
    let v = cascaded(
        ".scrim { background: rgba(20, 24, 27, 0.34); }",
        "scrim",
        "background",
    );
    let c = velox_dom::style::Color::parse(&v)
        .unwrap_or_else(|| panic!("Color::parse rejected the cascaded value {v:?}"));
    assert_eq!((c.r, c.g, c.b), (20, 24, 27));
    // 0.34 * 255 = 86.7 -> 86
    assert_eq!(c.a, 86, "alpha 0.34 should survive as 86/255");
}

#[test]
fn cascaded_tight_rgba_is_parseable_too() {
    // The tight form is what `composer_pixels.rs` uses inline. It must behave
    // identically to the spaced form — a whitespace-only difference.
    let v = cascaded(
        ".add { background: rgba(0,0,255,0.5); }",
        "add",
        "background",
    );
    assert_eq!(v, "rgba(0,0,255,0.5)");
    let c = velox_dom::style::Color::parse(&v).expect("tight rgba must parse");
    assert_eq!((c.r, c.g, c.b, c.a), (0, 0, 255, 127));
}

// --- The bug did not stop at the first function: it ate EVERYTHING after the
// first `(` in the value. `transition: transform 0.2s ease` has no function at
// all, so it must be byte-identical to what the author wrote — a guard against
// a "fix" that rewrites whitespace globally.

#[test]
fn a_value_with_no_function_is_unchanged() {
    let v = cascaded(
        ".btn { transition: transform 0.2s ease; }",
        "btn",
        "transition",
    );
    assert_eq!(v, "transform 0.2s ease");
}

#[test]
fn a_function_after_other_tokens_does_not_eat_them() {
    // `border` is a plain keyword value followed by a function: the fix must not
    // lose either half.
    let v = cascaded(
        ".b { border: 1px solid rgba(0, 0, 0, 0.1); }",
        "b",
        "border",
    );
    assert_eq!(v, "1px solid rgba(0, 0, 0, 0.1)");
}

// --- Documented limitation, asserted so it cannot silently change: nothing in
// velox-dom resolves `calc()` or substitutes `var()`. The parser now hands
// them over INTACT; whether they are then RESOLVED is a separate defect with
// no reader. This test states the boundary rather than leaving it implied.

#[test]
fn calc_and_var_are_stored_but_still_not_resolved() {
    // Stored intact (the bug being fixed)...
    assert_eq!(
        cascaded(".w { width: calc(100% - 8px); }", "w", "width"),
        "calc(100% - 8px)"
    );
    // ...but `Length::parse` has no calc support, so it does not resolve yet.
    assert!(
        velox_dom::style::Length::parse("calc(100% - 8px)").is_none(),
        "if calc() now resolves, this test is stale: PARSE it and update it"
    );
}
