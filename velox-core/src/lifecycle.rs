// velox-core/src/lifecycle.rs
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

/// A unique identifier for a component instance
type ComponentId = usize;

thread_local! {
    #[allow(clippy::missing_const_for_thread_local)]
    static NEXT_COMPONENT_ID: RefCell<usize> = RefCell::new(1);
    #[allow(clippy::type_complexity, clippy::missing_const_for_thread_local)]
    static MOUNTED_HOOKS: RefCell<HashMap<ComponentId, Vec<Box<dyn FnOnce()>>>> = RefCell::new(HashMap::new());
    #[allow(clippy::type_complexity, clippy::missing_const_for_thread_local)]
    static DESTROY_HOOKS: RefCell<HashMap<ComponentId, Vec<Box<dyn FnOnce()>>>> = RefCell::new(HashMap::new());
    #[allow(clippy::missing_const_for_thread_local)]
    static CURRENT_COMPONENT: RefCell<Option<ComponentId>> = RefCell::new(None);
}

/// Generate a new unique component ID
pub fn generate_component_id() -> ComponentId {
    NEXT_COMPONENT_ID.with(|id| {
        let current = *id.borrow();
        *id.borrow_mut() = current + 1;
        current
    })
}

/// Set the current component context for registering hooks
pub fn set_current_component(id: ComponentId) {
    CURRENT_COMPONENT.with(|c| {
        *c.borrow_mut() = Some(id);
    });
}

/// Clear the current component context
pub fn clear_current_component() {
    CURRENT_COMPONENT.with(|c| {
        *c.borrow_mut() = None;
    });
}

/// Get the current component ID if any
pub fn current_component_id() -> Option<ComponentId> {
    CURRENT_COMPONENT.with(|c| *c.borrow())
}

/// Register a hook to run when the current component is mounted
pub fn on_mounted(f: impl FnOnce() + 'static) {
    CURRENT_COMPONENT.with(|c| {
        if let Some(id) = *c.borrow() {
            MOUNTED_HOOKS.with(|h| {
                h.borrow_mut().entry(id).or_default().push(Box::new(f));
            });
        } else {
            log::warn!(
                "on_mounted called without a current component context. \
                 Did you forget to call set_current_component()? The hook will not be registered."
            );
        }
    });
}

/// Internal: run all mounted hooks for a specific component and clear them
pub fn run_mounted_hooks(id: ComponentId) {
    MOUNTED_HOOKS.with(|h| {
        if let Some(hooks) = h.borrow_mut().remove(&id) {
            for hook in hooks {
                hook();
            }
        }
    });
}

/// Register a hook to run before a component is destroyed
pub fn before_destroy(f: impl FnOnce() + 'static) {
    CURRENT_COMPONENT.with(|c| {
        if let Some(id) = *c.borrow() {
            DESTROY_HOOKS.with(|h| {
                h.borrow_mut().entry(id).or_default().push(Box::new(f));
            });
        } else {
            log::warn!(
                "before_destroy called without a current component context. \
                 Did you forget to call set_current_component()? The hook will not be registered."
            );
        }
    });
}

/// Internal: run all destroy hooks for a specific component and clear them
pub fn run_destroy_hooks(id: ComponentId) {
    DESTROY_HOOKS.with(|h| {
        if let Some(hooks) = h.borrow_mut().remove(&id) {
            for hook in hooks {
                hook();
            }
        }
    });
}

/// Register a hook to run when the current component is unmounted.
///
/// This is an alias for [`before_destroy`] so the lifecycle API reads uniformly
/// (`on_mounted` → `on_updated` → `on_unmounted`). Registered hooks run once,
/// right before the component is destroyed.
pub fn on_unmounted(f: impl FnOnce() + 'static) {
    before_destroy(f);
}

/// Internal: run all unmounted hooks for a specific component and clear them.
/// Alias for [`run_destroy_hooks`].
pub fn run_unmounted_hooks(id: ComponentId) {
    run_destroy_hooks(id);
}

// ===== on_updated =====
//
// Unlike `on_mounted`/`on_unmounted` (which fire once and are then removed),
// an `on_updated` hook must stay registered so it runs again on every update
// (re-render). It therefore lives in its own storage and is re-runnable (`FnMut`).

type UpdatedHook = dyn FnMut();

thread_local! {
    #[allow(clippy::type_complexity, clippy::missing_const_for_thread_local)]
    static UPDATED_HOOKS: RefCell<HashMap<ComponentId, Vec<Box<UpdatedHook>>>> = RefCell::new(HashMap::new());
}

/// Register a hook to run whenever the current component is updated (re-rendered).
///
/// The hook is re-runnable and stays registered until [`cleanup_component`] is
/// called (e.g. when the component is unmounted), so it fires on every update.
pub fn on_updated(f: impl FnMut() + 'static) {
    CURRENT_COMPONENT.with(|c| {
        if let Some(id) = *c.borrow() {
            UPDATED_HOOKS.with(|h| {
                h.borrow_mut().entry(id).or_default().push(Box::new(f));
            });
        } else {
            log::warn!(
                "on_updated called without a current component context. \
                 Did you forget to call set_current_component()? The hook will not be registered."
            );
        }
    });
}

/// Internal: run all updated hooks for a specific component.
/// Unlike mounted/destroy hooks, updated hooks are NOT removed — they fire on
/// every update. Run against a length snapshot to stay correct even if a hook
/// registers another `on_updated` while it runs.
pub fn run_updated_hooks(id: ComponentId) {
    UPDATED_HOOKS.with(|h| {
        let mut guard = h.borrow_mut();
        if let Some(hooks) = guard.get_mut(&id) {
            let len = hooks.len();
            for i in 0..len {
                if let Some(hook) = hooks.get_mut(i) {
                    hook();
                }
            }
        }
    });
}

/// Internal: clear all updated hooks for a component.
pub fn clear_updated_hooks(id: ComponentId) {
    UPDATED_HOOKS.with(|h| {
        h.borrow_mut().remove(&id);
    });
}

/// Check whether a component has any registered `on_updated` hooks.
pub fn has_updated_hooks(id: ComponentId) -> bool {
    UPDATED_HOOKS.with(|h| h.borrow().get(&id).is_some_and(|hooks| !hooks.is_empty()))
}

/// Run *all* queued `on_mounted` hooks that have not yet fired.
///
/// Mounted hooks are removed after they run, so this call is idempotent — the
/// renderer may safely call it at the top of every `RedrawRequested` frame.
pub fn run_all_mounted_hooks() {
    let ids: Vec<ComponentId> = MOUNTED_HOOKS.with(|h| h.borrow().keys().copied().collect());
    for id in ids {
        run_mounted_hooks(id);
    }
}

/// Run all `on_updated` hooks for every mounted component.
pub fn run_all_updated_hooks() {
    let ids: Vec<ComponentId> = UPDATED_HOOKS.with(|h| h.borrow().keys().copied().collect());
    for id in ids {
        run_updated_hooks(id);
    }
}

/// Run (and remove) all `before_destroy` / `on_unmounted` hooks for every
/// component and clear their `on_updated` hooks. The renderer calls this from
/// `CloseRequested` (window close) and from the drop guard around the event
/// loop.
pub fn run_all_destroy_hooks() {
    let ids: Vec<ComponentId> = DESTROY_HOOKS.with(|h| h.borrow().keys().copied().collect());
    for id in ids {
        run_destroy_hooks(id);
    }
    UPDATED_HOOKS.with(|h| h.borrow_mut().clear());
    MOUNTED_HOOKS.with(|h| h.borrow_mut().clear());
    // `DESTROY_HOOKS` entries were already removed by `run_destroy_hooks`; the
    // clear above covers cases where cleanup_component was not yet called.
}

/// Clean up all hooks for a component (call when component is fully destroyed)
pub fn cleanup_component(id: ComponentId) {
    MOUNTED_HOOKS.with(|h| {
        h.borrow_mut().remove(&id);
    });
    DESTROY_HOOKS.with(|h| {
        h.borrow_mut().remove(&id);
    });
    UPDATED_HOOKS.with(|h| {
        h.borrow_mut().remove(&id);
    });
}

/// RAII handle that owns a component id and runs destroy+cleanup on drop.
///
/// The renderer creates one per mounted app and keeps it alive for the
/// duration of the event loop so `on_unmounted`/`before_destroy` fire even when
/// the loop exits via `Drop` rather than an explicit `CloseRequested`.
pub struct LifecycleHandle {
    id: ComponentId,
    mounted: Cell<bool>,
}

impl LifecycleHandle {
    /// Allocate a fresh component id and enter its registration context.
    #[must_use]
    pub fn new() -> Self {
        let id = generate_component_id();
        set_current_component(id);
        Self {
            id,
            mounted: Cell::new(false),
        }
    }

    /// Wrap an existing `id` (the id is *not* re-entered into the current
    /// context; call `set_current_component` if hook registration is needed).
    #[must_use]
    pub fn with_id(id: ComponentId) -> Self {
        Self {
            id,
            mounted: Cell::new(false),
        }
    }

    pub fn id(&self) -> ComponentId {
        self.id
    }

    /// Run mounted hooks once. Safe to call redundantly.
    pub fn mount(&self) {
        if !self.mounted.get() {
            self.mounted.set(true);
            run_mounted_hooks(self.id);
        }
    }

    /// True once [`mount`](Self::mount) has been called.
    pub fn is_mounted(&self) -> bool {
        self.mounted.get()
    }
}

impl Default for LifecycleHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for LifecycleHandle {
    fn drop(&mut self) {
        run_destroy_hooks(self.id);
        cleanup_component(self.id);
    }
}

// ===== Lifecycle macros =====
//
// These are `#[macro_export]`, so they live at the crate root and are callable
// from anywhere with a path, e.g. `velox_core::on_mounted!{ ... }` — exactly how
// an SFC `<script setup>` block (which is embedded verbatim into a generated
// module) would call them. Importing `use velox_core::on_mounted;` also works.
//
// Each macro accepts either a single expression (a closure) or a block body;
// a block is wrapped in a `move ||` closure for you.

/// Register a hook to run when the current component is mounted.
///
/// Pass a block body or a closure:
/// ```rust,ignore
/// velox_core::lifecycle::set_current_component(velox_core::lifecycle::generate_component_id());
/// velox_core::on_mounted! { { println!("mounted"); } }
/// velox_core::on_mounted!(|| println!("also mounted"));
/// ```
#[macro_export]
macro_rules! on_mounted {
    ({ $($body:tt)* }) => {
        $crate::lifecycle::on_mounted(move || { $($body)* })
    };
    ($f:expr) => {
        $crate::lifecycle::on_mounted($f)
    };
}

/// Register a hook to run whenever the current component is updated (re-rendered).
///
/// Register a hook to run whenever the current component is updated (re-rendered).
///
/// ```rust,ignore
/// velox_core::lifecycle::set_current_component(velox_core::lifecycle::generate_component_id());
/// velox_core::on_updated! { { println!("updated"); } }
/// ```
#[macro_export]
macro_rules! on_updated {
    ({ $($body:tt)* }) => {
        $crate::lifecycle::on_updated(move || { $($body)* })
    };
    ($f:expr) => {
        $crate::lifecycle::on_updated($f)
    };
}

/// Register a hook to run when the current component is unmounted.
///
/// Register a hook to run when the current component is unmounted.
///
/// ```rust,ignore
/// velox_core::lifecycle::set_current_component(velox_core::lifecycle::generate_component_id());
/// velox_core::on_unmounted! { { println!("unmounted"); } }
/// ```
#[macro_export]
macro_rules! on_unmounted {
    ({ $($body:tt)* }) => {
        $crate::lifecycle::on_unmounted(move || { $($body)* })
    };
    ($f:expr) => {
        $crate::lifecycle::on_unmounted($f)
    };
}
