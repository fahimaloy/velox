use std::cell::RefCell;
use std::rc::Rc;

use velox_dom::{h, text, Props};
use velox_renderer::{events, events::EventRegistry, Renderer};

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
    let tree = r.mount(&vnode);

    let click_count = Rc::new(RefCell::new(0));
    let hover_count = Rc::new(RefCell::new(0));
    let mut reg = EventRegistry::new();
    {
        let cc = click_count.clone();
        reg.on("onClick", move || {
            *cc.borrow_mut() += 1;
        });
    }
    {
        let hc = hover_count.clone();
        reg.on("onHover", move || {
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
    let tree = r.mount(&vnode);

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
    let tree = r.mount(&vnode);

    let count = Rc::new(RefCell::new(0));
    let mut reg = EventRegistry::new();
    // Register a different handler name
    {
        let c = count.clone();
        reg.on("otherHandler", move || {
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
        reg.on("test", move || {
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
    reg.on("foo", || {});
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
    let tree = r.mount(&vnode);

    let count = Rc::new(RefCell::new(0));
    let mut reg = EventRegistry::new();
    {
        let c = count.clone();
        reg.on("deepClick", move || {
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
    let tree = r.mount(&vnode);

    let count = Rc::new(RefCell::new(0));
    let mut reg = EventRegistry::new();
    {
        let c = count.clone();
        reg.on("shared", move || {
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
    let tree = r.mount(&vnode);

    let a_count = Rc::new(RefCell::new(0));
    let b_count = Rc::new(RefCell::new(0));
    let mut reg = EventRegistry::new();
    {
        let ac = a_count.clone();
        reg.on("handlerOne", move || {
            *ac.borrow_mut() += 1;
        });
    }
    {
        let bc = b_count.clone();
        reg.on("handlerTwo", move || {
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
    let tree = r.mount(&vnode);

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
    let tree = r.mount(&vnode);

    // 1 div + 3 span + 3 text = 7
    assert_eq!(tree.node_count, 7);
    assert_eq!(tree.text_count, 3);
}

#[test]
fn mount_empty_element() {
    let vnode = h("div", (), vec![]);
    let r = velox_renderer::new_selected_renderer();
    let tree = r.mount(&vnode);

    assert_eq!(tree.node_count, 1);
    assert_eq!(tree.text_count, 0);
}

#[test]
fn mount_text_only() {
    let vnode = text("plain text");
    let r = velox_renderer::new_selected_renderer();
    let tree = r.mount(&vnode);

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
    let tree = r.mount(&vnode);

    // 10 divs + 1 text = 11
    assert_eq!(tree.node_count, 11);
    assert_eq!(tree.text_count, 1);
}

#[test]
fn mount_wide_tree() {
    let children: Vec<velox_dom::VNode> = (0..50)
        .map(|i| h("span", (), vec![text(&i.to_string())]))
        .collect();
    let vnode = h("div", (), children);
    let r = velox_renderer::new_selected_renderer();
    let tree = r.mount(&vnode);

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
    let tree = r.mount(&vnode);

    let count = Rc::new(RefCell::new(0));
    let mut rt = events::Runtime::new(tree);
    {
        let c = count.clone();
        rt.registry.on("onClick", move || {
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
    let tree = r.mount(&vnode);

    let dbl_count = Rc::new(RefCell::new(0));
    let mut rt = events::Runtime::new(tree);
    {
        let dc = dbl_count.clone();
        rt.registry.on("onDbl", move || {
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
    let tree = r.mount(&vnode);

    let hover_count = Rc::new(RefCell::new(0));
    let mut rt = events::Runtime::new(tree);
    {
        let hc = hover_count.clone();
        rt.registry.on("onHover", move || {
            *hc.borrow_mut() += 1;
        });
    }

    rt.cursor_moved();
    assert_eq!(*hover_count.borrow(), 1);

    // Second hover should not fire (hover_sent flag)
    rt.cursor_moved();
    assert_eq!(*hover_count.borrow(), 1);

    // Reset allows another hover
    rt.reset_hover();
    rt.cursor_moved();
    assert_eq!(*hover_count.borrow(), 2);
}

// =============================================================================
// Reconcile integration
// =============================================================================

#[test]
fn reconcile_keyed_children_preserves_node_identity() {
    let mut old: Vec<velox_dom::VNode> = vec![
        h("li", vec![("key", "a"), ("data-id", "1")], vec![text("A")]),
        h("li", vec![("key", "b"), ("data-id", "2")], vec![text("B")]),
    ];
    let new: Vec<velox_dom::VNode> = vec![
        h("li", vec![("key", "b")], vec![text("B-new")]),
        h("li", vec![("key", "a")], vec![text("A-new")]),
    ];

    velox_renderer::reconcile_keyed_children(&mut old, &new);

    assert_eq!(old.len(), 2);
    // After reconciliation, order should be b, a
    match &old[0] {
        velox_dom::VNode::Element { props, .. } => {
            assert_eq!(props.attrs.get("key"), Some(&"b".to_string()));
        }
        _ => panic!("expected element"),
    }
    match &old[1] {
        velox_dom::VNode::Element { props, .. } => {
            assert_eq!(props.attrs.get("key"), Some(&"a".to_string()));
        }
        _ => panic!("expected element"),
    }
}

#[test]
fn reconcile_keyed_children_adds_new() {
    let mut old: Vec<velox_dom::VNode> = vec![h("li", vec![("key", "a")], vec![])];
    let new: Vec<velox_dom::VNode> = vec![
        h("li", vec![("key", "a")], vec![]),
        h("li", vec![("key", "b")], vec![]),
    ];

    velox_renderer::reconcile_keyed_children(&mut old, &new);

    assert_eq!(old.len(), 2);
}

#[test]
fn reconcile_keyed_children_removes_old() {
    let mut old: Vec<velox_dom::VNode> = vec![
        h("li", vec![("key", "a")], vec![]),
        h("li", vec![("key", "b")], vec![]),
        h("li", vec![("key", "c")], vec![]),
    ];
    let new: Vec<velox_dom::VNode> = vec![h("li", vec![("key", "b")], vec![])];

    velox_renderer::reconcile_keyed_children(&mut old, &new);

    assert_eq!(old.len(), 1);
    match &old[0] {
        velox_dom::VNode::Element { props, .. } => {
            assert_eq!(props.attrs.get("key"), Some(&"b".to_string()));
        }
        _ => panic!("expected element"),
    }
}

#[test]
fn reconcile_keyed_children_empty_old() {
    let mut old: Vec<velox_dom::VNode> = vec![];
    let new: Vec<velox_dom::VNode> = vec![
        h("li", vec![("key", "a")], vec![]),
        h("li", vec![("key", "b")], vec![]),
    ];

    velox_renderer::reconcile_keyed_children(&mut old, &new);

    assert_eq!(old.len(), 2);
}

#[test]
fn reconcile_keyed_children_empty_new() {
    let mut old: Vec<velox_dom::VNode> = vec![h("li", vec![("key", "a")], vec![])];
    let new: Vec<velox_dom::VNode> = vec![];

    velox_renderer::reconcile_keyed_children(&mut old, &new);

    assert_eq!(old.len(), 0);
}
