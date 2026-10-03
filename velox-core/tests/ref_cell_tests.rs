// velox-core/tests/ref_cell_tests.rs
use velox_core::ref_cell::TemplateRef;

#[test]
fn test_template_ref() {
    let r = TemplateRef::new();
    assert!(r.get().is_none());
    r.set(5);
    assert_eq!(*r.get(), Some(5));
    r.clear();
    assert!(r.get().is_none());
}

#[test]
fn test_template_ref_with_value() {
    let r = TemplateRef::with_value(10);
    assert_eq!(*r.get(), Some(10));
}

#[test]
fn test_template_ref_default() {
    let r: TemplateRef<i32> = TemplateRef::default();
    assert!(r.get().is_none());
}
