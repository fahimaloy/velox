use std::cell::RefCell as StdRefCell;
use std::rc::Rc;
use velox_core::lifecycle::{before_destroy, on_mounted, run_destroy_hooks, run_mounted_hooks};

#[test]
fn test_mounted_and_destroy_hooks() {
    // Wrap counters so closures can own clones
    let v1 = Rc::new(StdRefCell::new(0));
    let v2 = Rc::new(StdRefCell::new(0));

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

    // Execute the queued hooks
    run_mounted_hooks();
    run_destroy_hooks();

    // Verify they ran
    assert_eq!(*v1.borrow(), 1);
    assert_eq!(*v2.borrow(), 2);
}
