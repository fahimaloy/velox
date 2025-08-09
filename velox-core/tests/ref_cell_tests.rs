// velox-core/tests/ref_cell_tests.rs
use velox_core::ref_cell::RefCell;

#[test]
fn test_ref_cell() {
    let r = RefCell::new(5);
    assert_eq!(*r.get(), 5);
    r.set(10);
    assert_eq!(*r.get(), 10);
}

