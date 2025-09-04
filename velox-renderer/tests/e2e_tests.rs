use std::cell::RefCell;
use std::rc::Rc;

use velox_core::signal::{Signal, effect};
use velox_dom::{diff::diff, h, text, Props, VNode};
use velox_renderer::Renderer;
use velox_style::{Stylesheet, apply_styles};

fn view(count: i32) -> VNode {
    h(
        "div",
        Props::new().set("class", "app"),
        vec![text(format!("{}", count))],
    )
}

#[test]
fn end_to_end_reactive_updates_and_mount() {
    // Model
    let count = Rc::new(Signal::new(0));

    // Style sheet
    let ss = Stylesheet::parse(".app { color: red; }");

    // Current VNode tree stored in a cell, recomputed from signals via effect
    let current: Rc<RefCell<VNode>> = Rc::new(RefCell::new(apply_styles(&view(0), &ss)));

    {
        let count = count.clone();
        let current = current.clone();
        effect(move || {
            let v = view(count.get());
            let styled = apply_styles(&v, &ss);
            *current.borrow_mut() = styled;
        });
    }

    // Initial tree should reflect 0 and carry style
    if let VNode::Element { props, children, .. } = &*current.borrow() {
        assert_eq!(props.attrs.get("class").unwrap(), "app");
        assert!(props.attrs.get("style").unwrap().contains("color: red;"));
        assert!(matches!(children[0], VNode::Text(_)));
    } else { panic!("expected element"); }

    // Mount returns a summary tree (in-memory)
    let r = velox_renderer::new_selected_renderer();
    let mounted = r.mount(&current.borrow());
    assert_eq!(mounted.node_count, 2);
    assert_eq!(mounted.text_count, 1);

    // Update signal; diff old vs new shows Replace for text child
    let before = current.borrow().clone();
    count.set(1);
    let after = current.borrow().clone();
    let patches = match (&before, &after) {
        (VNode::Element { children: a, .. }, VNode::Element { children: b, .. }) => {
            diff(&a[0], &b[0])
        }
        _ => panic!("expected element children"),
    };
    assert!(patches.iter().any(|p| matches!(p, velox_dom::diff::Patch::Replace(_))));
}

