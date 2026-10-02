//! css-flexbox-1 §4.5: a flex item's automatic minimum size.
//!
//! When the authored `min-width` is absent (its initial value is `auto`), or
//! the literal keyword `auto`, the used floor of the item's main size is the
//! item's content-based minimum size. An explicit length — including
//! `min-width: 0` — REPLACES that floor.
//!
//! These tests pin:
//!
//!   * a long unbreakable word in a shrinkable row item raises the used main
//!     size to the word's width (the content floor), overflowing the row —
//!     the pinned behavior, not an accident;
//!   * `min-width: 0` defeats the floor, so the same item shrinks into the
//!     row (the standard escape hatch actually works now);
//!   * an authored `min-width: auto` behaves exactly like an absent
//!     declaration — the distinguishing case: if the length parser ever
//!     returned `Some(0.0)` for the keyword (the M1 mutation), this arm
//!     would collapse to the escape-hatch value and go RED;
//!   * a breakable text's floor is its longest word, which does not bind
//!     when the row is wider than the word.

mod common;

use common::register_synthetic;
use velox_dom::{VNode, h, layout::compute_layout};

const FS: i32 = 16;
/// 10 characters of unbreakable text at the synthetic measurer's 0.5em width
/// per character: the content floor of the item below.
const LONG_WORD: i32 = 80;
/// A row narrow enough that shrink would otherwise take the item below the
/// floor.
const ROW: i32 = 60;

/// A one-item row, shrinkable via `flex: 1 1 0`, holding 10 chars of
/// unbreakable text, styled with `min` declarations.
fn row_with_word(min_style: &str) -> VNode {
    h(
        "div",
        vec![("style", format!("display:flex; width:{ROW}px;").as_str())],
        vec![h(
            "div",
            vec![(
                "style",
                format!("flex: 1 1 0; font-size: {FS}px; {min_style}").as_str(),
            )],
            vec![VNode::Text("aaaaaaaaaa".to_string())],
        )],
    )
}

#[test]
fn no_min_width_floors_the_item_at_its_longest_unbreakable_piece() {
    register_synthetic();
    let laid = compute_layout(&row_with_word(""), 600, 600);
    let w = laid.children[0].rect.w;
    println!("no min-width: rect.w = {w} (row {ROW}px, word {LONG_WORD}px)");
    assert_eq!(
        w, LONG_WORD,
        "with no `min-width`, the automatic §4.5 floor must hold the shrinkable \
         item at its content-based minimum (the unbreakable word's width), so \
         the row overflows rather than shrinking the item below it"
    );
}

#[test]
fn explicit_min_width_auto_is_the_automatic_floor() {
    register_synthetic();
    let laid = compute_layout(&row_with_word("min-width: auto;"), 600, 600);
    let w = laid.children[0].rect.w;
    println!("min-width: auto: rect.w = {w}");
    assert_eq!(
        w, LONG_WORD,
        "authored `min-width: auto` must resolve to the SAME automatic floor \
         as an absent declaration; if the length parser returns Some(0.0) for \
         the keyword (M1), this drops to {ROW} and goes RED"
    );
}

#[test]
fn min_width_zero_defeats_the_automatic_floor() {
    register_synthetic();
    let laid = compute_layout(&row_with_word("min-width: 0;"), 600, 600);
    let w = laid.children[0].rect.w;
    println!("min-width: 0: rect.w = {w}");
    assert_eq!(
        w, ROW,
        "an explicit `min-width: 0` is the escape hatch: the item must shrink \
         to the row's free space ({ROW}), not the content floor ({LONG_WORD}). \
         Distinguishing this from the two cases above is what makes the \
         automatic minimum real rather than accidental"
    );
}

#[test]
fn a_breakable_text_is_floored_at_its_longest_word_only() {
    register_synthetic();
    let tree = h(
        "div",
        vec![("style", format!("display:flex; width:{ROW}px;").as_str())],
        vec![h(
            "div",
            vec![("style", format!("flex: 1 1 0; font-size: {FS}px;").as_str())],
            vec![VNode::Text("aa bb cc".to_string())],
        )],
    );
    let laid = compute_layout(&tree, 600, 600);
    let w = laid.children[0].rect.w;
    println!("breakable text: rect.w = {w}");
    // Longest word "aa" is 16px; the row is 60px, so the floor does not bind
    // and the item fills the row via grow.
    assert_eq!(
        w, ROW,
        "a text that wraps must not be floored at its full unwrapped width: \
         the floor is the longest unbreakable piece, and when that fits, grow \
         fills the row"
    );
}
