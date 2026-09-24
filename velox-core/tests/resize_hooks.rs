use std::cell::RefCell;
use std::rc::Rc;

use velox_core::lifecycle::{
    before_destroy, cleanup_component, clear_current_component, generate_component_id, on_resize,
    run_all_destroy_hooks, run_resize_hooks, set_current_component,
};

#[test]
fn resize_dispatcher_invokes_each_call_for_renderer_change_filtering() {
    let id = generate_component_id();
    let calls = Rc::new(RefCell::new(Vec::new()));

    set_current_component(id);
    {
        let calls = calls.clone();
        velox_core::on_resize!(move |width, height| calls.borrow_mut().push((width, height)));
    }
    clear_current_component();

    run_resize_hooks(640, 480);
    run_resize_hooks(640, 480);
    run_resize_hooks(800, 600);

    assert_eq!(&*calls.borrow(), &[(640, 480), (640, 480), (800, 600)]);
    cleanup_component(id);
}

#[test]
fn resize_hooks_can_register_another_hook_while_dispatching() {
    let outer_id = generate_component_id();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let nested_calls = Rc::new(RefCell::new(Vec::new()));

    set_current_component(outer_id);
    {
        let calls = calls.clone();
        let nested_calls = nested_calls.clone();
        velox_core::lifecycle::on_resize(move |width, height| {
            calls.borrow_mut().push((width, height));
            let nested_calls = nested_calls.clone();
            on_resize(move |nested_width, nested_height| {
                nested_calls
                    .borrow_mut()
                    .push((nested_width, nested_height));
            });
        });
    }
    clear_current_component();

    // The callback may touch the registry while the dispatcher is running.
    set_current_component(outer_id);
    run_resize_hooks(1024, 768);
    clear_current_component();
    run_resize_hooks(1024, 768);
    run_resize_hooks(1280, 800);

    assert_eq!(&*calls.borrow(), &[(1024, 768), (1024, 768), (1280, 800)]);
    assert_eq!(&*nested_calls.borrow(), &[(1024, 768), (1280, 800)]);
    cleanup_component(outer_id);
}

#[test]
fn global_destroy_cleanup_preserves_other_components_resize_hooks() {
    let first_id = generate_component_id();
    let second_id = generate_component_id();
    let second_calls = Rc::new(RefCell::new(Vec::new()));

    set_current_component(first_id);
    on_resize(|_, _| {});
    before_destroy(|| {});
    clear_current_component();
    set_current_component(second_id);
    {
        let second_calls = second_calls.clone();
        on_resize(move |width, height| second_calls.borrow_mut().push((width, height)));
    }
    clear_current_component();

    run_all_destroy_hooks();
    run_resize_hooks(1024, 768);

    assert_eq!(&*second_calls.borrow(), &[(1024, 768)]);
    cleanup_component(first_id);
    cleanup_component(second_id);
}
