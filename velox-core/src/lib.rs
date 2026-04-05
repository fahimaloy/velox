pub mod lifecycle;
pub mod ref_cell;
pub mod signal;
pub mod watch;

/// Create a Signal with an initial value.
/// Usage: `let count = signal!(count = 0);`
#[macro_export]
macro_rules! signal {
    ($name:ident = $value:expr) => {
        std::rc::Rc::new($crate::signal::Signal::new($value))
    };
}
