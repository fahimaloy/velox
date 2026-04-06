use std::cell::RefCell as StdRefCell;
use std::rc::Rc;
use velox_core::next_tick::next_tick;

// =============================================================================
// next_tick basic tests
// =============================================================================

#[test]
fn next_tick_queues_callback() {
    let called = Rc::new(StdRefCell::new(false));
    {
        let called = called.clone();
        next_tick(move || {
            *called.borrow_mut() = true;
        });
    }
    // The callback is queued and flushed via effect mechanism
    assert!(*called.borrow());
}

#[test]
fn next_tick_multiple_callbacks() {
    let counter = Rc::new(StdRefCell::new(0));
    for i in 0..5 {
        let counter = counter.clone();
        next_tick(move || {
            *counter.borrow_mut() += i;
        });
    }
    // All callbacks should have been executed
    // Sum of 0+1+2+3+4 = 10
    assert_eq!(*counter.borrow(), 10);
}

#[test]
fn next_tick_callbacks_execute_in_order() {
    let order = Rc::new(StdRefCell::new(Vec::new()));
    for i in 0..3 {
        let order = order.clone();
        next_tick(move || {
            order.borrow_mut().push(i);
        });
    }
    assert_eq!(*order.borrow(), vec![0, 1, 2]);
}

#[test]
fn next_tick_callback_can_capture_environment() {
    let data = Rc::new(StdRefCell::new(String::from("initial")));
    {
        let data = data.clone();
        next_tick(move || {
            *data.borrow_mut() = String::from("modified");
        });
    }
    assert_eq!(*data.borrow(), "modified");
}

#[test]
fn next_tick_callback_receives_moved_value() {
    let value = String::from("moved");
    let received = Rc::new(StdRefCell::new(String::new()));
    {
        let received = received.clone();
        next_tick(move || {
            *received.borrow_mut() = value;
        });
    }
    assert_eq!(*received.borrow(), "moved");
}

// =============================================================================
// next_tick edge cases
// =============================================================================

#[test]
fn next_tick_empty_callback() {
    // Should not panic
    next_tick(|| {});
}

#[test]
fn next_tick_nested_callbacks() {
    // Callbacks that schedule more callbacks
    let depth = Rc::new(StdRefCell::new(0));
    {
        let depth = depth.clone();
        next_tick(move || {
            *depth.borrow_mut() = 1;
            let depth2 = depth.clone();
            next_tick(move || {
                *depth2.borrow_mut() = 2;
            });
        });
    }
    // The outer callback runs; inner one is queued for next flush
    assert!(*depth.borrow() >= 1);
}

#[test]
fn next_tick_callback_that_panics() {
    // This test verifies the behavior when a callback panics.
    // We catch it to ensure it doesn't break other tests.
    let result = std::panic::catch_unwind(|| {
        next_tick(|| {
            panic!("intentional panic in next_tick callback");
        });
    });
    // The panic should propagate
    assert!(result.is_err());
}
