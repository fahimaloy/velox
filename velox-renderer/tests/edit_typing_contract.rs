//! Task 5.6a — the editing contract the two event loops' text-input arms depend on.
//!
//! The keyboard and `ReceivedCharacter` arms in `run_window_vnode_skia` and
//! `run_window_vnode_skia_with_hmr` both funnel every edit through
//! `apply_edit_to_focused` → `apply_edit`. The arms themselves are unreachable
//! from an integration test (see `event_loop_arms.rs` for why, and for the
//! structural pins that stand in for them), so this file pins the primitive they
//! call, over the public `velox_renderer::events` API.
//!
//! The point of testing the primitive here rather than only in `lib.rs` is the
//! feature gate: `apply_edit` is not behind `skia-native`, so these tests run in
//! the plain `cargo test -p velox-renderer` build, where the `edit_focus_tests`
//! module in `lib.rs` is compiled out. Without them, "typing updates the value
//! and the caret" would have no coverage in the default build at all.
//!
//! What the arms ask of `apply_edit` is precisely this:
//!
//! * an `Insert` reports the new value and leaves the caret one char further on,
//!   so the next character lands after the one just typed;
//! * a `Backspace` reports the new value and leaves the caret one char earlier;
//! * a caret move reports `moved` with no `value`, so the loops repaint without
//!   telling the app its text changed;
//! * a key that changes nothing reports neither, so the loops skip a redraw and
//!   the app is not woken for a keystroke that did nothing.
//!
//! `needs_repaint()` is the single value both loops branch on after every edit
//! (`if changed { .. w.request_redraw() }`), so its exact meaning is the
//! contract, not an implementation detail.

use velox_renderer::events::{EditAction, InputTarget, StackCtx, apply_edit, focused_input_index};

/// A focused text field, caret `cursor` chars in, caret not yet blinking.
fn field(cursor: usize) -> InputTarget {
    InputTarget {
        rect: velox_dom::layout::Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 20,
        },
        path: vec![0],
        z_index: 0,
        order: 0,
        clip: None,
        sc: StackCtx::ROOT,
        focused: true,
        cursor,
        anchor: None,
        blink_on: false,
    }
}

/// Typing a character updates the value and puts the caret after it.
///
/// The single most basic thing the text-input arms exist to do, and the thing
/// that broke under HMR: the arm was missing, so nothing called this.
#[test]
fn typing_into_a_focused_input_updates_the_value_and_the_caret() {
    let mut target = field(4);
    let res = apply_edit(&mut target, "hell", EditAction::Insert('o'));
    assert_eq!(
        res.value.as_deref(),
        Some("hello"),
        "the value must gain the char"
    );
    assert_eq!(
        target.cursor, 5,
        "the caret must follow the char just typed"
    );
    assert!(
        res.needs_repaint(),
        "a changed value must schedule a repaint"
    );
    assert!(
        target.blink_on,
        "typing must make the caret solid, not blinking"
    );
}

/// A character is inserted at the caret, not appended to the value.
///
/// Without this the field would read backwards as you edit into the middle of
/// it, and the two loops would still look correct on an append-only test.
#[test]
fn a_character_lands_at_the_caret_not_at_the_end() {
    let mut target = field(0);
    let res = apply_edit(&mut target, "helo", EditAction::Insert('X'));
    assert_eq!(res.value.as_deref(), Some("Xhelo"));
    assert_eq!(target.cursor, 1);
    assert!(res.needs_repaint());
}

/// Backspace removes the character before the caret and backs the caret up.
#[test]
fn backspace_removes_the_char_before_the_caret() {
    let mut target = field(5);
    let res = apply_edit(&mut target, "hello", EditAction::Backspace);
    assert_eq!(res.value.as_deref(), Some("hell"));
    assert_eq!(
        target.cursor, 4,
        "the caret must move back with the deletion"
    );
    assert!(res.needs_repaint());
}

/// Backspace at the caret position zero changes nothing at all.
///
/// The arms call `apply_edit` on every `Backspace` keypress, with no field check
/// of their own beyond `apply_edit_to_focused`'s. Reporting a change here would
/// dispatch an `on:input` event to the app with an unchanged value and schedule a
/// repaint for a keypress that did nothing.
#[test]
fn backspace_at_the_start_of_the_value_changes_nothing() {
    let mut target = field(0);
    let res = apply_edit(&mut target, "hello", EditAction::Backspace);
    assert_eq!(
        res.value, None,
        "no text changed, so no value may be reported"
    );
    assert!(!res.moved, "the caret did not move");
    assert!(
        !res.needs_repaint(),
        "a no-op keypress must not schedule a repaint or wake the app"
    );
}

/// A caret move repaints but reports no new value.
///
/// `needs_repaint` is true (the loops must redraw or the caret bar does not move)
/// while `value` is `None` (the app's `on:input` must not be told the text
/// changed when only the caret did).
#[test]
fn a_caret_move_repaints_without_reporting_a_text_change() {
    let mut target = field(3);
    let res = apply_edit(&mut target, "hello", EditAction::MoveLeft { shift: false });
    assert_eq!(res.value, None, "a caret move does not change the text");
    assert!(res.moved, "the caret moved");
    assert_eq!(target.cursor, 2);
    assert!(
        res.needs_repaint(),
        "the caret bar is painted from the target, so the loop must redraw"
    );
}

/// Pressing an arrow key at the end of the value is a no-op.
///
/// The other half of the "no-op keypress" contract, and the one that is easy to
/// get wrong in a rewrite: `MoveRight` at the end must not report a move, or
/// every arrow press at the end of a field dispatches a bogus `on:input`.
#[test]
fn move_right_at_the_end_of_the_value_changes_nothing() {
    let mut target = field(5);
    let res = apply_edit(&mut target, "hello", EditAction::MoveRight { shift: false });
    assert_eq!(res.value, None);
    assert!(!res.moved, "there is nowhere further to go");
    assert!(!res.needs_repaint());
    assert_eq!(target.cursor, 5, "the caret must not run past the value");
}

/// `focused_input_index` is how both loops find the field to type into, and how
/// they decide where to inject the caret attrs before a repaint.
///
/// It must be the index of the *focused* target, not the first target, and it
/// must report `None` when nothing is focused. A loop that reads `None` for a
/// focused field types into nothing; a loop that reads `Some(0)` for an unfocused
/// field paints a caret in the wrong place. The arms pass this straight into
/// `inject_input_caret_attrs` and `apply_edit_to_focused`.
#[test]
fn focused_input_index_reports_the_focused_field_and_nothing_otherwise() {
    let mut first = field(3);
    first.focused = false;
    let mut second = field(0);
    second.focused = false;
    second.order = 1;
    let mut third = field(0);
    third.focused = false;
    third.order = 2;

    assert_eq!(
        focused_input_index(&[first.clone(), second.clone(), third.clone()]),
        None,
        "with nothing focused there is no caret to draw and nothing to type into"
    );

    second.focused = true;
    assert_eq!(
        focused_input_index(&[first.clone(), second.clone(), third.clone()]),
        Some(1),
        "must be the focused field's own index, not the first field's"
    );

    second.focused = false;
    third.focused = true;
    assert_eq!(
        focused_input_index(&[first, second, third.clone()]),
        Some(2),
        "focus must be reported wherever it moved to"
    );
}

/// A caret index past the end of the value is clamped, not trusted.
///
/// The caret arrives from the click path (a click-to-character index computed
/// from measured glyph widths) and from the previous frame's value. If those
/// disagree, a stale caret must degrade to a clamped one — a caret past the end
/// would index the value out of range when the next character is inserted, which
/// is a panic in the loop rather than a wrong pixel.
#[test]
fn a_caret_past_the_end_of_the_value_is_clamped() {
    let mut target = field(99);
    let res = apply_edit(&mut target, "hi", EditAction::Insert('!'));
    assert_eq!(
        res.value.as_deref(),
        Some("hi!"),
        "the insert must land at the clamped end, not panic or wrap"
    );
    assert_eq!(target.cursor, 3);
}

/// Selecting text and typing replaces the selection rather than appending.
///
/// Both loops can produce a selection — shift-arrow, or a drag — and the arms
/// pass the action straight through. If the selection were not cleared first,
/// shift-typing in the middle of a field would insert instead of replacing.
#[test]
fn typing_over_a_selection_replaces_it() {
    let mut target = field(4);
    target.anchor = Some(1);
    assert_eq!(target.selection(), Some((1, 4)));
    let res = apply_edit(&mut target, "hello", EditAction::Insert('X'));
    assert_eq!(
        res.value.as_deref(),
        Some("hXo"),
        "the selection must be replaced"
    );
    assert_eq!(
        target.cursor, 2,
        "the caret sits just after the inserted char"
    );
    assert_eq!(target.anchor, None, "a replacement collapses the selection");
}

/// Submit reports the submit flag and the untouched value.
///
/// `Enter` is the one editing key that must not insert a newline character, and
/// it is handled by the keyboard arm rather than the `ReceivedCharacter` arm —
/// which only inserts when the character is not a control character. The arms
/// dispatch the value on submit, so the value must be reported as-is.
#[test]
fn submit_reports_the_submit_flag_and_the_unchanged_value() {
    let mut target = field(5);
    let res = apply_edit(&mut target, "hello", EditAction::Submit);
    assert!(res.submit, "Enter must be reported as a submit");
    assert_eq!(res.value, None, "submitting does not change the text");
    assert!(res.needs_repaint());
}
