pub mod ergonomics;
pub mod lifecycle;
pub mod next_tick;
pub mod provide_inject;
pub mod ref_cell;
pub mod signal;
pub mod watch;

// Re-export ergonomics for convenient access
pub use ergonomics::*;

/// Create a Signal with an initial value.
/// Usage: `let count = signal!(count = 0);`
#[macro_export]
macro_rules! signal {
    ($name:ident = $value:expr) => {
        std::rc::Rc::new($crate::signal::Signal::new($value))
    };
}

/// Create a Vue-like reactive reference.
/// Usage: `let count = ref!(0);`
/// Expands to: `velox_core::ref_value(0)`
#[macro_export]
macro_rules! r#ref {
    ($value:expr) => {
        $crate::ergonomics::ref_value($value)
    };
}

/// Create a shallow reactive reference for Copy types.
/// Usage: `let count = shallow!(0);`
#[macro_export]
macro_rules! shallow {
    ($value:expr) => {
        $crate::ergonomics::shallow_ref($value)
    };
}
