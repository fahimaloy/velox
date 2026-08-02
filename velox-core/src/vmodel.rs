use std::cell::{Cell, RefCell};
use std::str::FromStr;

use crate::signal::Signal;

/// Trait for v-model two-way binding support.
///
/// Generated v-model setters (`__vmodel_set_*`) call `VModel::vmodel_set`
/// so they work with `Cell<T>`, `RefCell<String>`, and `Signal<T>`.
pub trait VModel {
    /// Set the value from a string payload (e.g., input event value).
    fn vmodel_set(&self, payload: &str);
}

impl<T: FromStr + Default> VModel for Cell<T> {
    fn vmodel_set(&self, payload: &str) {
        self.set(payload.parse().unwrap_or_default());
    }
}

impl VModel for RefCell<String> {
    fn vmodel_set(&self, payload: &str) {
        self.replace(payload.to_string());
    }
}

impl<T: FromStr + Default + Clone> VModel for Signal<T> {
    fn vmodel_set(&self, payload: &str) {
        self.set(payload.parse().unwrap_or_default());
    }
}

/// Convenience function. Equivalent to `target.vmodel_set(payload)`.
pub fn apply_str(target: &impl VModel, payload: &str) {
    target.vmodel_set(payload);
}
