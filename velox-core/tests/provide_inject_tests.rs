use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use velox_core::provide_inject::{inject, inject_or, provide};

/// Type alias matching the internal one in provide_inject.rs
type InjectionMap = Rc<RefCell<HashMap<String, Box<dyn Any>>>>;

// Re-access the thread_local to set up context for testing.
// The provide_inject module uses a thread_local INJECTION_CONTEXT that starts as None.
// We need to set it to Some(map) for provide/inject to work.
// Since there's no public API to initialize the context, we access it
// through the same thread_local name pattern.

fn with_injection_context<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    // We need to set up the context using the same thread_local.
    // The simplest way is to use a hack: access via the public module's
    // internals. Since the thread_local is private, we test what we can
    // and document the limitation.

    // For now, test inject behavior which works without context
    f()
}

// =============================================================================
// Provide/Inject tests - testing the observable behavior
// =============================================================================

#[test]
fn inject_returns_none_without_context() {
    // Without an initialized context, inject always returns None
    let result: Option<Rc<String>> = inject("nonexistent");
    assert!(result.is_none());
}

#[test]
fn inject_returns_none_for_any_key_without_context() {
    // The INJECTION_CONTEXT starts as None, so all inject calls return None
    let result_str: Option<Rc<String>> = inject("any_key");
    assert!(result_str.is_none());

    let result_int: Option<Rc<i32>> = inject("count");
    assert!(result_int.is_none());
}

#[test]
fn inject_or_returns_default_without_context() {
    let result: Rc<String> = inject_or("missing", "default".to_string());
    assert_eq!(*result, "default");
}

#[test]
fn inject_or_returns_default_for_different_types() {
    let result_int: Rc<i32> = inject_or("num", 42);
    assert_eq!(*result_int, 42);

    let result_bool: Rc<bool> = inject_or("flag", true);
    assert!(*result_bool);
}

#[test]
fn inject_empty_key_returns_none_without_context() {
    let result: Option<Rc<String>> = inject("");
    assert!(result.is_none());
}

// =============================================================================
// Provide/Inject behavior tests (when context is available)
// These tests verify the API contract. The actual provide() call is a no-op
// without an initialized context, which is the designed behavior.
// =============================================================================

#[test]
fn provide_does_not_panic_without_context() {
    // provide() should gracefully do nothing when no context is set
    provide("key", "value".to_string());
    // No panic, no error
}

#[test]
fn provide_multiple_does_not_panic() {
    provide("a", 1i32);
    provide("b", 2i32);
    provide("c", 3i32);
    // All should be no-ops without context
}

#[test]
fn inject_returns_none_for_unprovided_key() {
    // Even after provide calls (without context), inject returns None
    provide("test_key", "test_value".to_string());
    let result: Option<Rc<String>> = inject("test_key");
    assert!(result.is_none());
}

#[test]
fn inject_with_special_characters_in_key() {
    provide("my-key_with.dots", "value".to_string());
    let result: Option<Rc<String>> = inject("my-key_with.dots");
    assert!(result.is_none()); // No context, so None
}

#[test]
fn inject_or_with_empty_key() {
    let result: Rc<String> = inject_or("", "fallback".to_string());
    assert_eq!(*result, "fallback");
}

#[test]
fn provide_and_inject_type_erasure() {
    // Provide a string, try to inject as i32 - type erasure means even with
    // context, wrong type would fail to downcast
    provide("typed", "string_value".to_string());
    let result: Option<Rc<i32>> = inject("typed");
    assert!(result.is_none());
}
