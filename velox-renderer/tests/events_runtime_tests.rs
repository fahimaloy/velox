use std::cell::RefCell;
use std::rc::Rc;

use velox_dom::{h, text, Props};
use velox_renderer::Renderer;

#[test]
fn runtime_click_and_dblclick_and_hover() {
    // Build a simple tree with handlers
    let vnode = h(
        "div",
        Props::new()
            .set("on:click", "inc")
            .set("on:dblclick", "boom")
            .set("on:hover", "hov"),
        vec![text("ok")],
    );
    let r = velox_renderer::new_selected_renderer();
    let tree = r.mount(&vnode);
    let mut rt = velox_renderer::EventRuntime::new(tree);

    let clicks = Rc::new(RefCell::new(0));
    let dbls = Rc::new(RefCell::new(0));
    let hovs = Rc::new(RefCell::new(0));

    {
        let c = clicks.clone();
        rt.registry.on("inc", move || *c.borrow_mut() += 1);
    }
    {
        let d = dbls.clone();
        rt.registry.on("boom", move || *d.borrow_mut() += 1);
    }
    {
        let h = hovs.clone();
        rt.registry.on("hov", move || *h.borrow_mut() += 1);
    }

    // First click
    let n = rt.mouse_click();
    assert_eq!(n, 1);
    assert_eq!(*clicks.borrow(), 1);

    // Second quick click -> dblclick
    let n = rt.mouse_click();
    assert!(n >= 1);
    assert_eq!(*dbls.borrow(), 1);

    // Hover fires once until reset
    let n = rt.cursor_moved();
    assert_eq!(n, 1);
    assert_eq!(*hovs.borrow(), 1);
    let n2 = rt.cursor_moved();
    assert_eq!(n2, 0);
    rt.reset_hover();
    let n3 = rt.cursor_moved();
    assert_eq!(n3, 1);
}
