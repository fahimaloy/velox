// velox-core/src/ref_cell.rs
use std::cell::RefCell as StdRefCell;

/// Template reference wrapper for accessing DOM elements after render.
/// This is distinct from std::cell::RefCell to avoid naming conflicts.
pub struct TemplateRef<T> {
    inner: StdRefCell<Option<T>>,
}

impl<T> TemplateRef<T> {
    /// Create a new TemplateRef with no value (None)
    pub fn new() -> Self {
        TemplateRef {
            inner: StdRefCell::new(None),
        }
    }

    /// Create a new TemplateRef with an initial value
    pub fn with_value(value: T) -> Self {
        TemplateRef {
            inner: StdRefCell::new(Some(value)),
        }
    }

    /// Get a reference to the value if set
    pub fn get(&self) -> std::cell::Ref<'_, Option<T>> {
        self.inner.borrow()
    }

    /// Set the template reference value (called by renderer on mount)
    pub fn set(&self, value: T) {
        *self.inner.borrow_mut() = Some(value);
    }

    /// Clear the reference (called by renderer on unmount)
    pub fn clear(&self) {
        *self.inner.borrow_mut() = None;
    }
}

impl<T> Default for TemplateRef<T> {
    fn default() -> Self {
        Self::new()
    }
}
