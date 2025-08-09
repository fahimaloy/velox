use std::cell::RefCell as StdRefCell;
use std::rc::Rc;
use velox_core::signal::{Signal, effect};

#[test]
fn test_signal_and_effect() {
    // Wrap your Signal in Rc so you can clone it into the effect
    let count = Rc::new(Signal::new(0));
    // Observed must also be Rc<RefCell> to mutate inside the closure
    let observed = Rc::new(StdRefCell::new(0));

    {
        let count_clone = count.clone();
        let observed_clone = observed.clone();
        effect(move || {
            *observed_clone.borrow_mut() = count_clone.get();
        });
    }

    // Initial effect run should have written 0
    assert_eq!(*observed.borrow(), 0);

    // This set should notify the effect again
    count.set(42);
    assert_eq!(*observed.borrow(), 42);
}
