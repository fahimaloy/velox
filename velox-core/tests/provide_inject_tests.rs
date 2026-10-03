use std::rc::Rc;
use velox_core::provide_inject::{
    clear_injection_context, init_injection_context, inject, inject_or, provide,
    with_injection_context,
};

// =============================================================================
// Provide/Inject tests with initialized context
// =============================================================================

#[test]
fn inject_returns_none_without_context() {
    clear_injection_context();
    let result: Option<Rc<String>> = inject("nonexistent");
    assert!(result.is_none());
}

#[test]
fn provide_and_inject_with_initialized_context() {
    init_injection_context();
    provide("theme", "dark".to_string());

    let result: Option<Rc<String>> = inject("theme");
    assert!(result.is_some());
    assert_eq!(*result.unwrap(), "dark");

    clear_injection_context();
}

#[test]
fn inject_returns_none_for_unprovided_key() {
    init_injection_context();
    provide("some_key", "some_value".to_string());

    let result: Option<Rc<String>> = inject("other_key");
    assert!(result.is_none());

    clear_injection_context();
}

#[test]
fn inject_or_returns_default_when_not_found() {
    init_injection_context();

    let result: Rc<String> = inject_or("missing", "default".to_string());
    assert_eq!(*result, "default");

    clear_injection_context();
}

#[test]
fn inject_or_returns_value_when_found() {
    init_injection_context();
    provide("exists", "provided_value".to_string());

    let result: Rc<String> = inject_or("exists", "default".to_string());
    assert_eq!(*result, "provided_value");

    clear_injection_context();
}

#[test]
fn provide_multiple_values() {
    init_injection_context();
    provide("name", "Velox".to_string());
    provide("version", 1i32);
    provide("debug", true);

    let name: Option<Rc<String>> = inject("name");
    let version: Option<Rc<i32>> = inject("version");
    let debug: Option<Rc<bool>> = inject("debug");

    assert_eq!(*name.unwrap(), "Velox");
    assert_eq!(*version.unwrap(), 1);
    assert!(*debug.unwrap());

    clear_injection_context();
}

#[test]
fn provide_overwrites_existing_value() {
    init_injection_context();
    provide("key", "first".to_string());
    provide("key", "second".to_string());

    let result: Option<Rc<String>> = inject("key");
    assert_eq!(*result.unwrap(), "second");

    clear_injection_context();
}

// =============================================================================
// Hierarchical context tests
// =============================================================================

#[test]
fn child_context_can_access_parent_value() {
    init_injection_context();
    provide("parent_key", "parent_value".to_string());

    let child_result = with_injection_context(|| {
        let parent_value: Option<Rc<String>> = inject("parent_key");
        parent_value
    });

    assert!(child_result.is_some());
    assert_eq!(*child_result.unwrap(), "parent_value");

    clear_injection_context();
}

#[test]
fn child_context_can_override_parent_value() {
    init_injection_context();
    provide("key", "parent".to_string());

    let child_result = with_injection_context(|| {
        provide("key", "child".to_string());
        let value: Option<Rc<String>> = inject("key");
        value
    });

    assert_eq!(*child_result.unwrap(), "child");

    let parent_value: Option<Rc<String>> = inject("key");
    assert_eq!(*parent_value.unwrap(), "parent");

    clear_injection_context();
}

#[test]
fn nested_contexts_stack_correctly() {
    init_injection_context();
    provide("level", "0".to_string());

    let result = with_injection_context(|| {
        provide("level", "1".to_string());
        provide("level1_only", "in_level_1".to_string());

        with_injection_context(|| {
            provide("level", "2".to_string());

            let level2_val: Option<Rc<String>> = inject("level");
            assert_eq!(level2_val.as_ref().unwrap().as_str(), "2");

            let level1_val: Option<Rc<String>> = inject("level1_only");
            assert_eq!(level1_val.as_ref().unwrap().as_str(), "in_level_1");

            level2_val
        })
    });

    assert_eq!(*result.unwrap(), "2");

    let level0_val: Option<Rc<String>> = inject("level");
    assert_eq!(*level0_val.unwrap(), "0");

    clear_injection_context();
}
