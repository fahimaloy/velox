//! `@keydown` dispatch and stable-name focus resolution.
//!
//! Three things are pinned here, and they are the three that could each break
//! independently:
//!
//! 1. **A key reaches the app by name.** `plan_keydown` finds `on:keydown`
//!    bindings and `apply_keydown` calls them with a stable string, not a
//!    winit enum. Pure tree logic — no window, no backend.
//! 2. **Focus is nameable and stable.** An author says which field a key
//!    focuses with `data-focus-id`, and that name keeps pointing at the same
//!    field when inputs above it are added, removed or reordered. The test that
//!    matters here is the one that *reorders* the list, because a numeric index
//!    passes every other test in this file.
//! 3. **Dispatch is additive.** A `@keydown` binding is not allowed to eat a
//!    keystroke: A–Z, digits, the arrows, Backspace and Enter must still reach
//!    the editor on the same press. `event_loop_arms.rs` pins the *loop*
//!    placement that guarantees it; the tests here pin that the dispatch
//!    function itself never edits, never blurs and never consumes.
//!
//! `VirtualKeyCode -> &str` needs winit, so the key-name table and the
//! winit-facing `dispatch_keydown` are behind `skia-native`. Everything else is
//! backend-free and runs in both configurations.

use velox_dom::layout::compute_layout;
use velox_dom::{VNode, h};
use velox_renderer::events::{
    EditAction, StackCtx, apply_edit, apply_keydown, collect_input_targets, find_focus_id_path,
    focus_input, focused_input_index, input_index_by_path, plan_keydown,
};

/// A text input, laid out in flow so it is definitely collected as a target.
fn text_input(id: &str, focus_on: &str, value: &str) -> VNode {
    let mut v = h(
        "input",
        vec![
            ("type", "text"),
            ("value", value),
            ("style", "width:200px;height:24px;"),
        ],
        vec![],
    );
    if !id.is_empty() {
        v = with_attr(v, "data-focus-id", id);
    }
    if !focus_on.is_empty() {
        v = with_attr(v, "data-focus-on", focus_on);
    }
    v
}

/// The app-level `@keydown` binding: a plain element carrying the handler.
fn keydown_host(handler: &str, child: VNode) -> VNode {
    with_attr(h("div", vec![], vec![child]), "on:keydown", handler)
}

fn with_attr(mut v: VNode, key: &str, value: &str) -> VNode {
    if let VNode::Element { props, .. } = &mut v {
        props.attrs.insert(key.to_string(), value.to_string());
    }
    v
}

fn collect(vnode: &VNode) -> Vec<velox_renderer::events::InputTarget> {
    let layout = compute_layout(vnode, 800, 600);
    let mut targets = Vec::new();
    let mut order = 0;
    let mut path = Vec::new();
    collect_input_targets(
        vnode,
        &layout,
        None,
        StackCtx::ROOT,
        &mut path,
        &mut order,
        &mut targets,
    );
    targets
}

/// Char length of the value at `path`, the resolver the loop threads into
/// `apply_keydown` so a freshly focused field can put its caret at the end.
fn value_len(vnode: &VNode) -> impl Fn(&[usize]) -> usize {
    let v = vnode.clone();
    move |p: &[usize]| {
        input_value_at_path(&v, p)
            .map(|s| s.chars().count())
            .unwrap_or(0)
    }
}

/// The `value` attribute of the element at `path`, the same read the loop does.
fn input_value_at_path(vnode: &VNode, path: &[usize]) -> Option<String> {
    let mut cur = vnode;
    for &i in path {
        match cur {
            VNode::Element { children, .. } => cur = children.get(i)?,
            _ => return None,
        }
    }
    match cur {
        VNode::Element { props, .. } => Some(props.attrs.get("value").cloned().unwrap_or_default()),
        _ => None,
    }
}

/// The canonical app under test: a keydown binding, a field the key focuses,
/// and a field it must not.
fn app(extra_above: Option<VNode>) -> VNode {
    let composer = text_input("composer", "F2", "draft");
    let mut children = vec![keydown_host("on_key", composer)];
    if let Some(extra) = extra_above {
        children.insert(0, extra);
    }
    h("div", vec![], children)
}

// ---------------------------------------------------------------------------
// 1. The key reaches the app by name
// ---------------------------------------------------------------------------

#[test]
fn a_keydown_binding_is_found_and_called_with_the_key_name() {
    let vnode = app(None);
    let plan = plan_keydown(&vnode, "F2");
    assert_eq!(plan.handlers, vec!["on_key".to_string()]);

    let mut seen: Vec<(String, String)> = Vec::new();
    let mut targets = collect(&vnode);
    let mut focused: Option<Vec<usize>> = None;
    let changed = apply_keydown(
        &plan,
        "F2",
        &mut targets,
        &mut focused,
        &value_len(&vnode),
        &mut |name, payload| seen.push((name.to_string(), payload.unwrap_or_default().to_string())),
    );

    assert!(changed, "focus moved, so the caller must repaint");
    assert_eq!(
        seen,
        vec![("on_key".to_string(), "F2".to_string())],
        "the handler must receive the key name as its payload"
    );
}

#[test]
fn every_keydown_binding_in_the_tree_is_invoked_in_document_order() {
    let inner = with_attr(h("span", vec![], vec![]), "on:keydown", "inner");
    let outer = keydown_host("outer", inner);
    let vnode = h("div", vec![], vec![outer]);
    let plan = plan_keydown(&vnode, "Enter");
    assert_eq!(
        plan.handlers,
        vec!["outer".to_string(), "inner".to_string()]
    );

    let mut seen: Vec<String> = Vec::new();
    let mut targets = collect(&vnode);
    let mut focused = None;
    apply_keydown(
        &plan,
        "Enter",
        &mut targets,
        &mut focused,
        &|_| 0,
        &mut |name, _| seen.push(name.to_string()),
    );
    assert_eq!(seen, vec!["outer".to_string(), "inner".to_string()]);
}

/// A tree with no `@keydown` and no focus grant: the case every app had before
/// this feature, which must stay free.
fn inert() -> VNode {
    h("div", vec![], vec![text_input("plain", "", "x")])
}

#[test]
fn a_key_with_no_binding_changes_nothing() {
    let vnode = inert();
    let plan = plan_keydown(&vnode, "F5");
    assert!(plan.handlers.is_empty() && plan.focus_paths.is_empty());

    let mut targets = collect(&vnode);
    let mut focused = None;
    let mut called = 0;
    let changed = apply_keydown(
        &plan,
        "F5",
        &mut targets,
        &mut focused,
        &value_len(&vnode),
        &mut |_, _| called += 1,
    );
    assert!(!changed, "nothing happened, so nothing to repaint");
    assert_eq!(called, 0);
    assert!(focused.is_none());
}

#[test]
fn focus_on_matching_ignores_attribute_case() {
    let vnode = h("div", vec![], vec![text_input("composer", "f2", "")]);
    assert_eq!(plan_keydown(&vnode, "F2").focus_paths.len(), 1);
}

#[test]
fn a_keydown_binding_with_no_handler_name_is_ignored() {
    let vnode = h(
        "div",
        vec![],
        vec![with_attr(h("span", vec![], vec![]), "on:keydown", "")],
    );
    assert!(plan_keydown(&vnode, "F2").handlers.is_empty());
}

// ---------------------------------------------------------------------------
// 2. The name is stable
// ---------------------------------------------------------------------------

/// The test that a numeric index cannot pass.
///
/// A field is inserted *above* the composer, which moves the composer's position
/// in the target vector by one and gives it a different structural path. The id
/// must still resolve to the composer's own path, so focus still lands on the
/// composer. A design that handed the author "focus input 0" would focus the
/// newly inserted field here.
#[test]
fn the_focus_id_still_resolves_after_a_field_is_inserted_above_it() {
    let before = app(None);
    let after = app(Some(text_input("search", "", "")));

    let before_targets = collect(&before);
    let after_targets = collect(&after);
    let path_before = find_focus_id_path(&before, &[], "composer").expect("id is declared");
    let path_after = find_focus_id_path(&after, &[], "composer").expect("id survives");

    assert_ne!(
        path_before, path_after,
        "the structural path must actually have moved, or this test proves nothing"
    );
    let idx_before = input_index_by_path(&before_targets, &path_before).unwrap();
    let idx_after = input_index_by_path(&after_targets, &path_after).unwrap();
    assert_ne!(
        idx_before, idx_after,
        "and the target index must have moved with it — that is exactly the \
         position an author cannot see or keep"
    );

    // Both documents now focus the same *named* field.
    for (vnode, targets, path) in [
        (&before, &before_targets, &path_before),
        (&after, &after_targets, &path_after),
    ] {
        let plan = plan_keydown(vnode, "F2");
        assert_eq!(plan.focus_paths, vec![path.clone()]);
        let mut targets = targets.clone();
        let mut focused = None;
        let changed = apply_keydown(
            &plan,
            "F2",
            &mut targets,
            &mut focused,
            &value_len(vnode),
            &mut |_, _| {},
        );
        assert!(changed);
        assert_eq!(
            focused_input_index(&targets),
            input_index_by_path(&targets, path)
        );
        assert_eq!(focused.as_deref(), Some(path.as_slice()));
    }

    // And the position a numeric index would have named now belongs to a
    // different field — so "focus input 0" is not a weaker answer to this test,
    // it is a wrong one.
    assert_eq!(idx_before, 0, "the composer was the first target before");
    assert_eq!(idx_after, 1, "and the second target after");
    let composer_value = input_value_at_path(&after, &after_targets[idx_after].path);
    let stale_value = input_value_at_path(&after, &after_targets[idx_before].path);
    assert_eq!(composer_value.as_deref(), Some("draft"));
    assert_eq!(
        stale_value.as_deref(),
        Some(""),
        "target 0 is the newly inserted field, not the composer"
    );

    // So the id, not the index, is what picks the field: focusing by index 0 in
    // the reordered document lands on the wrong one, and focusing by id does not.
    let mut by_index = after_targets.clone();
    focus_input(&mut by_index, idx_before);
    assert_eq!(
        input_value_at_path(
            &after,
            &by_index[focused_input_index(&by_index).unwrap()].path
        ),
        stale_value,
        "a bare numeric index lands on the wrong field after a reorder"
    );
}

#[test]
fn a_duplicate_focus_id_resolves_to_the_first_in_document_order() {
    let vnode = h(
        "div",
        vec![],
        vec![
            text_input("dup", "F2", "first"),
            text_input("dup", "", "second"),
        ],
    );
    let path = find_focus_id_path(&vnode, &[], "dup").expect("declared");
    let targets = collect(&vnode);
    let idx = input_index_by_path(&targets, &path).unwrap();
    let value = input_value_at_path(&vnode, &path);
    // The *second* input is the one with no `data-focus-on`, so focusing `dup`
    // must land on the first: child index 0.
    assert_eq!(path, vec![0]);
    assert_eq!(value.as_deref(), Some("first"));
    assert_eq!(idx, 0);
}

#[test]
fn an_unknown_focus_id_resolves_to_nothing() {
    let vnode = app(None);
    assert!(find_focus_id_path(&vnode, &[], "nope").is_none());
    assert!(
        input_index_by_path(&collect(&vnode), &[7, 7]).is_none(),
        "a path with no target must be None, not a panic"
    );
}

#[test]
fn focusing_moves_the_caret_to_the_end_of_the_value() {
    let vnode = app(None);
    let plan = plan_keydown(&vnode, "F2");
    let mut targets = collect(&vnode);
    let mut focused = None;
    apply_keydown(
        &plan,
        "F2",
        &mut targets,
        &mut focused,
        &value_len(&vnode),
        &mut |_, _| {},
    );
    let idx = focused_input_index(&targets).unwrap();
    assert_eq!(targets[idx].cursor, "draft".chars().count());
    assert!(targets[idx].anchor.is_none());
    assert!(targets[idx].blink_on);
}

#[test]
fn pressing_the_same_key_again_does_not_move_the_caret() {
    // No handler alongside, so the only thing that can report a change is focus.
    let vnode = h("div", vec![], vec![text_input("composer", "F2", "draft")]);
    let plan = plan_keydown(&vnode, "F2");
    assert!(plan.handlers.is_empty());
    let mut targets = collect(&vnode);
    let mut focused = None;
    apply_keydown(
        &plan,
        "F2",
        &mut targets,
        &mut focused,
        &value_len(&vnode),
        &mut |_, _| {},
    );
    let idx = focused_input_index(&targets).unwrap();
    // The user arrows back into the middle of what they have typed.
    targets[idx].cursor = 2;
    let caret = targets[idx].cursor;
    assert!(caret < "draft".chars().count());

    let changed = apply_keydown(
        &plan,
        "F2",
        &mut targets,
        &mut focused,
        &value_len(&vnode),
        &mut |_, _| {},
    );
    assert!(!changed, "nothing moved, so nothing to repaint");
    assert_eq!(targets[idx].cursor, caret);
}

/// A binding fires whatever the focus state is.
///
/// The asymmetry is deliberate and worth stating: `@keydown` is a global
/// observation (a key press has no pointer, so there is nothing to hit test),
/// whereas `data-focus-on` is a focus rule (it only means something once a field
/// is in the tree). So a host binding hears every key — including the ones typing
/// into a focused field produces — while a key with no binding attached changes
/// nothing at all. Both halves of that are asserted here, in the state that is
/// easy to get wrong: a field IS focused, and the key is an ordinary letter.
#[test]
fn a_binding_fires_while_a_field_is_focused_and_an_unbound_key_does_not() {
    let vnode = app(None);
    let mut targets = collect(&vnode);
    let mut focused = None;
    let mut seen: Vec<String> = Vec::new();

    // F2 takes focus, and the host hears it.
    apply_keydown(
        &plan_keydown(&vnode, "F2"),
        "F2",
        &mut targets,
        &mut focused,
        &value_len(&vnode),
        &mut |name, _| seen.push(name.to_string()),
    );
    assert!(focused_input_index(&targets).is_some());
    assert_eq!(seen, vec!["on_key"]);

    // A plain letter while that field is focused: still the app's key, so the
    // host still hears it. A field being focused is not a reason to stop
    // listening — that would make the binding useless for exactly the shortcuts
    // an app wants (submit on Enter, cancel on Escape).
    seen.clear();
    assert!(apply_keydown(
        &plan_keydown(&vnode, "K"),
        "K",
        &mut targets,
        &mut focused,
        &value_len(&vnode),
        &mut |name, _| seen.push(name.to_string()),
    ));
    assert_eq!(seen, vec!["on_key"]);
    assert!(
        focused_input_index(&targets).is_some(),
        "an ordinary key must not drop focus either"
    );

    // And the reverse: no binding means no observer, even for a key the editor
    // cares about. `Escape` here is the case — the app asked to hear nothing, so
    // nothing is dispatched, and only the editor below gets a say.
    seen.clear();
    let mut silent = h("div", vec![], vec![text_input("plain", "", "x")]);
    let _ = &mut silent;
    let plan = plan_keydown(&silent, "Escape");
    assert!(plan.handlers.is_empty());
    let mut silent_targets = collect(&silent);
    let mut silent_focused = None;
    let _ = apply_keydown(
        &plan,
        "Escape",
        &mut silent_targets,
        &mut silent_focused,
        &value_len(&silent),
        &mut |_, _| panic!("a tree with no @keydown has no observer"),
    );
    assert!(seen.is_empty());
}

/// Two fields both claiming `F2` is a template bug, but the runtime still has to
/// resolve it to one focused field rather than two — two flagged targets would
/// send every keystroke to whichever happened to be first in the vector.
#[test]
fn two_grants_of_the_same_key_still_leave_exactly_one_field_focused() {
    let vnode = h(
        "div",
        vec![],
        vec![text_input("a", "F2", "one"), text_input("b", "F2", "two")],
    );
    let plan = plan_keydown(&vnode, "F2");
    assert_eq!(plan.focus_paths, vec![vec![0], vec![1]]);
    let mut targets = collect(&vnode);
    let mut focused = None;
    apply_keydown(
        &plan,
        "F2",
        &mut targets,
        &mut focused,
        &value_len(&vnode),
        &mut |_, _| {},
    );
    // Grants apply in document order, so the later field — the one painted on
    // top — holds focus, and the earlier one is cleared.
    assert_eq!(focused_input_index(&targets), Some(1));
    assert_eq!(focused.as_deref(), Some([1].as_slice()));
    assert_eq!(targets.iter().filter(|t| t.focused).count(), 1);
}

#[test]
fn a_focus_grant_for_something_that_is_not_a_text_input_does_nothing() {
    // `data-focus-on` on a div: there is no input target to grant focus to, and
    // that must be a quiet no-op rather than a panic.
    let vnode = h(
        "div",
        vec![],
        vec![with_attr(h("div", vec![], vec![]), "data-focus-on", "F2")],
    );
    let plan = plan_keydown(&vnode, "F2");
    assert_eq!(plan.focus_paths, vec![vec![0]]);
    let mut targets = collect(&vnode);
    let mut focused = None;
    let mut called = 0;
    let changed = apply_keydown(
        &plan,
        "F2",
        &mut targets,
        &mut focused,
        &value_len(&vnode),
        &mut |_, _| called += 1,
    );
    assert!(!changed, "no focus moved and no handler ran");
    assert!(focused.is_none());
    assert_eq!(called, 0);
}

/// The two halves of a plan are independent: a grant that finds nothing does not
/// stop the handler on the same press from running.
#[test]
fn an_unresolvable_grant_does_not_stop_the_handler_on_the_same_press() {
    let vnode = keydown_host(
        "on_key",
        with_attr(h("div", vec![], vec![]), "data-focus-on", "F2"),
    );
    let plan = plan_keydown(&vnode, "F2");
    assert_eq!(plan.focus_paths, vec![vec![0]]);
    let mut targets = collect(&vnode);
    let mut focused = None;
    let mut called = 0;
    apply_keydown(
        &plan,
        "F2",
        &mut targets,
        &mut focused,
        &value_len(&vnode),
        &mut |_, _| called += 1,
    );
    assert!(focused.is_none());
    assert_eq!(
        called, 1,
        "the handler still runs — the grant and the call are independent"
    );
}

// ---------------------------------------------------------------------------
// 3. Additivity: a keydown binding must not eat a keystroke
// ---------------------------------------------------------------------------

/// With a `@keydown` binding attached, an ordinary letter still edits the
/// focused field, and the handler still hears the key. Both halves run on the
/// same press, which is the property that makes the binding safe to add.
#[test]
fn ordinary_typing_still_edits_while_a_keydown_handler_is_present() {
    let vnode = app(None);
    let mut targets = collect(&vnode);
    let mut focused = None;

    // Focus it the way F2 does.
    let plan = plan_keydown(&vnode, "F2");
    apply_keydown(
        &plan,
        "F2",
        &mut targets,
        &mut focused,
        &value_len(&vnode),
        &mut |_, _| {},
    );

    // Press "A": the handler hears it and the editor is left alone by dispatch.
    let plan = plan_keydown(&vnode, "A");
    let mut heard: Vec<String> = Vec::new();
    apply_keydown(
        &plan,
        "A",
        &mut targets,
        &mut focused,
        &value_len(&vnode),
        &mut |_, payload| heard.push(payload.unwrap_or_default().to_string()),
    );
    assert_eq!(heard, vec!["A".to_string()]);
    let idx = focused_input_index(&targets).expect("still focused");
    assert_eq!(
        targets[idx].cursor,
        "draft".chars().count(),
        "dispatch did not edit or blur"
    );

    // The editor's own step — which the loop runs in the same key arm — works.
    // (Printable characters reach the field on winit's `ReceivedCharacter`
    // event, not on `KeyboardInput`; either way the `on:keydown` binding above
    // had no say in it.)
    let res = apply_edit(&mut targets[idx], "draft", EditAction::Insert('a'));
    assert_eq!(res.value.as_deref(), Some("drafta"));
    assert!(res.needs_repaint());
}

#[test]
fn dispatch_never_edits_blurs_or_changes_focus() {
    let vnode = app(None);
    let mut targets = collect(&vnode);
    let mut focused = None;
    let plan = plan_keydown(&vnode, "F2");
    apply_keydown(
        &plan,
        "F2",
        &mut targets,
        &mut focused,
        &value_len(&vnode),
        &mut |_, _| {},
    );
    let idx = focused_input_index(&targets).unwrap();
    let before = (
        targets[idx].focused,
        targets[idx].cursor,
        targets[idx].anchor,
    );

    // Every editing key a binding can name goes through dispatch first.
    for key in [
        "A",
        "Z",
        "0",
        "9",
        "ArrowLeft",
        "ArrowRight",
        "ArrowUp",
        "ArrowDown",
        "Backspace",
        "Delete",
        "Enter",
        "Home",
        "End",
        "Space",
        "Tab",
    ] {
        let plan = plan_keydown(&vnode, key);
        assert_eq!(
            plan.handlers,
            vec!["on_key".to_string()],
            "handler sees {key}"
        );
        apply_keydown(
            &plan,
            key,
            &mut targets,
            &mut focused,
            &value_len(&vnode),
            &mut |_, _| {},
        );
        assert_eq!(
            (
                targets[idx].focused,
                targets[idx].cursor,
                targets[idx].anchor
            ),
            before,
            "dispatching {key} must not move focus or the caret"
        );
        assert!(
            !plan.focus_paths.contains(&targets[idx].path),
            "{key} is not a focus grant for this tree"
        );
    }
}

#[test]
fn escape_blurs_the_focused_field_while_a_keydown_handler_is_present() {
    let vnode = app(None);
    let mut targets = collect(&vnode);
    let mut focused = None;
    let plan = plan_keydown(&vnode, "F2");
    apply_keydown(
        &plan,
        "F2",
        &mut targets,
        &mut focused,
        &value_len(&vnode),
        &mut |_, _| {},
    );
    assert!(focused_input_index(&targets).is_some());

    // Esc is *not* a focus grant: dispatch sees it, grants nothing.
    let plan = plan_keydown(&vnode, "Escape");
    assert!(plan.focus_paths.is_empty());
    let mut heard = Vec::new();
    apply_keydown(
        &plan,
        "Escape",
        &mut targets,
        &mut focused,
        &value_len(&vnode),
        &mut |_, p| heard.push(p.unwrap_or_default().to_string()),
    );
    assert_eq!(heard, vec!["Escape".to_string()]);
    assert!(
        focused_input_index(&targets).is_some(),
        "dispatch alone does not blur"
    );

    // The editor's Blur — which the loop runs in the same key arm — still works.
    let idx = focused_input_index(&targets).unwrap();
    let res = apply_edit(&mut targets[idx], "draft", EditAction::Blur);
    assert!(res.needs_repaint());
    assert!(!targets[idx].focused, "focus is gone");
    assert_eq!(targets[idx].cursor, 0, "and the caret resets");
    assert!(focused_input_index(&targets).is_none());
}

// ---------------------------------------------------------------------------
// The winit-facing half (needs the backend that owns VirtualKeyCode)
// ---------------------------------------------------------------------------

#[cfg(feature = "skia-native")]
mod winit_keys {
    use super::*;
    use velox_renderer::{
        apply_edit_to_focused, dispatch_keydown, edit_action_for_key, focus_input_by_id, key_name,
    };
    use winit::event::VirtualKeyCode as K;

    #[test]
    fn the_key_name_table_is_author_facing() {
        // The use case.
        assert_eq!(key_name(K::F2), "F2");
        // The keys an author is most likely to compare against.
        assert_eq!(key_name(K::Escape), "Escape");
        assert_eq!(key_name(K::Return), "Enter");
        assert_eq!(key_name(K::NumpadEnter), "Enter");
        assert_eq!(key_name(K::Back), "Backspace");
        assert_eq!(key_name(K::Left), "ArrowLeft");
        assert_eq!(key_name(K::Up), "ArrowUp");
        assert_eq!(key_name(K::A), "A");
        assert_eq!(key_name(K::Z), "Z");
        assert_eq!(key_name(K::Key0), "0");
        assert_eq!(key_name(K::Key9), "9");
        assert_eq!(key_name(K::Space), "Space");
        assert_eq!(key_name(K::Tab), "Tab");
        assert_eq!(key_name(K::Delete), "Delete");
        assert_eq!(key_name(K::Home), "Home");
        assert_eq!(key_name(K::End), "End");
        assert_eq!(key_name(K::F12), "F12");
    }

    #[test]
    fn an_unmapped_key_is_named_rather_than_panicking() {
        // The table is intentionally partial — it covers what an author writes a
        // branch for, not every key the OS can send — so the fallback is load
        // bearing, and a key outside the table must still arrive as a usable
        // name rather than panicking or arriving as an empty string.
        for key in [K::Snapshot, K::Pause] {
            assert_eq!(key_name(key), "Unidentified", "{key:?}");
        }
    }

    #[test]
    fn letters_are_named_upper_case_regardless_of_shift() {
        // `key_name` takes no modifier: a shifted `a` and an unshifted one are
        // the same key press with the same name, which is what lets
        // `if key == "A"` work without the author also handling a "a" spelling.
        assert_eq!(key_name(K::A), "A");
        assert_eq!(key_name(K::LShift), "Shift");
        assert_eq!(key_name(K::RShift), "Shift");
    }

    /// The whole chain a windowed app runs, minus the window: a real
    /// `VirtualKeyCode`, the real dispatcher, the real focus grant, the real
    /// handler call.
    #[test]
    fn f2_focuses_the_field_the_author_named() {
        let vnode = app(None);
        let mut targets = collect(&vnode);
        let mut focused = None;
        let mut seen: Vec<String> = Vec::new();
        let changed = dispatch_keydown(
            K::F2,
            &mut targets,
            &Some(vnode.clone()),
            &mut focused,
            &mut |_, payload| seen.push(payload.unwrap_or_default().to_string()),
        );
        assert!(changed);
        assert_eq!(seen, vec!["F2".to_string()]);
        let path = find_focus_id_path(&vnode, &[], "composer").unwrap();
        assert_eq!(
            focused_input_index(&targets),
            input_index_by_path(&targets, &path)
        );
        assert_eq!(focused.as_deref(), Some(path.as_slice()));
    }

    #[test]
    fn dispatching_a_letter_does_not_consume_it() {
        let vnode = app(None);
        let mut targets = collect(&vnode);
        let mut focused = None;
        let mut seen: Vec<String> = Vec::new();
        let changed = dispatch_keydown(
            K::A,
            &mut targets,
            &Some(vnode.clone()),
            &mut focused,
            &mut |_, payload| seen.push(payload.unwrap_or_default().to_string()),
        );
        assert!(changed, "the handler ran");
        assert_eq!(seen, vec!["A".to_string()]);
        assert!(
            focused_input_index(&targets).is_none(),
            "and it granted no focus"
        );
    }

    #[test]
    fn focus_by_id_is_the_same_thing_the_key_path_does() {
        let vnode = app(None);
        let mut targets = collect(&vnode);
        let mut focused = None;
        assert!(focus_input_by_id(
            &mut targets,
            &Some(vnode.clone()),
            &mut focused,
            "composer"
        ));
        let path = find_focus_id_path(&vnode, &[], "composer").unwrap();
        assert_eq!(
            focused_input_index(&targets),
            input_index_by_path(&targets, &path)
        );

        // Already focused: reports no change, so a caller cannot mistake a
        // repeat for a focus gain.
        assert!(!focus_input_by_id(
            &mut targets,
            &Some(vnode.clone()),
            &mut focused,
            "composer"
        ));
        assert!(!focus_input_by_id(
            &mut targets,
            &Some(vnode.clone()),
            &mut focused,
            "nope"
        ));
        assert!(!focus_input_by_id(
            &mut targets,
            &None,
            &mut focused,
            "composer"
        ));
    }

    #[test]
    fn dispatch_without_a_tree_is_quiet() {
        let mut targets = collect(&app(None));
        let mut focused = None;
        assert!(!dispatch_keydown(
            K::F2,
            &mut targets,
            &None,
            &mut focused,
            &mut |_, _| panic!("no tree, no handler")
        ));
    }

    /// The loop's key arm, run verbatim minus the window: dispatch first, then
    /// the editor branch, both for the same `VirtualKeyCode`.
    ///
    /// This is the additivity proof that cannot be argued with. It is the same
    /// two calls, in the same order, that `run_window_vnode_skia` makes — and
    /// `event_loop_arms.rs` separately asserts both loops still make them, in
    /// that order, with `dispatch_keydown` outside the `match`. Between the two
    /// tests there is no arrangement of a key press where adding a `@keydown`
    /// binding costs a keystroke.
    fn press(
        keycode: K,
        shift: bool,
        vnode: &VNode,
        targets: &mut [velox_renderer::events::InputTarget],
        focused: &mut Option<Vec<usize>>,
        on_event: &mut impl FnMut(&str, Option<&str>),
    ) -> bool {
        let mut repainted = false;
        if dispatch_keydown(keycode, targets, &Some(vnode.clone()), focused, on_event) {
            repainted = true;
        }
        if let Some(action) = edit_action_for_key(keycode, shift)
            && apply_edit_to_focused(targets, action, &Some(vnode.clone()), focused, on_event)
        {
            repainted = true;
        }
        repainted
    }

    #[test]
    fn a_press_runs_the_binding_and_the_editor_together() {
        // An app whose field is bound to F2 and which also reacts to a letter.
        let vnode = h(
            "div",
            vec![],
            vec![keydown_host(
                "on_key",
                text_input("composer", "F2", "draft"),
            )],
        );
        let mut targets = collect(&vnode);
        let mut focused: Option<Vec<usize>> = None;
        let mut seen: Vec<(String, String)> = Vec::new();
        macro_rules! press {
            ($key:expr) => {
                press(
                    $key,
                    false,
                    &vnode,
                    &mut targets,
                    &mut focused,
                    &mut |n, p| seen.push((n.to_string(), p.unwrap_or_default().to_string())),
                )
            };
        }

        // F2: the handler hears the key and the field gains focus.
        press!(K::F2);
        let path = find_focus_id_path(&vnode, &[], "composer").unwrap();
        assert_eq!(
            focused_input_index(&targets),
            input_index_by_path(&targets, &path)
        );
        assert_eq!(seen, vec![("on_key".to_string(), "F2".to_string())]);

        // A printable letter: the handler hears it, and it is not an editing
        // *action* — its text arrives on winit's separate `ReceivedCharacter`
        // event, which the binding likewise has no say in.
        seen.clear();
        let idx = focused_input_index(&targets).unwrap();
        assert_eq!(targets[idx].cursor, "draft".chars().count());
        press!(K::H);
        assert_eq!(seen, vec![("on_key".to_string(), "H".to_string())]);
        assert_eq!(targets[idx].cursor, "draft".chars().count(), "not an edit");
        assert_eq!(edit_action_for_key(K::H, false), None);

        // Backspace *is* an editing action: heard, and the caret steps back.
        press!(K::Back);
        assert_eq!(seen.last().unwrap().1, "Backspace");
        assert_eq!(targets[idx].cursor, "draft".chars().count() - 1);

        // Escape: heard, and focus is gone.
        press!(K::Escape);
        assert_eq!(seen.last().unwrap().1, "Escape");
        assert!(focused_input_index(&targets).is_none());
        assert!(focused.is_none(), "the index-free mirror followed");
    }

    #[test]
    fn an_editing_key_reaches_the_editor_with_a_binding_attached() {
        let vnode = h(
            "div",
            vec![],
            vec![keydown_host("on_key", text_input("composer", "F2", "hi"))],
        );
        let mut targets = collect(&vnode);
        let mut focused: Option<Vec<usize>> = None;
        let mut seen: Vec<(String, String)> = Vec::new();
        press(
            K::F2,
            false,
            &vnode,
            &mut targets,
            &mut focused,
            &mut |n, p| seen.push((n.to_string(), p.unwrap_or_default().to_string())),
        );
        let idx = focused_input_index(&targets).unwrap();

        // Every editing key the closed match knows, each of which must still
        // change the caret after the binding has run.
        for (key, want) in [
            (K::Left, 1),
            (K::Right, 2),
            (K::Home, 0),
            (K::End, 2),
            (K::Left, 1),
        ] {
            press(
                key,
                false,
                &vnode,
                &mut targets,
                &mut focused,
                &mut |_, _| {},
            );
            assert_eq!(
                targets[idx].cursor, want,
                "{key:?} must still move the caret"
            );
        }

        // Shift is the one modifier the editor honours, and it is honoured
        // *after* the binding ran — the binding cannot suppress it. Shift+End
        // from the start of the value selects all of it.
        press(
            K::Home,
            false,
            &vnode,
            &mut targets,
            &mut focused,
            &mut |_, _| {},
        );
        assert_eq!(targets[idx].cursor, 0);
        press(
            K::End,
            true,
            &vnode,
            &mut targets,
            &mut focused,
            &mut |_, _| {},
        );
        assert_eq!(targets[idx].cursor, 2);
        assert_eq!(
            targets[idx].anchor,
            Some(0),
            "shift+end extends the selection"
        );
        press(
            K::Escape,
            false,
            &vnode,
            &mut targets,
            &mut focused,
            &mut |_, _| {},
        );

        // Focus is gone, selection and all.
        assert!(focused_input_index(&targets).is_none());
    }

    #[test]
    fn a_key_with_no_edit_action_and_no_binding_is_a_complete_no_op() {
        // The pre-@keydown behaviour: an unbound, non-editing key costs
        // nothing, triggers nothing and paints nothing.
        let vnode = inert();
        let mut targets = collect(&vnode);
        let mut focused = None;
        let mut called = 0;
        assert!(!press(
            K::F12,
            false,
            &vnode,
            &mut targets,
            &mut focused,
            &mut |_, _| called += 1
        ));
        assert_eq!(called, 0);
        assert!(focused_input_index(&targets).is_none());
    }
}
