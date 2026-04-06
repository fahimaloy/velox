use std::cell::RefCell as StdRefCell;
use std::rc::Rc;
use velox_core::signal::{Signal, effect};

// =============================================================================
// Signal edge cases
// =============================================================================

#[test]
fn signal_string_type() {
    let s = Rc::new(Signal::new(String::from("hello")));
    let observed = Rc::new(StdRefCell::new(String::new()));
    {
        let s_clone = s.clone();
        let observed_clone = observed.clone();
        effect(move || {
            *observed_clone.borrow_mut() = s_clone.get();
        });
    }
    assert_eq!(*observed.borrow(), "hello");
    s.set(String::from("world"));
    assert_eq!(*observed.borrow(), "world");
}

#[test]
fn signal_bool_type() {
    let s = Rc::new(Signal::new(false));
    let observed = Rc::new(StdRefCell::new(false));
    {
        let s_clone = s.clone();
        let observed_clone = observed.clone();
        effect(move || {
            *observed_clone.borrow_mut() = s_clone.get();
        });
    }
    assert!(!*observed.borrow());
    s.set(true);
    assert!(*observed.borrow());
}

#[test]
fn signal_multiple_effects_same_signal() {
    let s = Rc::new(Signal::new(0));
    let a = Rc::new(StdRefCell::new(0));
    let b = Rc::new(StdRefCell::new(0));
    {
        let s_clone = s.clone();
        let a_clone = a.clone();
        effect(move || {
            *a_clone.borrow_mut() = s_clone.get() * 2;
        });
    }
    {
        let s_clone = s.clone();
        let b_clone = b.clone();
        effect(move || {
            *b_clone.borrow_mut() = s_clone.get() * 3;
        });
    }
    assert_eq!(*a.borrow(), 0);
    assert_eq!(*b.borrow(), 0);
    s.set(5);
    assert_eq!(*a.borrow(), 10);
    assert_eq!(*b.borrow(), 15);
}

#[test]
fn signal_effect_reads_multiple_signals() {
    let a = Rc::new(Signal::new(1));
    let b = Rc::new(Signal::new(2));
    let sum = Rc::new(StdRefCell::new(0));
    {
        let a_clone = a.clone();
        let b_clone = b.clone();
        let sum_clone = sum.clone();
        effect(move || {
            *sum_clone.borrow_mut() = a_clone.get() + b_clone.get();
        });
    }
    assert_eq!(*sum.borrow(), 3);
    a.set(10);
    assert_eq!(*sum.borrow(), 12);
    b.set(20);
    assert_eq!(*sum.borrow(), 30);
}

#[test]
fn signal_effect_does_not_fire_on_unread_signal() {
    let s1 = Rc::new(Signal::new(1));
    let s2 = Rc::new(Signal::new(2));
    let count = Rc::new(StdRefCell::new(0));
    {
        let s1_clone = s1.clone();
        let count_clone = count.clone();
        effect(move || {
            // Only read s1
            let _ = s1_clone.get();
            *count_clone.borrow_mut() += 1;
        });
    }
    assert_eq!(*count.borrow(), 1);
    // Changing s2 should NOT trigger the effect
    s2.set(99);
    assert_eq!(*count.borrow(), 1);
    // Changing s1 SHOULD trigger
    s1.set(2);
    assert_eq!(*count.borrow(), 2);
}

#[test]
fn signal_functional_update() {
    let s = Rc::new(Signal::new(0));
    s.update(|v| v + 1);
    assert_eq!(s.get(), 1);
    s.update(|v| v * 10);
    assert_eq!(s.get(), 10);
    s.update(|_| 42);
    assert_eq!(s.get(), 42);
}

#[test]
fn signal_update_triggers_effects() {
    let s = Rc::new(Signal::new(0));
    let count = Rc::new(StdRefCell::new(0));
    {
        let s_clone = s.clone();
        let count_clone = count.clone();
        effect(move || {
            let _ = s_clone.get();
            *count_clone.borrow_mut() += 1;
        });
    }
    assert_eq!(*count.borrow(), 1);
    s.update(|v| v + 1);
    assert_eq!(*count.borrow(), 2);
}

#[test]
fn signal_nested_effect_subscription() {
    // An effect that creates another effect
    let outer = Rc::new(Signal::new(0));
    let inner = Rc::new(Signal::new(0));
    let outer_count = Rc::new(StdRefCell::new(0));
    {
        let outer_clone = outer.clone();
        let inner_clone = inner.clone();
        let outer_count_clone = outer_count.clone();
        effect(move || {
            let _ = outer_clone.get();
            *outer_count_clone.borrow_mut() += 1;
            // This inner effect subscribes to inner signal
            let inner_clone2 = inner_clone.clone();
            effect(move || {
                let _ = inner_clone2.get();
            });
        });
    }
    assert_eq!(*outer_count.borrow(), 1);
    outer.set(1);
    assert_eq!(*outer_count.borrow(), 2);
    // Inner signal should also trigger the inner effect
    inner.set(1);
}

#[test]
fn signal_same_value_still_triggers() {
    // The signal set() method always notifies subscribers, even if value is same
    let s = Rc::new(Signal::new(5));
    let count = Rc::new(StdRefCell::new(0));
    {
        let s_clone = s.clone();
        let count_clone = count.clone();
        effect(move || {
            let _ = s_clone.get();
            *count_clone.borrow_mut() += 1;
        });
    }
    assert_eq!(*count.borrow(), 1);
    // Setting the same value still triggers (signal does not do equality check)
    s.set(5);
    assert_eq!(*count.borrow(), 2);
}

#[test]
fn signal_vec_type() {
    let s = Rc::new(Signal::new(vec![1, 2, 3]));
    let len = Rc::new(StdRefCell::new(0));
    {
        let s_clone = s.clone();
        let len_clone = len.clone();
        effect(move || {
            *len_clone.borrow_mut() = s_clone.get().len();
        });
    }
    assert_eq!(*len.borrow(), 3);
    s.set(vec![1, 2, 3, 4, 5]);
    assert_eq!(*len.borrow(), 5);
}

#[test]
fn signal_option_type() {
    let s = Rc::new(Signal::new(Some(42)));
    let val = Rc::new(StdRefCell::new(None));
    {
        let s_clone = s.clone();
        let val_clone = val.clone();
        effect(move || {
            *val_clone.borrow_mut() = s_clone.get();
        });
    }
    assert_eq!(*val.borrow(), Some(42));
    s.set(None);
    assert_eq!(*val.borrow(), None);
}

// =============================================================================
// Effect edge cases
// =============================================================================

#[test]
fn effect_runs_immediately() {
    let executed = Rc::new(StdRefCell::new(false));
    {
        let executed_clone = executed.clone();
        effect(move || {
            *executed_clone.borrow_mut() = true;
        });
    }
    assert!(*executed.borrow());
}

#[test]
fn effect_empty_closure() {
    // Should not panic
    effect(|| {});
}

#[test]
fn effect_no_signal_read_no_re_run() {
    // An effect that doesn't read any signal shouldn't re-run on any set()
    let count = Rc::new(StdRefCell::new(0));
    {
        let count_clone = count.clone();
        effect(move || {
            *count_clone.borrow_mut() += 1;
        });
    }
    assert_eq!(*count.borrow(), 1);
    // Since no signals were read, setting any signal shouldn't trigger
    let unrelated = Signal::new(0);
    unrelated.set(1);
    assert_eq!(*count.borrow(), 1);
}

#[test]
fn effect_multiple_set_in_same_scope() {
    let a = Rc::new(Signal::new(0));
    let b = Rc::new(Signal::new(0));
    let count = Rc::new(StdRefCell::new(0));
    {
        let a_clone = a.clone();
        let b_clone = b.clone();
        let count_clone = count.clone();
        effect(move || {
            let _ = a_clone.get();
            let _ = b_clone.get();
            *count_clone.borrow_mut() += 1;
        });
    }
    assert_eq!(*count.borrow(), 1);
    // Setting both in quick succession
    a.set(1);
    b.set(1);
    // Each set triggers the effect once
    assert_eq!(*count.borrow(), 3);
}

#[test]
fn signal_macro_creates_rc() {
    use velox_core::signal;
    let count = signal!(count = 0);
    assert_eq!(count.get(), 0);
    count.set(10);
    assert_eq!(count.get(), 10);
}
