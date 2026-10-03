//! The SFC grammar spells nesting as mutual recursion —
//! `nested_template = { template_open ~ template_body ~ "</template>" }` inside
//! `template_body = @{ (nested_template | !"</template>" ~ ANY)* }`
//! (`src/grammar.pest:51-52`) — with no ceiling of its own. pest offers no way
//! to bound a rule's depth, so the bound has to live outside the grammar, in
//! `parse_sfc`.
//!
//! These tests pin that bound: that it fires, that it fires with a message a
//! human can act on, that it points at the offending opener rather than at the
//! top-level `<template>` every file has, and that it is never tighter than the
//! element builder's own `MAX_TEMPLATE_DEPTH`.
//!
//! **The depths here are literal on purpose.** A fixture derived from
//! `MAX_NESTED_TEMPLATE_DEPTH + 4` moves its own goalposts: raise the constant
//! and the bomb still trips the (now larger) limit, so the test cannot tell
//! whether the guard is doing anything. Against a literal, the constant becomes
//! the thing under test — change it and this suite says so, which is the correct
//! response to a change to a safety limit.
//!
//! **What is NOT proved here.** No test asserts that pest survives an unbounded
//! recursion, because demonstrating that requires actually letting it recurse
//! until the stack dies — a process-killing experiment this suite will not run.
//! What these tests do establish is the weaker but checkable claim: the count of
//! `<template` substrings that could open a nested block is an upper bound on
//! the grammar's real nesting depth (every `nested_template` consumes a
//! distinct one), so a source that gets past this guard cannot be one that
//! recurses without limit. The bomb below is only four levels past the ceiling
//! deliberately: it is the shallowest source that exercises the guard, and it
//! keeps the unfalsified behaviour close to what the grammar handles anyway.

use velox_sfc::parse_sfc;
use velox_sfc::template_parse::MAX_TEMPLATE_DEPTH;

/// The ceiling `sfc::MAX_NESTED_TEMPLATE_DEPTH` is expected to hold. Spelled out
/// here rather than imported so the two cannot move together unnoticed.
const CEILING: usize = 256;

/// Depth of the recursion bomb: four levels past the ceiling.
const BOMB_DEPTH: usize = CEILING + 4;

/// A file whose slot fragments are nested just past the ceiling. Every fragment
/// is properly CLOSED, so this is a file pest would parse — by recursing once
/// per level. That is the whole point: an unclosed run of openers would make
/// pest fail on its own after one level, and the guard would be untested.
fn just_over_the_ceiling() -> String {
    let mut source = String::from("<template>\n");
    for level in 0..BOMB_DEPTH {
        source.push_str(&format!("<template v-slot:slot{level}><Slot{level}>\n"));
    }
    for level in (0..BOMB_DEPTH).rev() {
        source.push_str(&format!("</Slot{level}></template>\n"));
    }
    source.push_str("</template>\n");
    source
}

#[test]
fn a_recursion_bomb_is_refused_before_pest_sees_it() {
    let source = just_over_the_ceiling();
    let err = parse_sfc(&source).expect_err(
        "a slot-fragment tree this deep drives one `template_body` frame per level \
         in `grammar.pest`, with no ceiling of its own. parse_sfc must refuse it \
         before handing it to pest.",
    );
    assert!(
        err.contains("nested `<template` blocks exceed the limit of"),
        "the refusal must name what was exceeded and by how much, or the reader \
         cannot tell a parse bug from a depth limit: {err}"
    );
    assert!(
        err.contains(&format!("{CEILING}")),
        "the refusal must quote the ceiling ({CEILING}), so the reader knows what \
         to aim for. A ceiling that does not match this file means the limit moved \
         without the test noticing: {err}"
    );
}

/// The scan must walk the source by CHARACTER, not by byte. A `.vx` file may
/// contain any Unicode — an emoji in a label, CJK in a paragraph — and a byte
/// counter lands inside those characters and slices a `&str` across a boundary.
/// This is the shape that made the first version of the scan panic on every
/// existing fixture in the crate.
#[test]
fn a_source_with_multibyte_characters_is_scanned_safely() {
    let source = "<template>\n<p>こんにちは 🎉 — em dash, and ünïcödé</p>\n<span>🎉🎉🎉</span>\n</template>\n";
    parse_sfc(source).unwrap_or_else(|e| panic!("a Unicode template was refused: {e}"));
}

/// The same shape, but with the guard tripped: the panic has to be gone from the
/// path that BUILDS the refusal, not only from the path that skips it.
#[test]
fn a_multibyte_source_is_scanned_safely_even_when_the_guard_fires() {
    let mut source = String::from("<template>\n<p>こんにちは 🎉</p>\n");
    for level in 0..BOMB_DEPTH {
        source.push_str(&format!(
            "<template v-slot:slot{level}><Slot{level}>あ 🎉\n"
        ));
    }
    for level in (0..BOMB_DEPTH).rev() {
        source.push_str(&format!("</Slot{level}></template>\n"));
    }
    source.push_str("</template>\n");
    let err = parse_sfc(&source).expect_err("the depth guard did not fire on a Unicode bomb");
    // BOMB_DEPTH slot fragments plus the top-level `<template>` every file has.
    assert!(
        err.contains(&format!(
            "{} nested `<template` blocks exceed",
            BOMB_DEPTH + 1
        )),
        "every opener must be counted even when the text between them is \
         multi-byte: {err}"
    );
}

/// The ceiling is a published number (`sfc::MAX_NESTED_TEMPLATE_DEPTH`), so it
/// has to agree with the number the refusal quotes. Two constants that describe
/// one limit cannot both be right.
#[test]
fn the_published_ceiling_is_the_one_the_guard_enforces() {
    assert_eq!(
        CEILING,
        velox_sfc::sfc::MAX_NESTED_TEMPLATE_DEPTH,
        "`sfc::MAX_NESTED_TEMPLATE_DEPTH` moved. It is part of the crate's API, and \
         the bomb fixture and the refusal message are both pinned to it."
    );
}

/// The message alone is not enough: this has to read as a thing the author can
/// change, not a crash.
#[test]
fn the_refusal_says_what_to_do_instead() {
    let source = just_over_the_ceiling();
    let err = parse_sfc(&source).expect_err("the depth guard did not fire");
    assert!(
        err.contains("nest fewer than"),
        "the refusal must offer a way forward, not just report the number: {err}"
    );
}

/// Byte 0 is the top-level `<template>` that every SFC has, so it is never the
/// position worth reporting. A depth guard that points there sends the reader to
/// a tag they cannot do anything about.
#[test]
fn the_refusal_points_at_the_opening_tag_that_overran_the_limit() {
    let source = just_over_the_ceiling();
    let err = parse_sfc(&source).expect_err("the depth guard did not fire");

    // Line 1 is the top-level `<template>` and line 2 is `v-slot:slot0`, so the
    // Nth slot fragment sits on line N + 2. The top-level opener counts as
    // opener #1, which means the (ceiling + 1)th opener — the first past the
    // limit — is fragment number `ceiling - 1`, on line `ceiling + 1`.
    let top_level_line = 1;
    let overrun_line = CEILING + 1;
    assert!(
        err.contains(&format!("{overrun_line} |")),
        "expected a caret line for opener #{} — the one that takes the count past \
         {CEILING} — but the diagnostic pointed somewhere else:\n{err}",
        CEILING + 1
    );
    assert!(
        !err.contains(&format!("{top_level_line} |")),
        "the diagnostic points at line {top_level_line}, which is the top-level \
         `<template>` every file has. That tag is not the problem and the reader \
         can do nothing about it:\n{err}"
    );
}

/// The guard counts the literal `<template`, not the word. A file may name the
/// construct all over its prose, its comments and its markup; none of that makes
/// the grammar recurse, so none of it may count against the ceiling.
#[test]
fn a_file_that_merely_names_templates_is_not_penalised() {
    let filler = "<p>the &lt;template v-slot:x&gt; block nests</p>\n".repeat(400);
    let source =
        format!("<template>\n{filler}</template>\n<script setup>\n// template\n</script>\n");
    parse_sfc(&source).unwrap_or_else(|e| {
        panic!("a file with 400 mentions of the word and ONE real opener was refused: {e}")
    });
}

/// A tag that merely starts with the same letters is not a nested block, and
/// must not count. `template_open` cannot match `<templates>` — the character
/// after the literal has to be whitespace or `>` — so counting it would refuse
/// files over a tag the grammar does not even treat as markup.
#[test]
fn tags_that_only_start_with_template_do_not_count_towards_the_ceiling() {
    let filler = "<templates>x</templates>\n".repeat(400);
    let source = format!("<template>\n{filler}</template>\n");
    parse_sfc(&source).unwrap_or_else(|e| {
        panic!("400 `<templates>` tags were counted as 400 nested blocks: {e}")
    });
}

/// The invariant that keeps this guard honest over time: the grammar's ceiling
/// must never sit BELOW the element builder's. `MAX_TEMPLATE_DEPTH` bounds the
/// hand-written parser's open-element stack, which is checked after pest has
/// already returned; a tighter grammar ceiling would start refusing files the
/// rest of the crate accepts, and the refusal would look arbitrary.
#[test]
fn the_grammar_ceiling_is_never_below_the_element_builder_ceiling() {
    // `black_box` on the left operand keeps this a RUNTIME comparison. Without it
    // both sides are constants, the comparison is folded at compile time, and
    // clippy is right that the assertion is dead — a violated invariant would
    // break the build with no explanation instead of failing here with the
    // diagnostic below.
    let grammar_ceiling = std::hint::black_box(velox_sfc::sfc::MAX_NESTED_TEMPLATE_DEPTH);
    assert!(
        grammar_ceiling >= MAX_TEMPLATE_DEPTH,
        "the grammar's nesting ceiling ({grammar_ceiling}) is below the element builder's \
         ({MAX_TEMPLATE_DEPTH}), so the depth guard refuses files the builder is \
         perfectly willing to take",
    );
}
