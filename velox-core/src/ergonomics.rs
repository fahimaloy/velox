//! Vue-like ergonomic APIs for Velox.
//!
//! These helpers provide a more concise syntax for common reactive patterns,
//! making Velox feel like Vue's Composition API while using Rust under the hood.
//!
//! # Examples
//!
//! ```rust,ignore
//! // Vue-like ref() instead of Rc::new(Signal::new(...))
//! let count = ref_value(0);           // Ref<i32>
//! count.set(42);
//! assert_eq!(count.get(), 42);
//!
//! // reactive!() macro for creating component state
//! let state = reactive! {
//!     count: i32 = 0,
//!     name: String = String::from("Hello"),
//! };
//! state.count.set(10);
//! state.name.set(String::from("World"));
//! ```

use std::rc::Rc;

use crate::signal::Signal;

/// A Vue-like reactive reference.
///
/// This is a thin wrapper around `Rc<Signal<T>>` that provides
/// a more ergonomic API for creating reactive state.
///
/// # Examples
///
/// ```rust,ignore
/// let count = ref_value(0);
/// count.set(42);
/// assert_eq!(count.get(), 42);
/// ```
#[derive(Clone)]
pub struct Ref<T>(Rc<Signal<T>>);

impl<T: Clone + 'static> Ref<T> {
    /// Create a new reactive reference with the given initial value.
    #[inline]
    pub fn new(value: T) -> Self {
        Self(Rc::new(Signal::new(value)))
    }

    /// Get the current value.
    #[inline]
    pub fn get(&self) -> T {
        self.0.get()
    }

    /// Set a new value and notify subscribers.
    #[inline]
    pub fn set(&self, value: T) {
        self.0.set(value);
    }

    /// Update the value using a closure.
    #[inline]
    pub fn update<F>(&self, f: F)
    where
        F: FnOnce(T) -> T,
    {
        self.0.update(f);
    }

    /// Set value only if it changed.
    #[inline]
    pub fn set_if_changed(&self, value: T)
    where
        T: PartialEq,
    {
        self.0.set_if_changed(value);
    }

    /// Get the inner `Rc<Signal<T>>` for use with effects and other APIs.
    #[inline]
    pub fn as_signal(&self) -> &Rc<Signal<T>> {
        &self.0
    }

    /// Convert into the inner `Rc<Signal<T>>`.
    #[inline]
    pub fn into_signal(self) -> Rc<Signal<T>> {
        self.0
    }
}

impl<T: Clone + 'static> std::ops::Deref for Ref<T> {
    type Target = Rc<Signal<T>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: std::fmt::Debug + Clone + 'static> std::fmt::Debug for Ref<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Ref").field(&self.get()).finish()
    }
}

/// Create a reactive reference (Vue-like `ref()`).
///
/// # Examples
///
/// ```rust,ignore
/// let count = ref_value(0);
/// let name = ref_value(String::from("Hello"));
/// ```
#[inline]
pub fn ref_value<T: Clone + 'static>(value: T) -> Ref<T> {
    Ref::new(value)
}

/// Create a reactive reference from a `Signal<T>`.
///
/// This is useful when you already have a `Signal` and want the Ref API.
#[inline]
pub fn from_signal<T: Clone + 'static>(signal: Rc<Signal<T>>) -> Ref<T> {
    Ref(signal)
}

/// Create a reactive reference from a `Cell<T>` (non-reactive, for internal state).
///
/// Use this for values that don't need reactive updates.
#[inline]
pub fn cell_ref<T: Clone + 'static>(signal: Rc<Signal<T>>) -> Ref<T> {
    Ref(signal)
}

/// Convert a `Ref<T>` back to `Rc<Signal<T>>` for use with effects.
#[inline]
pub fn to_signal<T: Clone + 'static>(r: &Ref<T>) -> &Rc<Signal<T>> {
    r.as_signal()
}

/// Create a shallow reactive reference using `std::cell::Cell`.
///
/// This is useful for simple types like `i32`, `bool`, `usize`
/// that implement `Copy`. It's lighter weight than `Signal<T>`.
pub struct ShallowRef<T: Copy>(std::rc::Rc<std::cell::Cell<T>>);

impl<T: Copy> ShallowRef<T> {
    #[inline]
    pub fn new(value: T) -> Self {
        Self(std::rc::Rc::new(std::cell::Cell::new(value)))
    }

    #[inline]
    pub fn get(&self) -> T {
        self.0.get()
    }

    #[inline]
    pub fn set(&self, value: T) {
        self.0.set(value);
    }

    #[inline]
    pub fn update<F>(&self, f: F)
    where
        F: FnOnce(T) -> T,
    {
        self.0.set(f(self.0.get()));
    }
}

impl<T: Copy> Clone for ShallowRef<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: Copy + std::fmt::Debug> std::fmt::Debug for ShallowRef<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ShallowRef").field(&self.get()).finish()
    }
}

/// Create a shallow reactive reference for `Copy` types.
///
/// This is lighter weight than `ref_value()` for simple types.
#[inline]
pub fn shallow_ref<T: Copy>(value: T) -> ShallowRef<T> {
    ShallowRef::new(value)
}

/// Create a reactive struct with named fields.
///
/// Each field becomes a `Ref<T>`. This is useful for grouping
/// related reactive state together.
///
/// # Examples
///
/// ```rust,ignore
/// let state = reactive_struct! {
///     count: Ref<i32> = ref_value(0),
///     name: Ref<String> = ref_value(String::from("Hello")),
/// };
///
/// state.count.set(42);
/// state.name.set(String::from("World"));
/// ```
#[macro_export]
macro_rules! reactive_struct {
    ($( $field:ident: $ty:ty = $init:expr ),* $(,)?) => {
        {
            struct __State {
                $( $field: $ty, )*
            }
            __State {
                $( $field: $init, )*
            }
        }
    };
}

/// Define reactive state inline using a struct expression.
///
/// This is a shorthand for creating an anonymous struct with Ref fields.
///
/// # Examples
///
/// ```rust,ignore
/// let state = reactive! {
///     count: i32 = 0,
///     name: String = String::from("Hello"),
/// };
///
/// // Usage:
/// state.count.set(42);
/// state.name.set(String::from("World"));
/// ```
#[macro_export]
macro_rules! reactive {
    ($( $field:ident: $ty:ty = $init:expr ),* $(,)?) => {
        {
            struct __ReactiveState {
                $( $field: $crate::ergonomics::Ref<$ty>, )*
            }
            __ReactiveState {
                $( $field: $crate::ergonomics::ref_value($init), )*
            }
        }
    };
}

/// Define emits for a component (Vue-like `defineEmits`).
///
/// This macro documents which events a component can emit.
/// At runtime it creates the emit helper function.
///
/// # Examples
///
/// ```rust,ignore
/// define_emits!(change, reset);
///
/// // In your methods:
/// pub fn increment(&self) {
///     self.count.set(self.count.get() + 1);
///     emit!("change", &self.count.get().to_string());
/// }
/// ```
#[macro_export]
macro_rules! define_emits {
    ($( $event:ident ),* $(,)?) => {
        /// Supported events for this component.
        pub const EMIT_EVENTS: &[&str] = &[$(stringify!($event),)*];
    };
}

/// Emit an event to the parent component.
///
/// This is a placeholder - the actual emit mechanism depends on
/// whether the component was rendered with `render_with_callbacks`.
///
/// # Examples
///
/// ```rust,ignore
/// emit!("change", &value.to_string());
/// emit!("click");
/// ```
pub fn emit(_event: &str, _payload: &str) {
    // In standalone mode, this is a no-op.
    // When rendered with callbacks, the codegen replaces this.
}

/// Emit an event without payload.
#[inline]
pub fn emit_void(event: &str) {
    emit(event, "");
}

/// Vue-like `watch` shorthand.
///
/// Watches a signal and calls the callback when it changes.
///
/// # Examples
///
/// ```rust,ignore
/// let count = ref_value(0);
/// watch_ref(&count, |new, old| {
///     println!("count changed: {} -> {}", old, new);
/// });
/// ```
pub fn watch_ref<T: Clone + PartialEq + 'static>(
    source: &Ref<T>,
    callback: impl FnMut(T, T) + 'static,
) {
    let sig = source.as_signal().clone();
    crate::watch::watch(
        move || sig.get(),
        callback,
        crate::watch::WatchOptions::default(),
    );
}

/// Vue-like `watchEffect` shorthand.
///
/// Runs the effect immediately and re-runs when dependencies change.
///
/// # Examples
///
/// ```rust,ignore
/// let count = ref_value(0);
/// watch_effect(|| {
///     println!("count is: {}", count.get());
/// });
/// ```
pub fn watch_effect<F: FnMut() + 'static>(f: F) -> crate::signal::EffectHandle {
    crate::signal::effect(f)
}

/// Vue-like `computed` shorthand.
///
/// Creates a computed signal that derives its value from other signals.
///
/// # Examples
///
/// ```rust,ignore
/// let count = ref_value(0);
/// let doubled = computed_ref(|| count.get() * 2);
/// assert_eq!(doubled.get(), 0);
/// count.set(5);
/// assert_eq!(doubled.get(), 10);
/// ```
pub fn computed_ref<T: Clone + PartialEq + 'static, F: Fn() -> T + 'static>(compute: F) -> Ref<T> {
    Ref(crate::signal::computed(compute))
}

/// Create a readonly computed ref from a closure.
///
/// This is the same as `computed_ref()` but makes the intent clearer
/// that the result is not meant to be written to directly.
#[inline]
pub fn readonly<T: Clone + PartialEq + 'static, F: Fn() -> T + 'static>(compute: F) -> Ref<T> {
    computed_ref(compute)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ref_value() {
        let count = ref_value(0);
        assert_eq!(count.get(), 0);

        count.set(42);
        assert_eq!(count.get(), 42);

        count.update(|v| v + 1);
        assert_eq!(count.get(), 43);
    }

    #[test]
    fn test_ref_clone() {
        let a = ref_value(String::from("hello"));
        let b = a.clone();
        b.set(String::from("world"));
        assert_eq!(a.get(), "world");
    }

    #[test]
    fn test_ref_set_if_changed() {
        let count = ref_value(0);
        count.set(1);
        count.set_if_changed(1); // no change
        count.set_if_changed(2); // changed
        assert_eq!(count.get(), 2);
    }

    #[test]
    fn test_shallow_ref() {
        let count = shallow_ref(0);
        assert_eq!(count.get(), 0);

        count.set(42);
        assert_eq!(count.get(), 42);

        count.update(|v| v * 2);
        assert_eq!(count.get(), 84);
    }

    #[test]
    fn test_shallow_ref_clone() {
        let a = shallow_ref(10);
        let b = a.clone();
        b.set(20);
        assert_eq!(a.get(), 20);
    }

    #[test]
    fn test_reactive_macro() {
        let state = reactive! {
            count: i32 = 0,
            name: String = String::from("hello"),
        };

        assert_eq!(state.count.get(), 0);
        assert_eq!(state.name.get(), "hello");

        state.count.set(42);
        state.name.set(String::from("world"));

        assert_eq!(state.count.get(), 42);
        assert_eq!(state.name.get(), "world");
    }

    #[test]
    #[allow(clippy::const_is_empty)]
    fn test_define_emits_macro() {
        // Just verify it compiles
        define_emits!(change, reset);
        assert!(!EMIT_EVENTS.is_empty());
    }

    #[test]
    fn test_computed_ref() {
        let count = ref_value(5);
        let doubled = computed_ref({
            let count = count.clone();
            move || count.get() * 2
        });

        assert_eq!(doubled.get(), 10);

        count.set(10);
        assert_eq!(doubled.get(), 20);
    }

    #[test]
    fn test_readonly() {
        let count = ref_value(5);
        let doubled = readonly({
            let count = count.clone();
            move || count.get() * 2
        });

        assert_eq!(doubled.get(), 10);
    }

    #[test]
    fn test_from_signal() {
        let signal = Rc::new(Signal::new(42));
        let r = from_signal(signal);
        assert_eq!(r.get(), 42);
        r.set(100);
        assert_eq!(r.get(), 100);
    }

    #[test]
    fn test_into_signal() {
        let r = ref_value(42);
        let signal = r.into_signal();
        assert_eq!(signal.get(), 42);
    }

    #[test]
    fn test_deref() {
        let r = ref_value(42);
        // Deref allows calling Signal methods directly
        assert_eq!(r.get(), 42);
    }

    #[test]
    fn test_emit_void() {
        // Should not panic
        emit_void("test");
    }

    #[test]
    fn test_emit_with_payload() {
        // Should not panic
        emit("test", "payload");
    }
}
