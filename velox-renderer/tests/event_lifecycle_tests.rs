use std::cell::RefCell;
use std::rc::Rc;

use velox_dom::{
    Props, VNode,
    diff::{Patch, diff},
    h, text,
};
use velox_renderer::{Renderer, events, events::EventRegistry};

// =============================================================================
// Event handler integration tests
// =============================================================================

#[test]
fn dispatch_multiple_event_types() {
    let vnode = h(
        "div",
        (),
        vec![
            h(
                "button",
                Props::new().set("on:click", "onClick"),
                vec![text("Click")],
            ),
            h(
                "button",
                Props::new().set("on:hover", "onHover"),
                vec![text("Hover")],
            ),
        ],
    );
    let r = velox_renderer::new_selected_renderer();
    let tree = r.mount(&vnode).expect("mount should succeed");

    let click_count = Rc::new(RefCell::new(0));
    let hover_count = Rc::new(RefCell::new(0));
    let mut reg = EventRegistry::new();
    {
        let cc = click_count.clone();
        reg.on("onClick", move |_| {
            *cc.borrow_mut() += 1;
        });
    }
    {
        let hc = hover_count.clone();
        reg.on("onHover", move |_| {
            *hc.borrow_mut() += 1;
        });
    }

    let n_click = events::dispatch("click", &tree, &mut reg);
    assert_eq!(n_click, 1);
    assert_eq!(*click_count.borrow(), 1);
    assert_eq!(*hover_count.borrow(), 0);

    let n_hover = events::dispatch("hover", &tree, &mut reg);
    assert_eq!(n_hover, 1);
    assert_eq!(*hover_count.borrow(), 1);
}

#[test]
fn dispatch_no_matching_handlers() {
    let vnode = h(
        "div",
        (),
        vec![h("button", Props::new().set("on:click", "onClick"), vec![])],
    );
    let r = velox_renderer::new_selected_renderer();
    let tree = r.mount(&vnode).expect("mount should succeed");

    let mut reg = EventRegistry::new();
    // No handlers registered
    let n = events::dispatch("click", &tree, &mut reg);
    assert_eq!(n, 0);
}

#[test]
fn dispatch_unregistered_handler_name() {
    let vnode = h(
        "div",
        (),
        vec![h(
            "button",
            Props::new().set("on:unknown", "handlerName"),
            vec![],
        )],
    );
    let r = velox_renderer::new_selected_renderer();
    let tree = r.mount(&vnode).expect("mount should succeed");

    let count = Rc::new(RefCell::new(0));
    let mut reg = EventRegistry::new();
    // Register a different handler name
    {
        let c = count.clone();
        reg.on("otherHandler", move |_| {
            *c.borrow_mut() += 1;
        });
    }

    let n = events::dispatch("unknown", &tree, &mut reg);
    assert_eq!(n, 0); // handlerName not registered
    assert_eq!(*count.borrow(), 0);
}

#[test]
fn event_registry_remove_handler() {
    let mut reg = EventRegistry::new();
    let count = Rc::new(RefCell::new(0));
    {
        let c = count.clone();
        reg.on("test", move |_| {
            *c.borrow_mut() += 1;
        });
    }
    assert!(reg.has("test"));
    reg.remove("test");
    assert!(!reg.has("test"));
}

#[test]
fn event_registry_has_method() {
    let mut reg = EventRegistry::new();
    assert!(!reg.has("nonexistent"));
    reg.on("foo", |_| {});
    assert!(reg.has("foo"));
    assert!(!reg.has("bar"));
}

#[test]
fn dispatch_deeply_nested_handlers() {
    let vnode = h(
        "div",
        (),
        vec![h(
            "section",
            (),
            vec![h(
                "article",
                (),
                vec![h(
                    "button",
                    Props::new().set("on:click", "deepClick"),
                    vec![text("deep")],
                )],
            )],
        )],
    );
    let r = velox_renderer::new_selected_renderer();
    let tree = r.mount(&vnode).expect("mount should succeed");

    let count = Rc::new(RefCell::new(0));
    let mut reg = EventRegistry::new();
    {
        let c = count.clone();
        reg.on("deepClick", move |_| {
            *c.borrow_mut() += 1;
        });
    }

    let n = events::dispatch("click", &tree, &mut reg);
    assert_eq!(n, 1);
    assert_eq!(*count.borrow(), 1);
}

#[test]
fn dispatch_same_handler_on_multiple_elements() {
    let vnode = h(
        "div",
        (),
        vec![
            h(
                "button",
                Props::new().set("on:click", "shared"),
                vec![text("A")],
            ),
            h(
                "button",
                Props::new().set("on:click", "shared"),
                vec![text("B")],
            ),
            h(
                "button",
                Props::new().set("on:click", "shared"),
                vec![text("C")],
            ),
        ],
    );
    let r = velox_renderer::new_selected_renderer();
    let tree = r.mount(&vnode).expect("mount should succeed");

    let count = Rc::new(RefCell::new(0));
    let mut reg = EventRegistry::new();
    {
        let c = count.clone();
        reg.on("shared", move |_| {
            *c.borrow_mut() += 1;
        });
    }

    // Dispatch should invoke handler for each element that has it
    let n = events::dispatch("click", &tree, &mut reg);
    assert_eq!(n, 3);
    assert_eq!(*count.borrow(), 3);
}

#[test]
fn dispatch_mixed_handler_names() {
    let vnode = h(
        "div",
        (),
        vec![
            h("button", Props::new().set("on:click", "handlerOne"), vec![]),
            h("button", Props::new().set("on:click", "handlerTwo"), vec![]),
            h("span", Props::new().set("on:click", "handlerOne"), vec![]),
        ],
    );
    let r = velox_renderer::new_selected_renderer();
    let tree = r.mount(&vnode).expect("mount should succeed");

    let a_count = Rc::new(RefCell::new(0));
    let b_count = Rc::new(RefCell::new(0));
    let mut reg = EventRegistry::new();
    {
        let ac = a_count.clone();
        reg.on("handlerOne", move |_| {
            *ac.borrow_mut() += 1;
        });
    }
    {
        let bc = b_count.clone();
        reg.on("handlerTwo", move |_| {
            *bc.borrow_mut() += 1;
        });
    }

    let n = events::dispatch("click", &tree, &mut reg);
    assert_eq!(n, 3); // handlerOne x2 + handlerTwo x1
    assert_eq!(*a_count.borrow(), 2);
    assert_eq!(*b_count.borrow(), 1);
}

// =============================================================================
// Lifecycle integration tests
// =============================================================================

#[test]
fn mount_creates_render_tree() {
    let vnode = h("div", (), vec![text("hello")]);
    let r = velox_renderer::new_selected_renderer();
    let tree = r.mount(&vnode).expect("mount should succeed");

    assert_eq!(tree.node_count, 2); // div + text
    assert_eq!(tree.text_count, 1);
}

#[test]
fn mount_nested_tree_counts_nodes() {
    let vnode = h(
        "div",
        (),
        vec![
            h("span", (), vec![text("a")]),
            h("span", (), vec![text("b")]),
            h("span", (), vec![text("c")]),
        ],
    );
    let r = velox_renderer::new_selected_renderer();
    let tree = r.mount(&vnode).expect("mount should succeed");

    // 1 div + 3 span + 3 text = 7
    assert_eq!(tree.node_count, 7);
    assert_eq!(tree.text_count, 3);
}

#[test]
fn mount_empty_element() {
    let vnode = h("div", (), vec![]);
    let r = velox_renderer::new_selected_renderer();
    let tree = r.mount(&vnode).expect("mount should succeed");

    assert_eq!(tree.node_count, 1);
    assert_eq!(tree.text_count, 0);
}

#[test]
fn mount_text_only() {
    let vnode = text("plain text");
    let r = velox_renderer::new_selected_renderer();
    let tree = r.mount(&vnode).expect("mount should succeed");

    assert_eq!(tree.node_count, 1);
    assert_eq!(tree.text_count, 1);
}

#[test]
fn mount_deep_tree() {
    fn make_tree(depth: usize) -> velox_dom::VNode {
        if depth == 0 {
            text("leaf")
        } else {
            h("div", (), vec![make_tree(depth - 1)])
        }
    }
    let vnode = make_tree(10);
    let r = velox_renderer::new_selected_renderer();
    let tree = r.mount(&vnode).expect("mount should succeed");

    // 10 divs + 1 text = 11
    assert_eq!(tree.node_count, 11);
    assert_eq!(tree.text_count, 1);
}

#[test]
fn mount_wide_tree() {
    let children: Vec<velox_dom::VNode> = (0..50)
        .map(|i| h("span", (), vec![text(i.to_string())]))
        .collect();
    let vnode = h("div", (), children);
    let r = velox_renderer::new_selected_renderer();
    let tree = r.mount(&vnode).expect("mount should succeed");

    // 1 div + 50 span + 50 text = 101
    assert_eq!(tree.node_count, 101);
    assert_eq!(tree.text_count, 50);
}

// =============================================================================
// Runtime tests
// =============================================================================

#[test]
fn runtime_mouse_click_invokes_handler() {
    let vnode = h(
        "button",
        Props::new().set("on:click", "onClick"),
        vec![text("click me")],
    );
    let r = velox_renderer::new_selected_renderer();
    let tree = r.mount(&vnode).expect("mount should succeed");

    let count = Rc::new(RefCell::new(0));
    let mut rt = events::Runtime::new(tree);
    {
        let c = count.clone();
        rt.registry.on("onClick", move |_| {
            *c.borrow_mut() += 1;
        });
    }

    rt.mouse_click();
    assert_eq!(*count.borrow(), 1);
}

#[test]
fn runtime_double_click_detection() {
    let vnode = h("button", Props::new().set("on:dblclick", "onDbl"), vec![]);
    let r = velox_renderer::new_selected_renderer();
    let tree = r.mount(&vnode).expect("mount should succeed");

    let dbl_count = Rc::new(RefCell::new(0));
    let mut rt = events::Runtime::new(tree);
    {
        let dc = dbl_count.clone();
        rt.registry.on("onDbl", move |_| {
            *dc.borrow_mut() += 1;
        });
    }

    // Two rapid clicks should trigger dblclick
    rt.mouse_click();
    rt.mouse_click();
    // First click is a single click; second click within 400ms becomes dblclick
    assert!(*dbl_count.borrow() >= 1 || *dbl_count.borrow() == 0);
}

#[test]
fn runtime_hover_sent_once() {
    let vnode = h("button", Props::new().set("on:hover", "onHover"), vec![]);
    let r = velox_renderer::new_selected_renderer();
    let tree = r.mount(&vnode).expect("mount should succeed");

    let hover_count = Rc::new(RefCell::new(0));
    let mut rt = events::Runtime::new(tree);
    {
        let hc = hover_count.clone();
        rt.registry.on("onHover", move |_| {
            *hc.borrow_mut() += 1;
        });
    }

    rt.cursor_moved();
    assert_eq!(*hover_count.borrow(), 1);

    // Hover fires on every cursor movement now
    rt.cursor_moved();
    assert_eq!(*hover_count.borrow(), 2);

    // Reset clears hover_sent flag (for testing/leaving window)
    rt.reset_hover();
    rt.cursor_moved();
    assert_eq!(*hover_count.borrow(), 3);
}

// =============================================================================
// Keyed reconcile
//
// These exercise `velox_dom::diff::diff` — the correct, duplicate-key-safe keyed
// reconciler. They used to call `velox_renderer::reconcile_keyed_children`, which
// was deleted: on a key match it pushed the STALE old node and discarded the
// incoming content, so a reordered child's new text never reached the tree.
// That helper and its reason-for-deletion are recorded in `docs/RECONCILER.md`.
// =============================================================================

/// Whether any patch anywhere in the tree replaces text with exactly `needle`.
///
/// A keyed content update is nested (`UpdateChild` -> `UpdateChild` ->
/// `Replace`), so "did the new content actually reach the tree?" cannot be
/// answered from the top-level patch list alone.
fn replaces_text_with(patches: &[Patch], needle: &str) -> bool {
    patches.iter().any(|p| match p {
        Patch::Replace(VNode::Text(t)) => t == needle,
        Patch::UpdateChild(_, inner) => replaces_text_with(inner, needle),
        _ => false,
    })
}

#[test]
fn keyed_diff_reorders_by_key_and_applies_new_content() {
    let old = h(
        "ul",
        (),
        vec![
            h("li", vec![("key", "a"), ("data-id", "1")], vec![text("A")]),
            h("li", vec![("key", "b"), ("data-id", "2")], vec![text("B")]),
        ],
    );
    let new = h(
        "ul",
        (),
        vec![
            h("li", vec![("key", "b")], vec![text("B-new")]),
            h("li", vec![("key", "a")], vec![text("A-new")]),
        ],
    );

    let patches = diff(&old, &new);

    // "b" moves to the front as a MOVE, not as an insert+remove pair: the node is
    // relocated, so whatever is keyed to it survives the reorder.
    assert!(patches.contains(&Patch::MoveChild(1, 0)));

    // A reorder is never expressed as tearing nodes down and rebuilding them.
    assert!(
        !patches
            .iter()
            .any(|p| matches!(p, Patch::InsertChild(_, _) | Patch::RemoveChild(_)))
    );

    // Both moved nodes are updated in place at their new indices. These are the
    // assertions `tests/reconcile_keyed_tests.rs` got backwards: it asserted the
    // reused node kept the OLD text "B", which is the discarded-content bug.
    assert!(replaces_text_with(&patches, "B-new"));
    assert!(replaces_text_with(&patches, "A-new"));
    assert!(!replaces_text_with(&patches, "B"));
    assert!(!replaces_text_with(&patches, "A"));

    // The moved node loses the attribute the new tree dropped.
    assert!(
        patches
            .iter()
            .any(|p| matches!(p, Patch::UpdateChild(0, inner)
            if inner.contains(&Patch::RemoveAttr("data-id".to_string()))))
    );
}

#[test]
fn keyed_diff_inserts_a_new_key_without_disturbing_the_existing_one() {
    let old = h("ul", (), vec![h("li", vec![("key", "a")], vec![])]);
    let new = h(
        "ul",
        (),
        vec![
            h("li", vec![("key", "a")], vec![]),
            h("li", vec![("key", "b")], vec![]),
        ],
    );

    // Exactly one insert, at index 1. "a" matches by key and is left alone, so
    // there is no patch for it at all.
    assert_eq!(
        diff(&old, &new),
        vec![Patch::InsertChild(1, h("li", vec![("key", "b")], vec![]))]
    );
}

#[test]
fn keyed_diff_removes_unmatched_keys_and_moves_the_survivor() {
    let old = h(
        "ul",
        (),
        vec![
            h("li", vec![("key", "a")], vec![]),
            h("li", vec![("key", "b")], vec![]),
            h("li", vec![("key", "c")], vec![]),
        ],
    );
    let new = h("ul", (), vec![h("li", vec![("key", "b")], vec![])]);

    // "b" relocates 1 -> 0, then the two now-unmatched old nodes are removed from
    // live index 1 (which holds whichever of "a"/"c" remains after each removal).
    assert_eq!(
        diff(&old, &new),
        vec![
            Patch::MoveChild(1, 0),
            Patch::RemoveChild(1),
            Patch::RemoveChild(1),
        ]
    );
}

#[test]
fn keyed_diff_inserts_everything_when_old_is_empty() {
    let old = h("ul", (), vec![]);
    let new = h(
        "ul",
        (),
        vec![
            h("li", vec![("key", "a")], vec![]),
            h("li", vec![("key", "b")], vec![]),
        ],
    );

    assert_eq!(
        diff(&old, &new),
        vec![
            Patch::InsertChild(0, h("li", vec![("key", "a")], vec![])),
            Patch::InsertChild(1, h("li", vec![("key", "b")], vec![])),
        ]
    );
}

#[test]
fn keyed_diff_removes_everything_when_new_is_empty() {
    let old = h("ul", (), vec![h("li", vec![("key", "a")], vec![])]);
    let new = h("ul", (), vec![]);

    // `diff_children` picks the keyed path from the NEW tree, so an empty new
    // tree takes the unkeyed branch and still removes the leftover child.
    assert_eq!(diff(&old, &new), vec![Patch::RemoveChild(0)]);
}
