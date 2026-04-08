use std::cell::RefCell as StdRefCell;
use std::rc::Rc;
use velox_core::lifecycle::{
    before_destroy, generate_component_id, on_mounted, run_destroy_hooks, run_mounted_hooks,
    set_current_component,
};

#[test]
fn test_mounted_and_destroy_hooks() {
    // Wrap counters so closures can own clones
    let v1 = Rc::new(StdRefCell::new(0));
    let v2 = Rc::new(StdRefCell::new(0));

    // Generate a component ID and set it as current
    let comp_id = generate_component_id();
    set_current_component(comp_id);

    {
        let v1_clone = v1.clone();
        on_mounted(move || {
            *v1_clone.borrow_mut() = 1;
        });
    }
    {
        let v2_clone = v2.clone();
        before_destroy(move || {
            *v2_clone.borrow_mut() = 2;
        });
    }

    // Execute the queued hooks for this component
    run_mounted_hooks(comp_id);
    run_destroy_hooks(comp_id);

    // Verify they ran
    assert_eq!(*v1.borrow(), 1);
    assert_eq!(*v2.borrow(), 2);
}
