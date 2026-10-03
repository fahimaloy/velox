//! The public focus/blur API — `focus_input`, `blur_focused_input`,
//! `any_input_focused`, `focused_input_path` — and the Esc behaviour that rides
//! on top of it.
//!
//! # Why this file exists
//!
//! Focus used to have exactly one writer, `apply_click_focus`, and it was a
//! private `fn` in `lib.rs` behind `#[cfg(feature = "skia-native")]`. Blur was
//! not an API at all: it was the empty-space `else` branch inside that function.
//! So nothing outside the crate could focus a field — which is why the Confirm
//! core component, which has a text input, could not be built: there was no way
//! to give its field focus except by synthesising a mouse click at the right
//! coordinate.
//!
//! These functions live in `events.rs` rather than `lib.rs` for the same reason
//! `apply_edit` does: they are not behind the feature gate, so they are
//! reachable — and therefore testable — in the plain `cargo test -p
//! velox-renderer` build, where the `edit_focus_tests` module in `lib.rs` is
//! compiled out entirely.
//!
//! # The invariant
//!
//! `InputTarget::focused` is the single source of truth. There is no second
//! "currently focused element" anywhere in the renderer, no focus-ring variable,
//! no id-to-element map. `focused_input_index`, `focused_input_path` and
//! `any_input_focused` are all *reads* of that one flag, and
//! `focus_input`/`blur_focused_input`/`EditAction::Blur` are all *writers* of it.
//! The tests below assert that from both ends: that the flag really does move,
//! and that the readers agree with the flag.

use velox_renderer::events::{
    EditAction, InputTarget, StackCtx, any_input_focused, apply_edit, blur_focused_input,
    focus_input, focused_input_index, focused_input_path,
};

/// An unfocused text field at index `i` of the page, carrying `path`.
fn field(i: usize, path: &[usize]) -> InputTarget {
    InputTarget {
        rect: velox_dom::layout::Rect {
            x: (i as i32) * 100,
            y: 0,
            w: 100,
            h: 20,
        },
        path: path.to_vec(),
        z_index: 0,
        order: i as i32,
        clip: None,
        sc: StackCtx::ROOT,
        focused: false,
        cursor: 0,
        anchor: None,
        blink_on: true,
    }
}

/// Three unfocused fields: `vec![0]`, `vec![0, 1]`, `vec![1]`.
fn page() -> Vec<InputTarget> {
    vec![field(0, &[0]), field(1, &[0, 1]), field(2, &[1])]
}

// ---------------------------------------------------------------------------
// any_input_focused — the gate on app-level shortcuts
// ---------------------------------------------------------------------------

/// The focus gate is closed exactly while a field holds focus.
///
/// This predicate is what stands between a developer shortcut and the letter
/// the user is typing, so its two poles are the whole contract: `true` with a
/// focused field (the shortcut must be withheld) and `false` without one (the
/// shortcut must still work — `r`/`q` are the only way out of the window). A
/// predicate that returned `true` unconditionally would "fix" the crash and
/// strand the user; one that returned `false` would leave the bug in place.
#[test]
fn the_shortcut_gate_is_closed_only_while_a_field_is_focused() {
    let mut targets = page();
    assert!(
        !any_input_focused(&targets),
        "nothing is focused on a fresh page, so r and q must still reload and quit"
    );

    focus_input(&mut targets, 1);
    assert!(
        any_input_focused(&targets),
        "a focused field means r and q are text, so the shortcut must be withheld"
    );

    blur_focused_input(&mut targets);
    assert!(
        !any_input_focused(&targets),
        "blur re-opens the gate, or the user can never quit after using a field"
    );
}

/// The gate reads the flag, it does not re-derive it from an index.
///
/// `focused_input_index` reports the *first* focused target. A gate built on that
/// would be closed whenever any field is focused — correct today only by
/// accident, since `focus_input` is what keeps the two in step. Asking about the
/// flag directly means a caller cannot get the two out of step.
#[test]
fn the_gate_agrees_with_the_flag_whichever_field_is_focused() {
    for idx in 0..3 {
        let mut targets = page();
        focus_input(&mut targets, idx);
        assert_eq!(
            any_input_focused(&targets),
            focused_input_index(&targets).is_some(),
            "the gate and the focused-field lookup must never disagree (focus on {idx})"
        );
    }
}

// ---------------------------------------------------------------------------
// focus_input
// ---------------------------------------------------------------------------

/// Focusing a field moves focus to it and takes it from whoever held it.
///
/// Focus is single-valued by construction. The reason it has to be: the editor
/// sends a keystroke to `focused_input_index(..)`, i.e. to whichever focused
/// target comes *first* in the vector. If clicking from one field to another
/// left both flagged — which is exactly what the old `input_targets[idx].focused
/// = true` did, because it never cleared the other one — then typing went into
/// the field the user had just left.
#[test]
fn focusing_a_field_takes_focus_from_the_previous_one() {
    let mut targets = page();
    assert!(
        focus_input(&mut targets, 0),
        "focusing an unfocused field changes it"
    );
    assert_eq!(focused_input_index(&targets), Some(0));

    assert!(
        focus_input(&mut targets, 2),
        "moving focus to a second field is a change"
    );
    assert_eq!(
        focused_input_index(&targets),
        Some(2),
        "focus must be on the field just focused"
    );
    assert_eq!(
        targets.iter().filter(|t| t.focused).count(),
        1,
        "exactly one field may be focused, or keystrokes go to whichever the \
         vector happens to list first"
    );
}

/// Re-focusing the field that already has focus is not a change.
///
/// The loops branch on this return value to decide whether to repaint, so
/// reporting `true` for a no-op would repaint on every click that happened to
/// land on the focused field — and, in the click arm, run the whole redraw path
/// for nothing.
#[test]
fn focusing_the_already_focused_field_reports_no_change() {
    let mut targets = page();
    focus_input(&mut targets, 1);
    assert!(
        !focus_input(&mut targets, 1),
        "focus did not move, so there is nothing to repaint"
    );
    assert_eq!(focused_input_index(&targets), Some(1));
}

/// An index past the end of the page is a no-op, not a panic.
///
/// The index reaches `focus_input` from hit testing or from a caller holding an
/// index into a vector it built before the tree was rebuilt underneath it. That
/// is an ordinary event in a live app, not a bug worth taking the window down
/// for, and a panic here would be a crash reachable from a click.
#[test]
fn focusing_an_out_of_range_index_changes_nothing() {
    let mut targets = page();
    focus_input(&mut targets, 1);
    assert!(!focus_input(&mut targets, 3), "there is no fourth field");
    assert!(!focus_input(&mut targets, usize::MAX));
    assert_eq!(
        focused_input_index(&targets),
        Some(1),
        "a rejected focus request must not disturb the focus already held"
    );
}

// ---------------------------------------------------------------------------
// blur_focused_input
// ---------------------------------------------------------------------------

/// Blur drops the focus and the selection, and keeps the caret.
///
/// Keeping the caret is the deliberate difference from Esc: a click on empty
/// space is not a decision to forget where you were, so clicking back into the
/// field restores the position. Dropping the selection is not optional — a
/// highlight left on a field the user has left is a stale artefact painted from
/// state nobody is looking at any more.
#[test]
fn blur_drops_focus_and_the_selection_but_keeps_the_caret() {
    let mut targets = page();
    focus_input(&mut targets, 1);
    targets[1].cursor = 4;
    targets[1].anchor = Some(1);
    targets[1].blink_on = false;
    assert_eq!(targets[1].selection(), Some((1, 4)));

    assert!(
        blur_focused_input(&mut targets),
        "a blur is a visual change"
    );

    assert!(!targets[1].focused, "focus must be released");
    assert_eq!(
        targets[1].anchor, None,
        "a stale highlight must not survive a blur"
    );
    assert_eq!(
        targets[1].cursor, 4,
        "the caret is kept, so clicking back into the field restores the position"
    );
    assert!(
        targets[1].blink_on,
        "the caret must come back solid, not mid-blink-out"
    );
}

/// Blurring a page with nothing focused and no selection changes nothing.
///
/// The loops branch on the return value, so reporting a change here would repaint
/// on every click that landed on empty space of a page with no fields.
#[test]
fn blurring_nothing_is_not_a_change() {
    let mut targets = page();
    assert!(
        !blur_focused_input(&mut targets),
        "there was no focus and no selection to take away"
    );
}

/// Blur releases every field, not just the first one.
///
/// Cheap insurance against the two-focused state that `focused_input_index`
/// cannot represent: if it ever came back, blur still leaves the page in the
/// state the editor can handle.
#[test]
fn blur_releases_every_focused_field() {
    let mut targets = page();
    // Force the unrepresentable state rather than trusting it cannot happen.
    targets[0].focused = true;
    targets[2].focused = true;
    targets[2].anchor = Some(2);

    assert!(blur_focused_input(&mut targets));
    assert_eq!(
        targets.iter().filter(|t| t.focused).count(),
        0,
        "after a blur no field may still claim focus"
    );
    assert_eq!(focused_input_index(&targets), None);
}

// ---------------------------------------------------------------------------
// focused_input_path
// ---------------------------------------------------------------------------

/// The focused field's path is the flag read back as the index-free shape the
/// loops keep a local of.
///
/// This exists so a caller can ask "which input is focused" without rebuilding
/// the answer from an index and a slice, and so there is one implementation of
/// that answer rather than one per loop.
#[test]
fn focused_input_path_reports_the_focused_field_and_nothing_otherwise() {
    let mut targets = page();
    assert_eq!(focused_input_path(&targets), None);

    focus_input(&mut targets, 2);
    assert_eq!(
        focused_input_path(&targets),
        Some(vec![1]),
        "must be the path of the field that holds focus"
    );

    focus_input(&mut targets, 0);
    assert_eq!(
        focused_input_path(&targets),
        Some(vec![0]),
        "the path must follow focus when it moves"
    );

    blur_focused_input(&mut targets);
    assert_eq!(focused_input_path(&targets), None);
}

// ---------------------------------------------------------------------------
// EditAction::Blur — Esc
// ---------------------------------------------------------------------------

/// Esc on a focused field takes focus, selection and caret, and leaves the text.
///
/// Esc is the one key that ends an editing session rather than editing the text.
/// It has to reach the editor as an [`EditAction`] because `edit_action_for_key`
/// is a closed match with no fallthrough to user code: a key absent from it
/// simply cannot happen, which is what left Esc inert and the field inescapable.
/// The behavioural half — that Esc maps to this action, and that the loops' own
/// `focused_input` mirror follows the blur — is asserted in `lib.rs`'s
/// `edit_focus_tests`, which is where those private functions are reachable.
#[test]
fn escape_blurs_the_field_and_leaves_its_value_alone() {
    let mut target = field(0, &[0]);
    target.focused = true;
    target.cursor = 3;
    target.anchor = Some(1);
    target.blink_on = false;

    let res = apply_edit(&mut target, "hello", EditAction::Blur);

    assert!(!target.focused, "Esc must leave the field");
    assert_eq!(
        target.selection(),
        None,
        "the selection goes with the focus"
    );
    assert_eq!(target.cursor, 0, "Esc clears the caret");
    assert!(
        res.value.is_none(),
        "Esc is not an edit: the app must not be told its text changed"
    );
    assert!(!res.submit, "Esc does not submit");
    assert!(
        res.needs_repaint(),
        "the focus ring and the highlight are painted, so the loops must redraw"
    );
}

/// Esc with nothing focused is free.
///
/// Same contract as every other no-op keypress in `apply_edit`: the loops branch
/// on `needs_repaint()`, so reporting a change here would repaint on a keypress
/// that did nothing.
#[test]
fn escape_with_nothing_focused_reports_no_change() {
    let mut target = field(0, &[0]);
    let res = apply_edit(&mut target, "hello", EditAction::Blur);
    assert!(!target.focused);
    assert!(
        !res.needs_repaint(),
        "there was no focus to drop, so there is nothing to redraw"
    );
}

/// Esc leaves the value untouched — including a value with a selection over it.
///
/// A blur is not an edit and must not be routed through the app's `on:input`. If
/// it were, dismissing a field would look to the app like the user changing its
/// text, and a controlled input would fight the renderer over the value.
#[test]
fn escape_over_a_selection_does_not_change_the_value() {
    let mut target = field(0, &[0]);
    target.focused = true;
    target.cursor = 4;
    target.anchor = Some(1);

    let res = apply_edit(&mut target, "hello", EditAction::Blur);

    assert_eq!(
        res.value, None,
        "a selected field dismissed with Esc still has its text"
    );
    assert!(!target.focused);
}
