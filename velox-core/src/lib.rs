pub mod ergonomics;
pub mod lifecycle;
pub mod next_tick;
pub mod provide_inject;
pub mod ref_cell;
pub mod signal;
pub mod vmodel;
pub mod watch;

// Re-export ergonomics for convenient access
pub use ergonomics::*;

// Re-export lifecycle hooks for convenient access.
// Each `on_*` function pairs with a same-named `#[macro_export]` macro at the
// crate root (e.g. `velox_core::on_mounted!`), so SFC `<script setup>` blocks
// can call either the function or macro form.
pub use lifecycle::{
    before_destroy, cleanup_component, clear_current_component, clear_updated_hooks,
    current_component_id, generate_component_id, has_updated_hooks, on_mounted, on_updated,
    on_unmounted, run_destroy_hooks, run_mounted_hooks, run_updated_hooks,
    run_unmounted_hooks, set_current_component,
};

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
