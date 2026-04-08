use std::rc::Rc;
use velox_core::signal::{computed, Signal};

#[test]
fn computed_derives_value_from_source() {
    let source = Rc::new(Signal::new(10));
    let comp = computed({
        let source = source.clone();
        move || source.get() * 2
    });
    
    assert_eq!(comp.get(), 20);
}

#[test]
fn computed_updates_when_source_changes() {
    let source = Rc::new(Signal::new(5));
    let comp = computed({
        let source = source.clone();
        move || source.get() * 3
    });
    
    assert_eq!(comp.get(), 15);
    
    source.set(10);
    assert_eq!(comp.get(), 30);
}

#[test]
fn computed_with_multiple_dependencies() {
    let a = Rc::new(Signal::new(2));
    let b = Rc::new(Signal::new(3));
    let comp = computed({
        let a = a.clone();
        let b = b.clone();
        move || a.get() + b.get()
    });
    
    assert_eq!(comp.get(), 5);
    
    a.set(10);
    assert_eq!(comp.get(), 13);
    
    b.set(20);
    assert_eq!(comp.get(), 30);
}

#[test]
fn computed_chained() {
    let base = Rc::new(Signal::new(1));
    let double = computed({
        let base = base.clone();
        move || base.get() * 2
    });
    let quadruple = computed({
        let double = double.clone();
        move || double.get() * 2
    });
    
    assert_eq!(quadruple.get(), 4);
    
    base.set(5);
    assert_eq!(double.get(), 10);
    assert_eq!(quadruple.get(), 20);
}
