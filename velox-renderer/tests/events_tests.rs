use std::cell::RefCell;
use std::rc::Rc;

use velox_dom::{h, text, Props};
use velox_renderer::{events, Renderer};

#[test]
fn dispatch_invokes_registered_callback() {
    // Build VNode tree with on:click handler name "inc"
    let vnode = h("button", Props::new().set("on:click", "inc"), vec![text("+1")] );
    let r = velox_renderer::new_selected_renderer();
    let tree = r.mount(&vnode);

    let count = Rc::new(RefCell::new(0));
    let mut reg = events::EventRegistry::new();
    {
        let count = count.clone();
        reg.on("inc", move || {
            *count.borrow_mut() += 1;
        });
    }

    let n = events::dispatch("click", &tree, &mut reg);
    assert_eq!(n, 1);
    assert_eq!(*count.borrow(), 1);
}

#[test]
fn dispatch_handles_multiple_targets() {
    let vnode = h(
        "div",
        Props::new(),
        vec![
            h("button", Props::new().set("on:click", "inc"), vec![]),
            h("button", Props::new().set("on:click", "inc"), vec![]),
        ],
    );
    let r = velox_renderer::new_selected_renderer();
    let tree = r.mount(&vnode);

    let count = Rc::new(RefCell::new(0));
    let mut reg = events::EventRegistry::new();
    {
        let count = count.clone();
        reg.on("inc", move || {
            *count.borrow_mut() += 1;
        });
    }
    let n = events::dispatch("click", &tree, &mut reg);
    assert_eq!(n, 2);
    assert_eq!(*count.borrow(), 2);
}

