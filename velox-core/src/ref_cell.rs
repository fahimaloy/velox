// velox-core/src/ref_cell.rs
use std::cell::RefCell as StdRefCell;

/// Simple RefCell wrapper for template refs
pub struct RefCell<T> {
    inner: StdRefCell<T>,
}

impl<T> RefCell<T> {
    /// Create a new RefCell
    pub fn new(value: T) -> Self {
        RefCell {
            inner: StdRefCell::new(value),
        }
    }

    /// Borrow the value immutably
    pub fn get(&self) -> std::cell::Ref<'_, T> {
        self.inner.borrow()
    }

    /// Borrow the value mutably and set
    pub fn set(&self, value: T) {
        *self.inner.borrow_mut() = value;
    }
}

