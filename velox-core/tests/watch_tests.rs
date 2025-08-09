use std::cell::RefCell as StdRefCell;
use std::rc::Rc;

use velox_core::signal::Signal;
use velox_core::watch::watch;

#[test]
fn watch_triggers_on_change_only() {
    let count = Rc::new(Signal::new(0));
    let events: Rc<StdRefCell<Vec<(i32, i32)>>> = Rc::new(StdRefCell::new(vec![]));

    {
        let count_src = count.clone();
        let events_cb = events.clone();
        watch::<i32, _, _>(
            move || count_src.get(),
            move |new, old| {
                events_cb.borrow_mut().push((*new, *old));
            },
        );
    }

    // No callback on initial run
    assert!(events.borrow().is_empty());

    count.set(1);
    assert_eq!(&*events.borrow(), &vec![(1, 0)]);

    // Setting the same value shouldn't trigger
    count.set(1);
    assert_eq!(&*events.borrow(), &vec![(1, 0)]);

    // Change again
    count.set(2);
    assert_eq!(&*events.borrow(), &vec![(1, 0), (2, 1)]);
}

#[test]
fn watch_callback_can_mutate_signals() {
    let count = Rc::new(Signal::new(0));
    let seen: Rc<StdRefCell<Vec<i32>>> = Rc::new(StdRefCell::new(vec![]));

    {
        // IMPORTANT: use two separate clones so each closure owns its own Rc
        let count_src = count.clone();
        let count_cb = count.clone();
        let seen_cb = seen.clone();

        watch::<i32, _, _>(
            move || count_src.get(),
            move |new, _old| {
                seen_cb.borrow_mut().push(*new);
                // Mutate inside callback to ensure no borrow/move conflicts
                if *new < 3 {
                    count_cb.set(*new + 1);
                }
            },
        );
    }

    // Kick off the chain
    count.set(1);

    // We should see 1, then 2, then 3 (as the callback increments)
    assert_eq!(&*seen.borrow(), &vec![1, 2, 3]);
}
