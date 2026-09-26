//! `VModel` for the `Ref<T>` ergonomics wrapper.
//!
//! The generated `v-model` setter is `VModel::vmodel_set(&self.<field>, payload)`
//! — a fully-qualified call, which does not auto-deref — so `Ref<T>` needs its own
//! impl even though it wraps a `Signal<T>`. These tests drive the real
//! `vmodel_set` and assert observed values, including that an unparseable
//! payload behaves the way it does for the `Signal` and `Cell` impls.

use std::cell::RefCell as StdRefCell;
use std::rc::Rc;

use velox_core::ergonomics::Ref;
use velox_core::signal::Signal;
use velox_core::vmodel::{VModel, apply_str};

#[test]
fn a_ref_string_takes_the_payload_a_generated_setter_sends() {
    let label = Ref::new(String::from("counter"));

    // The shape a `v-model="label"` setter emits, called as codegen calls it.
    VModel::vmodel_set(&label, "counterR2");

    assert_eq!(label.get(), "counterR2");
}

#[test]
fn a_ref_number_parses_the_payload() {
    let count = Ref::new(0);

    VModel::vmodel_set(&count, "42");

    assert_eq!(count.get(), 42);
}

#[test]
fn a_ref_leaves_the_default_when_the_payload_does_not_parse_like_a_signal_or_a_cell() {
    let from_ref = Ref::new(7);
    let from_signal = Signal::new(7);
    let from_cell = std::cell::Cell::new(7);

    VModel::vmodel_set(&from_ref, "not a number");
    VModel::vmodel_set(&from_signal, "not a number");
    VModel::vmodel_set(&from_cell, "not a number");

    assert_eq!(
        from_ref.get(),
        0,
        "an unparseable payload must leave the default"
    );
    assert_eq!(
        from_ref.get(),
        from_signal.get(),
        "Ref<T> must behave like Signal<T> on an unparseable payload"
    );
    assert_eq!(
        from_ref.get(),
        from_cell.get(),
        "Ref<T> must behave like Cell<T> on an unparseable payload"
    );
}

#[test]
fn a_ref_write_notifies_subscribers_because_it_goes_through_the_signal() {
    use velox_core::watch::{WatchOptions, watch};

    let label = Ref::new(String::from("counter"));
    let seen: Rc<StdRefCell<Vec<String>>> = Rc::new(StdRefCell::new(Vec::new()));

    let _handle = {
        let source = label.clone();
        let events = seen.clone();
        watch(
            move || source.get(),
            move |new, _old| events.borrow_mut().push(new),
            WatchOptions::default(),
        )
    };

    assert!(seen.borrow().is_empty(), "no callback on the initial run");

    VModel::vmodel_set(&label, "counterR");

    assert_eq!(
        &*seen.borrow(),
        &vec![String::from("counterR")],
        "a v-model write must notify subscribers, so it has to reach Signal::set"
    );
}

#[test]
fn apply_str_reaches_a_ref_too() {
    let label = Ref::new(String::from("counter"));

    apply_str(&label, "hi");

    assert_eq!(label.get(), "hi");
}

#[test]
fn a_ref_and_its_inner_signal_stay_the_same_value() {
    let label = Ref::new(String::from("counter"));

    VModel::vmodel_set(&label, "shared");

    // `Ref` wraps `Rc<Signal<T>>`, so a clone observes the write. This is the
    // property the impl inherits by delegating to `Ref::set`.
    let clone = label.clone();
    assert_eq!(clone.get(), "shared");
}
