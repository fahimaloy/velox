//! Task 5.6a — characterisation tests for the renderer's two event loops.
//!
//! `run_window_vnode_skia` (the plain loop) and `run_window_vnode_skia_with_hmr`
//! (the HMR loop) each carry their own copy of the same input arms: the keyboard
//! arm, the click-focus arm, the `ReceivedCharacter` arm, the caret-tick arm and
//! the redraw arm. The bodies are near-identical, which is exactly what Task 5.6b
//! wants to delete.
//!
//! This suite exists because that deletion is not obviously safe. The touched code
//! has no direct coverage, and this repo has already shipped two silent typing
//! regressions in this exact path: branch `fix/0A-hmr-deadlock` exists because the
//! HMR keyboard arm was a bare `_ => {}` stub with no `focused_input` in scope, so
//! under HMR typing did nothing and *nothing failed*; and the HMR loop had no
//! `ReceivedCharacter` arm at all, so printable characters were dropped there.
//! Both bugs were "an arm is missing", not "an arm misbehaves" — so the property
//! worth pinning is the presence and the equivalence of the arms.
//!
//! # Why these tests read the source
//!
//! The arms live inside closures passed to `EventLoop::run`, inside private `fn`s
//! behind `#[cfg(feature = "skia-native")]`. Neither a live window nor a crate-
//! private path is available to an integration test, so the arms cannot be called
//! from here. The source can be, and this is a deliberate trade: a source pin
//! proves an arm is still *there* and still makes the *same calls*, not that it
//! behaves correctly at runtime. The behaviour behind the wiring is pinned
//! separately — by `edit_typing_contract.rs` for the editor primitive the arms
//! call, and by the `edit_focus_tests` module in `lib.rs` for the focus/edit
//! plumbing behind it.
//!
//! # Surviving the merge
//!
//! Every assertion is phrased as "each loop must have X" and is evaluated for
//! every loop found, never as "loop 1 and loop 2 must have X". When 5.6b merges
//! the two loops the assertions simply run once, and the drift comparison in
//! [`the_duplicated_arms_of_both_loops_make_the_same_calls`] becomes vacuous. The
//! one exception is [`the_renderer_has_exactly_two_event_loops`], which is
//! count-sensitive on purpose — it is the single test that would notice a loop
//! disappearing, and 5.6b must delete it in the same commit that merges.

use std::fmt;

const LIB: &str = include_str!("../src/lib.rs");

// ---------------------------------------------------------------------------
// Slicing lib.rs
// ---------------------------------------------------------------------------

/// `LIB` with the contents of every `//` comment blanked, length preserved.
///
/// Comments are blanked rather than removed so offsets still index into `LIB`.
/// Without this a needle written in a comment (`// .. apply_click_focus(..)`)
/// would look like an arm body.
fn code() -> String {
    let src = LIB.as_bytes();
    let mut out: Vec<u8> = src.to_vec();
    let mut i = 0usize;
    while i < src.len() {
        if src[i] == b'/' && i + 1 < src.len() && src[i + 1] == b'/' {
            while i < src.len() && src[i] != b'\n' {
                out[i] = b' ';
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    String::from_utf8(out).expect("blanking ASCII keeps lib.rs valid UTF-8")
}

/// Whether a line declares a top-level item. Column 0 is required: a
/// multi-line signature puts `) -> Result<..>` and `where` at column 0 inside the
/// fn's header, and the fn's own `{` too, so "nearest unindented line" is wrong.
fn is_decl(line: &str) -> bool {
    !line.starts_with(char::is_whitespace)
        && [
            "fn ",
            "pub fn ",
            "pub(crate) fn ",
            "pub(super) fn ",
            "impl",
            "struct ",
            "enum ",
            "trait ",
            "mod ",
        ]
        .iter()
        .any(|p| line.starts_with(p))
}

/// One `EventLoop::run` closure, sliced out of `lib.rs` with comments blanked.
struct LoopSource {
    name: String,
    /// 1-based line of the enclosing `fn`, for assertion messages.
    decl_line: usize,
    /// The whole fn.
    body: String,
    /// The closure body: everything after the `{` that opens `run`'s closure.
    closure: String,
}

impl fmt::Display for LoopSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (lib.rs:{})", self.name, self.decl_line)
    }
}

/// Every event loop in `lib.rs`, in source order.
///
/// Loops are found by `event_loop.run(`, searched over text with both comment and
/// string-literal *contents* blanked. The masking is not tidiness: that exact
/// string is also a string literal in the in-src
/// `both_loops_publish_their_pre_loop_frame_before_they_start` test, and an
/// unmasked search reported four loops instead of two.
///
/// The anchor is deliberately *not* the `ControlFlow::Wait` statement, even though
/// that occurs exactly twice. Anchoring on a property a test asserts would make a
/// loop that lost that property invisible rather than failing: when
/// `each_loop_parks_on_control_flow_wait_before_it_dispatches` was falsified by
/// deleting the statement from the HMR loop, the loop stopped being discovered and
/// the test passed. The anchor has to be something whose removal is a bug, not
/// something a test is looking for.
///
/// An item ends at the first column-0 line that is exactly `}`, which is where a
/// top-level fn's closing brace is; the only other column-0 lines inside either
/// loop are its signature continuation and that brace.
fn event_loops() -> Vec<LoopSource> {
    let src = code();
    let search = mask_strings(&src);
    let lines: Vec<&str> = src.split('\n').collect();
    let mut out = Vec::new();
    for (i, line) in search.split('\n').enumerate() {
        if !line.contains("event_loop.run(") {
            continue;
        }
        let decl0 = (0..=i)
            .rev()
            .find(|&j| is_decl(lines[j]))
            .expect("an event loop with no enclosing fn");
        let end0 = (decl0..lines.len())
            .find(|&j| lines[j] == "}")
            .expect("a fn with no column-0 closing brace");
        let run0 = i;
        let brace = lines[run0]
            .rfind('{')
            .expect("event_loop.run( with no closure body");
        out.push(LoopSource {
            name: fn_name(lines[decl0]),
            decl_line: decl0 + 1,
            body: lines[decl0..=end0].join("\n"),
            closure: lines[run0][brace + 1..].to_string()
                + "\n"
                + &lines[run0 + 1..=end0].join("\n"),
        });
    }
    assert!(!out.is_empty(), "no event loop found in lib.rs");
    out
}

fn fn_name(header: &str) -> String {
    match header.find("fn ") {
        Some(i) => {
            let rest = &header[i + 3..];
            let end = rest
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            rest[..end].to_string()
        }
        None => header.trim().to_string(),
    }
}

/// The body of the match arm `marker` sits in, from just past `marker` to the
/// next arm header.
///
/// Arm headers are the only lines in a loop that begin with `Event::` at the
/// match's indentation, so they bound the arm — no brace counting, which would
/// have to know about the braces inside `format!("{{\"x\":{}}}", ..)`.
fn arm_span<'a>(closure: &'a str, marker: &str) -> &'a str {
    let at = closure
        .find(marker)
        .unwrap_or_else(|| panic!("no `{marker}` in this loop"));
    let rest = &closure[at + marker.len()..];
    let mut off = 0usize;
    for line in rest.split_inclusive('\n') {
        if line.trim_start().starts_with("Event::") {
            return &rest[..off];
        }
        off += line.len();
    }
    rest
}

/// The first statement of a block, ignoring leading whitespace and comments.
fn first_statement(body: &str) -> &str {
    let mut rest = body;
    loop {
        rest = rest.trim_start();
        match rest.strip_prefix("//") {
            Some(after) => {
                let after = &after[after.find('\n').map(|i| i + 1).unwrap_or(after.len())..];
                rest = after;
            }
            None => {
                return match rest.find(';') {
                    Some(i) => &rest[..=i],
                    None => rest,
                };
            }
        }
    }
}

/// Blank the contents of string literals, length preserved.
///
/// Only needed so that text inside a message such as `"... present error"` is not
/// mistaken for code. Escaped quotes are handled; `lib.rs` contains no raw
/// strings.
fn mask_strings(s: &str) -> String {
    let b = s.as_bytes().to_vec();
    let mut out = b.clone();
    let (mut i, mut in_str, mut esc) = (0usize, false, false);
    while i < b.len() {
        let c = b[i];
        if in_str {
            if esc {
                esc = false;
            } else if c == b'\\' {
                esc = true;
            } else if c == b'"' {
                in_str = false;
            } else if c != b'\n' {
                out[i] = b'x';
            }
        } else if c == b'"' {
            in_str = true;
        }
        i += 1;
    }
    String::from_utf8(out).expect("masking ASCII keeps the text valid UTF-8")
}

/// The names called by an arm, in order, with keywords and bare `(`s dropped.
///
/// Comparing this rather than the arm's text is what makes the drift test
/// trustworthy. A text diff flags the HMR loop's private state renames
/// (`scroll_offsets` → `scroll_offsets_hmr`) as differences, which is noise that
/// a future implementer would learn to ignore; a callee diff flags only a call
/// that is present in one arm and absent in the other, which is the drift that
/// actually breaks typing.
fn callees(arm: &str) -> Vec<String> {
    const KEYWORDS: &[&str] = &[
        "if", "for", "while", "match", "return", "fn", "impl", "let", "move", "loop", "else",
        "unsafe", "mut", "in", "as", "use", "crate", "self", "Self", "where", "ref", "const",
        "struct", "enum", "trait", "type", "async", "await", "box", "dyn", "break", "continue",
    ];
    let text = mask_strings(arm);
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < b.len() {
        if !(b[i].is_ascii_alphabetic() || b[i] == b'_') {
            i += 1;
            continue;
        }
        let start = i;
        while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b':') {
            i += 1;
        }
        let raw = &text[start..i];
        let name = raw.rsplit("::").next().unwrap_or(raw);
        let mut j = i;
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        if j < b.len() && b[j] == b'(' && !KEYWORDS.contains(&name) {
            out.push(name.to_string());
        }
    }
    out
}

/// How the two loops' copies of one arm are allowed to differ.
#[derive(Clone, Copy)]
enum Drift {
    /// Must make exactly the same calls, in the same order.
    Identical,
    /// The HMR arm may make these calls, in one contiguous run, and the plain arm
    /// may not. Used for a difference that exists today and that 5.6b has to
    /// decide about — naming it here is what stops it being "cleaned up" by
    /// accident.
    HmrOnly(&'static [&'static str]),
}

/// The arms that exist in both loops, and how far they may currently have drifted.
const DUPLICATED_ARMS: &[(&str, &str, Drift)] = &[
    ("keyboard", "WindowEvent::KeyboardInput", Drift::Identical),
    (
        "received_character",
        "WindowEvent::ReceivedCharacter",
        Drift::Identical,
    ),
    // The HMR click arm republishes `last_vnode` after a click handler has
    // rebuilt it; the plain loop does not. See the test below.
    (
        "click_left_press",
        "button: MouseButton::Left",
        Drift::HmrOnly(&["Some", "clone"]),
    ),
];

/// The index at which `want` appears as a contiguous run in `seq`.
fn find_run(seq: &[String], want: &[&str]) -> Option<usize> {
    if want.is_empty() || want.len() > seq.len() {
        return None;
    }
    (0..=seq.len() - want.len()).find(|&i| (0..want.len()).all(|k| seq[i + k] == want[k]))
}

/// The `impl Drop for CaretBlinkTicker` block, wherever it currently lives.
fn caret_blink_drop_impl() -> String {
    let masked = code();
    let lines: Vec<&str> = masked.split('\n').collect();
    for (i, line) in lines.iter().enumerate() {
        if !line.contains("Drop for CaretBlinkTicker") {
            continue;
        }
        let end = (i..lines.len())
            .find(|&j| lines[j] == "}")
            .expect("the Drop impl has no column-0 closing brace");
        return lines[i..=end].join("\n");
    }
    panic!("no `impl Drop for CaretBlinkTicker` in lib.rs")
}

fn show(s: &str) -> String {
    s.lines().take(4).collect::<Vec<_>>().join(" | ")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// The count of event loops is the one thing this suite pins that 5.6b is
/// expected to invalidate, so it gets its own test and its own name rather than
/// hiding in a helper. If this goes red unexpectedly, a loop was deleted —
/// investigate before changing anything else.
#[test]
fn the_renderer_has_exactly_two_event_loops() {
    let loops = event_loops();
    let names: Vec<&str> = loops.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["run_window_vnode_skia", "run_window_vnode_skia_with_hmr"],
        "expected the plain loop and the HMR loop, got {names:?}"
    );
}

/// Each loop parks on `ControlFlow::Wait` before it looks at an event.
///
/// This is the assignment 5.6b must not change. Under `Wait`, winit wakes the
/// loop only for real input; under `Poll` it would spin the CPU at 100% while
/// idle, and `WaitUntil` would park the loop and swallow the very events the
/// arms handle. It is the first statement in the closure, so moving it later
/// means one event is dispatched under a stale control flow.
#[test]
fn each_loop_parks_on_control_flow_wait_before_it_dispatches() {
    for lp in event_loops() {
        assert_eq!(
            first_statement(&lp.closure),
            "*control_flow = ControlFlow::Wait;",
            "{lp}: the loop must park on Wait before it matches on the event"
        );
    }
    for busy in ["ControlFlow::Poll", "ControlFlow::WaitUntil"] {
        assert!(
            !LIB.contains(busy),
            "{busy} would make the loops spin or swallow events; only Exit and Wait are used"
        );
    }
}

/// Each loop binds its blink ticker to a named local that outlives `run`.
///
/// `CaretBlinkTicker`'s `Drop` is what stops and joins the clock thread, so the
/// ticker has to be held by a local, not dropped at the end of its statement.
/// `let _ = CaretBlinkTicker::start(..)` compiles, starts the thread and kills it
/// on the spot — the caret would then never blink at all. `let _blink_ticker =`
/// (leading underscore, but a real binding) is the form that keeps it alive.
#[test]
fn each_loop_holds_its_blink_ticker_in_a_local_that_outlives_the_run() {
    for lp in event_loops() {
        let at = lp
            .body
            .find("CaretBlinkTicker::start(")
            .unwrap_or_else(|| panic!("{lp} starts no CaretBlinkTicker"));
        // The binding may be split over two lines, so look for the innermost
        // `let` that starts a line rather than for the enclosing statement.
        let binding = lp.body[..at]
            .lines()
            .rev()
            .find(|l| l.trim_start().starts_with("let "))
            .unwrap_or_else(|| panic!("{lp} does not bind its blink ticker to a local"));
        let name = binding
            .trim_start()
            .strip_prefix("let ")
            .and_then(|s| s.split('=').next())
            .map(str::trim)
            .unwrap_or("");
        assert!(
            !name.is_empty()
                && name != "_"
                && name.chars().all(|c| c.is_alphanumeric() || c == '_'),
            "{lp}: the ticker must be bound to a named local (`let _blink_ticker =`), not `let _ =` \
             — `let _ =` drops it immediately and the caret never blinks. Found: {binding:?}"
        );
    }
}

/// The `Drop` that stops the clock thread signals it *before* it joins it.
///
/// `join` on a thread that has not been told to stop never returns, so the order
/// of these two statements is the whole contract. This is the cleanup pattern the
/// rest of the loop leans on: the thread outlives `run`, and this `Drop` is what
/// ends it.
#[test]
fn caret_blink_ticker_drop_signals_the_thread_before_it_joins_it() {
    let drop = caret_blink_drop_impl();
    let stop = drop
        .find("self.stop.store(true,")
        .unwrap_or_else(|| panic!("the Drop must set the stop flag: {}", show(&drop)));
    let join = drop
        .find(".join()")
        .unwrap_or_else(|| panic!("the Drop must join the thread: {}", show(&drop)));
    assert!(
        stop < join,
        "the stop flag is set after the join, which would deadlock: {}",
        show(&drop)
    );
    assert!(
        drop[stop..join].contains("self.handle.take()"),
        "the handle must be taken before the join: {}",
        show(&drop)
    );
}

/// Each loop declares the focus state its own arms mutate.
///
/// `focused_input` and `input_targets` are locals the click and keyboard arms
/// write through. This is the specific thing the HMR loop was missing when its
/// keyboard arm shipped as a bare `_ => {}` stub: the arm had no `focused_input`
/// to pass, so the edit was dropped. A loop that stops declaring the state
/// cannot compile an arm that uses it, so this guards the state rather than the
/// arm — and the arm guards are below.
#[test]
fn each_loop_declares_the_focus_state_its_arms_mutate() {
    for lp in event_loops() {
        for state in ["let mut focused_input", "let mut input_targets"] {
            assert!(
                lp.body.contains(state),
                "{lp}: the loop must declare `{state}` before its arms use it"
            );
        }
    }
}

/// Each loop's keyboard arm routes editing keys through the shared editor, and
/// repaints when the edit changed something.
///
/// This is the arm that regressed under HMR. It has to reach `edit_action_for_key`
/// and then `apply_edit_to_focused`, and when that reports a change it has to
/// re-arm the blink deadline, run the updated hooks and request a redraw. An arm
/// that is present but routes nowhere still compiles, still passes review, and
/// makes typing silently do nothing.
#[test]
fn each_loop_routes_editing_keys_through_the_shared_editor() {
    for lp in event_loops() {
        let arm = arm_span(&lp.closure, "WindowEvent::KeyboardInput");
        for needle in [
            "edit_action_for_key(keycode, shift_held)",
            "apply_edit_to_focused(",
            "arm_blink_deadline(&mut blink_deadline)",
            "run_all_updated_hooks()",
            "w.request_redraw()",
        ] {
            assert!(
                arm.contains(needle),
                "{lp}: the keyboard arm must call `{needle}`, otherwise the key is handled and \
                 nothing changes on screen"
            );
        }
        // R and Q reload and quit. They must work with no field focused — they
        // are the only way out of the window — and they must be reached before
        // the `_` arm that routes everything else to the editor.
        let r = arm
            .find("VirtualKeyCode::R")
            .unwrap_or_else(|| panic!("{lp}: the keyboard arm lost R (reload)"));
        let q = arm
            .find("VirtualKeyCode::Q")
            .unwrap_or_else(|| panic!("{lp}: the keyboard arm lost Q (quit)"));
        let editor = arm.find("edit_action_for_key").expect("checked above");
        assert!(
            r < editor && q < editor,
            "{lp}: the reload/quit arms come first"
        );
        // R and Q share one arm because they do one thing: run the destroy hooks
        // and leave the loop, where the dev server restarts the app. Asserted as
        // "the arm they share actually exits" rather than as a count of exit
        // statements, so merging them (which is what happened when the focus
        // guard below landed) is not a failure in itself — losing the exit is.
        let exits = arm.matches("*control_flow = ControlFlow::Exit;").count();
        assert_eq!(
            exits, 1,
            "{lp}: the arm carrying both R and Q must exit the loop exactly once"
        );
    }
}

/// Each loop dispatches `@keydown` from its keyboard arm, additively.
///
/// The placement is the whole contract, so it is asserted as placement rather
/// than as a presence check:
///
/// - **Before** the `match keycode`, not inside one of its arms. Inside an arm it
///   would either miss the editing keys routed to `_` or — worse, if a future
///   author put it in the R/Q arm — fire only for the two keys that quit the app.
/// - **Outside** the `match`, and returning no control signal, so it cannot
///   swallow a keystroke. The editing branch is a *sibling* of the dispatch, not
///   a fallthrough target, and a key reaches both.
///
/// `keydown_focus.rs` pins the behaviour of what runs here; this pins that both
/// loops run it, which is the drift that bit the HMR loop's keyboard arm before.
#[test]
fn each_loop_dispatches_keydown_additively_before_routing_the_key() {
    for lp in event_loops() {
        let arm = arm_span(&lp.closure, "WindowEvent::KeyboardInput");
        let dispatch = arm.find("dispatch_keydown(").unwrap_or_else(|| {
            panic!(
                "{lp}: the keyboard arm never dispatches `on:keydown`, so an authored \
                     @keydown binding compiles and then never runs"
            )
        });
        let key_match = arm
            .find("match keycode")
            .unwrap_or_else(|| panic!("{lp}: the keyboard arm lost its key match"));
        let editor = arm
            .find("edit_action_for_key")
            .expect("each_loop_routes_editing_keys_through_the_shared_editor covers this");
        assert!(
            dispatch < key_match,
            "{lp}: the @keydown dispatch must sit before `match keycode`, or it would run only \
             for whichever keys that match names"
        );
        assert!(
            dispatch < editor,
            "{lp}: the @keydown dispatch must sit before the editing branch, or the two would \
             have to be ordered by hand for every key"
        );
        // The dispatch takes the same focus state the editing branch does, so a
        // grant made here is visible to the edit that follows it in the same
        // press — the ordering above is only meaningful if they share state.
        for needle in ["&mut input_targets", "&mut focused_input", "&mut on_event"] {
            let after = &arm[dispatch..];
            assert!(
                after.contains(needle),
                "{lp}: the @keydown dispatch must be handed `{needle}`"
            );
        }
    }
}

/// Neither loop may fire the reload/quit shortcut while a text field is focused.
///
/// This is the bug that made typing "r" into a field kill the application: the
/// arms were gated only on `input.state == ElementState::Pressed`, so a printable
/// key and a developer shortcut were indistinguishable. With nothing focused the
/// keys are still the only way out of the window, so the gate cannot be "always
/// off" either — it has to be a question about focus.
///
/// The gate itself is pinned behaviourally in `focus_blur_api.rs`
/// (`any_input_focused`); this pins that both loops actually ask it, in the arm,
/// before the exit — and that the key falls through to the editor rather than
/// being swallowed when the gate is closed.
#[test]
fn each_loop_withholds_reload_and_quit_while_a_text_field_has_focus() {
    for lp in event_loops() {
        let arm = arm_span(&lp.closure, "WindowEvent::KeyboardInput");
        let guard = arm
            .find("any_input_focused(&input_targets)")
            .unwrap_or_else(|| {
                panic!(
                    "{lp}: the reload/quit arm is not gated on focus, so typing \"r\" into a \
                     text field exits the app"
                )
            });
        let exit = arm
            .find("*control_flow = ControlFlow::Exit;")
            .unwrap_or_else(|| panic!("{lp}: the reload/quit arm never exits"));
        assert!(
            guard < exit,
            "{lp}: the focus guard must be evaluated before the loop exits"
        );
        // With the gate closed the key has to reach the editor, or the character
        // is swallowed instead of typed: a match guard falls through to `_`, an
        // `if` inside the arm body does not.
        assert!(
            arm.contains("edit_action_for_key(keycode, shift_held)"),
            "{lp}: a withheld reload key must still fall through to the editor"
        );
    }
}

/// Each loop's `ReceivedCharacter` arm inserts printable characters into the
/// focused field, and skips the ones the keyboard arm already handled.
///
/// The guard is what stops Enter, Backspace and Delete arriving twice — once as
/// the key the keyboard arm acted on and again as a literal character here. The
/// HMR loop is missing this arm altogether is a bug this file exists to keep from
/// coming back.
#[test]
fn each_loop_applies_received_characters_to_the_focused_field() {
    for lp in event_loops() {
        let arm = arm_span(&lp.closure, "WindowEvent::ReceivedCharacter");
        for needle in [
            "!c.is_control()",
            "c != '\\u{7f}'",
            "EditAction::Insert(c)",
            "apply_edit_to_focused(",
            "arm_blink_deadline(&mut blink_deadline)",
            "w.request_redraw()",
        ] {
            assert!(
                arm.contains(needle),
                "{}: the ReceivedCharacter arm must call `{needle}`",
                lp.name
            );
        }
    }
}

/// Each loop focuses the clicked field before it dispatches the click, and passes
/// its own focus state in.
///
/// `apply_click_focus` is the only thing that sets `focused` and places the caret,
/// and it writes through a `&mut` the loop owns. If the argument is dropped the
/// call stops compiling; if the *call* is dropped from the arm, or the loop stops
/// declaring the state, the loop still compiles and the caret never appears. The
/// focus call also has to come first: it reads `last_vnode` for the field's value,
/// and the click handler is what rebuilds that vnode.
#[test]
fn each_loop_focuses_the_clicked_field_before_dispatching_the_click() {
    for lp in event_loops() {
        let arm = arm_span(&lp.closure, "button: MouseButton::Left");
        let call = arm
            .find("apply_click_focus(")
            .unwrap_or_else(|| panic!("{}: the click arm never calls apply_click_focus", lp.name));
        let args = &arm[call..];
        let end = args
            .find(");")
            .expect("unterminated apply_click_focus call");
        assert!(
            args[..end].contains("&mut focused_input"),
            "{}: apply_click_focus is not given the loop's focus state, so focusing cannot stick",
            lp.name
        );
        let dispatch = arm
            .find("hit_test_click")
            .unwrap_or_else(|| panic!("{}: the click arm dispatches to no handler", lp.name));
        assert!(
            call < dispatch,
            "{}: focus must be resolved before the click handler rebuilds the vnode",
            lp.name
        );
    }
}

/// The arms that exist in both loops must make the same calls, in the same order.
///
/// This is what makes the duplication a known quantity instead of a hope. If one
/// arm gains or loses a call, this goes red and prints both sequences. After the
/// 5.6b merge there is one copy of each and the comparison is vacuous.
///
/// Compared by *calls* rather than by text, because a text diff flags the HMR
/// loop's private state renames (`scroll_offsets` → `scroll_offsets_hmr`,
/// `last_layout` → `last_layout_hmr`) as differences, and noise an implementer
/// learns to ignore is worse than no test. Calls are the thing that decides what
/// typing does.
///
/// The one difference that is real is allowed explicitly: the HMR click arm
/// republishes `last_vnode` after a click handler has rebuilt it, and the plain
/// loop does not. Whether the plain loop *should* is 5.6b's call — until it is
/// made, this test makes sure that republish is the only thing the two click
/// arms disagree about, so a second divergence cannot slip in beside it.
#[test]
fn the_duplicated_arms_of_both_loops_make_the_same_calls() {
    let loops = event_loops();
    if loops.len() < 2 {
        return; // merged: there is nothing to compare
    }
    for (what, marker, drift) in DUPLICATED_ARMS {
        let plain = callees(arm_span(&loops[0].closure, marker));
        let hmr = callees(arm_span(&loops[1].closure, marker));
        let extra = match drift {
            Drift::Identical => {
                assert_eq!(
                    plain, hmr,
                    "the {what} arms have drifted: they must be one shared arm (5.6b)\n\
                     plain: {plain:?}\n  hmr: {hmr:?}"
                );
                continue;
            }
            Drift::HmrOnly(extra) => extra,
        };
        let start = find_run(&hmr, extra).unwrap_or_else(|| {
            panic!(
                "the {what} arms drifted and the HMR arm no longer contains its one known extra \
                 calls {extra:?}\nplain: {plain:?}\n  hmr: {hmr:?}"
            )
        });
        let mut without = hmr.clone();
        without.drain(start..start + extra.len());
        assert_eq!(
            plain, without,
            "the {what} arms have drifted beyond the one known difference\n\
             plain: {plain:?}\n  hmr: {hmr:?}"
        );
    }
}

/// Each loop republishes and presents the frame it paints on `RedrawRequested`.
///
/// Two things in one arm. `last_vnode = Some(..)` is what makes the painted frame
/// visible to the input arms — `apply_click_focus` and `apply_edit_to_focused` both
/// read `last_vnode` to find the field's current value — and `presenter.present`
/// is what puts pixels on screen. Lose the first and input is handled against an
/// empty value; lose the second and the window never updates.
#[test]
fn each_loop_republishes_and_presents_on_redraw() {
    for lp in event_loops() {
        assert_eq!(
            lp.closure.matches("Event::RedrawRequested").count(),
            1,
            "{}: expected exactly one RedrawRequested arm",
            lp.name
        );
        let arm = arm_span(&lp.closure, "Event::RedrawRequested");
        for needle in ["last_vnode = Some(", "presenter.present(s)"] {
            assert!(
                arm.contains(needle),
                "{}: the redraw arm must call `{needle}`",
                lp.name
            );
        }
    }
}

/// Each loop repaints when the caret blink phase actually flips.
///
/// The tick only reaches this arm if the `CaretBlinkTicker` thread survived, which
/// is why the ticker is held in a local and why its `Drop` exists. The arm's job
/// is to ask for a redraw when `on_caret_blink_tick` says the phase flipped;
/// without it the caret would freeze at whatever phase it had when the window was
/// created.
#[test]
fn each_loop_repaints_when_the_caret_blink_phase_flips() {
    for lp in event_loops() {
        let arm = arm_span(&lp.closure, "on_caret_blink_tick(");
        for needle in [
            "&mut input_targets",
            "&mut blink_deadline",
            "w.request_redraw()",
        ] {
            assert!(
                arm.contains(needle),
                "{}: the caret-tick arm must call `{needle}`",
                lp.name
            );
        }
    }
}
