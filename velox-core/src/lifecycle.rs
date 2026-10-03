// velox-core/src/lifecycle.rs
//
// All registry state here is thread-local `RefCell` behind `Rc`-free plain
// data, so every hook map is `!Send` by construction — the same
// single-threaded contract as `signal.rs` (no `Arc`/`Mutex`). Every borrow is
// therefore fallible (`try_borrow`/`try_borrow_mut`): user hooks run without
// any registry borrow held, so a hook that registers, runs, or cleans up other
// hooks re-enters these functions instead of panicking with `BorrowMutError`.
// A borrow that still fails means genuine contention further down this same
// stack; run paths skip that pass and registration paths warn and drop the
// hook rather than bringing down the event loop.
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// A unique identifier for a component instance
type ComponentId = usize;

thread_local! {
    static NEXT_COMPONENT_ID: Cell<usize> = const { Cell::new(1) };
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
        let current = id.get();
        id.set(current.saturating_add(1));
        current
    })
}

/// Set the current component context for registering hooks
pub fn set_current_component(id: ComponentId) {
    CURRENT_COMPONENT.with(|c| {
        if let Ok(mut guard) = c.try_borrow_mut() {
            *guard = Some(id);
        }
    });
}

/// Clear the current component context
pub fn clear_current_component() {
    CURRENT_COMPONENT.with(|c| {
        if let Ok(mut guard) = c.try_borrow_mut() {
            *guard = None;
        }
    });
}

/// Get the current component ID if any
///
/// Returns `None` when no context is set *or* the context cell is borrowed
/// further down the stack; hook registration treats both as "no context".
pub fn current_component_id() -> Option<ComponentId> {
    CURRENT_COMPONENT.with(|c| c.try_borrow().ok().and_then(|guard| *guard))
}

/// Register a hook to run when the current component is mounted
pub fn on_mounted(f: impl FnOnce() + 'static) {
    if let Some(id) = current_component_id() {
        MOUNTED_HOOKS.with(|h| {
            if let Ok(mut map) = h.try_borrow_mut() {
                map.entry(id).or_default().push(Box::new(f));
            } else {
                log::warn!(
                    "on_mounted: hook registry is borrowed; the hook was dropped \
                     instead of panicking. This indicates a hook running while \
                     the registry is held, which run paths never do."
                );
            }
        });
    } else {
        log::warn!(
            "on_mounted called without a current component context. \
             Did you forget to call set_current_component()? The hook will not be registered."
        );
    }
}

/// Internal: run all mounted hooks for a specific component and clear them
///
/// The hook vec is removed while borrowed and run after the borrow is
/// released, so a hook may register or run other hooks without panicking. A
/// contended registry skips this pass instead of panicking.
pub fn run_mounted_hooks(id: ComponentId) {
    let hooks = MOUNTED_HOOKS.with(|h| h.try_borrow_mut().ok().and_then(|mut map| map.remove(&id)));
    if let Some(hooks) = hooks {
        for hook in hooks {
            hook();
        }
    }
}

/// Register a hook to run before a component is destroyed
pub fn before_destroy(f: impl FnOnce() + 'static) {
    if let Some(id) = current_component_id() {
        DESTROY_HOOKS.with(|h| {
            if let Ok(mut map) = h.try_borrow_mut() {
                map.entry(id).or_default().push(Box::new(f));
            } else {
                log::warn!(
                    "before_destroy: hook registry is borrowed; the hook was \
                     dropped instead of panicking."
                );
            }
        });
    } else {
        log::warn!(
            "before_destroy called without a current component context. \
             Did you forget to call set_current_component()? The hook will not be registered."
        );
    }
}

/// Internal: run all destroy hooks for a specific component and clear them
///
/// Same remove-then-run discipline as `run_mounted_hooks`: no registry borrow
/// is held across user code, and contention skips the pass.
pub fn run_destroy_hooks(id: ComponentId) {
    let hooks = DESTROY_HOOKS.with(|h| h.try_borrow_mut().ok().and_then(|mut map| map.remove(&id)));
    if let Some(hooks) = hooks {
        for hook in hooks {
            hook();
        }
    }
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
    if let Some(id) = current_component_id() {
        UPDATED_HOOKS.with(|h| {
            if let Ok(mut map) = h.try_borrow_mut() {
                map.entry(id).or_default().push(Box::new(f));
            } else {
                log::warn!(
                    "on_updated: hook registry is borrowed; the hook was \
                     dropped instead of panicking."
                );
            }
        });
    } else {
        log::warn!(
            "on_updated called without a current component context. \
             Did you forget to call set_current_component()? The hook will not be registered."
        );
    }
}

/// Internal: run all updated hooks for a specific component.
/// Unlike mounted/destroy hooks, updated hooks are NOT removed — they fire on
/// every update.
///
/// The vec is taken out of the registry before any hook runs, so a hook that
/// registers another `on_updated` (or triggers a nested update dispatch) does
/// not borrow the map while it is borrowed — the old code held `borrow_mut`
/// across every `hook()` call and panicked on exactly that. After the run the
/// hooks are merged back in front of any newly registered ones, preserving
/// registration order; a hook registered mid-run therefore fires from the
/// next dispatch, matching the old length-snapshot semantics. A contended
/// registry skips this pass instead of panicking.
pub fn run_updated_hooks(id: ComponentId) {
    let Some(mut running) =
        UPDATED_HOOKS.with(|h| h.try_borrow_mut().ok().and_then(|mut map| map.remove(&id)))
    else {
        return;
    };
    let len = running.len();
    for i in 0..len {
        if let Some(hook) = running.get_mut(i) {
            hook();
        }
    }
    UPDATED_HOOKS.with(|h| {
        if let Ok(mut map) = h.try_borrow_mut() {
            match map.get_mut(&id) {
                Some(fresh) => {
                    // Hooks that ran go back in front, in order; anything
                    // registered mid-run stays queued behind them.
                    let mut done = std::mem::take(&mut running);
                    done.append(fresh);
                    *fresh = done;
                }
                None => {
                    map.insert(id, running);
                }
            }
        } else {
            log::warn!(
                "run_updated_hooks: registry is borrowed while merging hooks \
                 back; the component's updated hooks were dropped instead of \
                 panicking."
            );
        }
    });
}

/// Internal: clear all updated hooks for a component.
pub fn clear_updated_hooks(id: ComponentId) {
    UPDATED_HOOKS.with(|h| {
        if let Ok(mut map) = h.try_borrow_mut() {
            map.remove(&id);
        }
    });
}

/// Check whether a component has any registered `on_updated` hooks.
///
/// A contended registry reports `false` rather than panicking.
pub fn has_updated_hooks(id: ComponentId) -> bool {
    UPDATED_HOOKS.with(|h| match h.try_borrow() {
        Ok(map) => map.get(&id).is_some_and(|hooks| !hooks.is_empty()),
        Err(_) => false,
    })
}

// ===== on_resize =====
//
// Resize hooks are re-runnable, like updated hooks, and remain registered until
// the owning component is cleaned up. Each hook is kept behind an `Rc<RefCell<_>>`
// so the registry map can be released before user code runs; this permits a
// hook to register another hook or touch component state without a registry
// borrow panic.

type ResizeHook = dyn FnMut(u32, u32);
type ResizeHookRef = Rc<RefCell<ResizeHook>>;

thread_local! {
    #[allow(clippy::type_complexity, clippy::missing_const_for_thread_local)]
    static RESIZE_HOOKS: RefCell<HashMap<ComponentId, Vec<ResizeHookRef>>> = RefCell::new(HashMap::new());
}

/// Register a hook to run when the current component's viewport changes.
///
/// `width` and `height` are logical/CSS pixels, not physical framebuffer
/// pixels. The hook is re-runnable and is removed by [`cleanup_component`].
pub fn on_resize(f: impl FnMut(u32, u32) + 'static) {
    if let Some(id) = current_component_id() {
        RESIZE_HOOKS.with(|h| {
            if let Ok(mut map) = h.try_borrow_mut() {
                map.entry(id).or_default().push(Rc::new(RefCell::new(f)));
            } else {
                log::warn!(
                    "on_resize: hook registry is borrowed; the hook was \
                     dropped instead of panicking."
                );
            }
        });
    } else {
        log::warn!(
            "on_resize called without a current component context. \
             Did you forget to call set_current_component()? The hook will not be registered."
        );
    }
}

/// Run all registered resize hooks for a committed logical viewport size.
///
/// This public entry point dispatches each call. The renderer owns the
/// change-only baseline per window and calls it only after a committed logical
/// size change, so direct core callers must provide their own dispatch scope.
pub fn run_resize_hooks(width: u32, height: u32) {
    // Clone handles while the registry is borrowed, then drop that borrow
    // before invoking any user code. A newly registered hook is intentionally
    // deferred until the next dispatch. A contended registry dispatches to
    // nothing instead of panicking.
    let hooks: Vec<ResizeHookRef> = RESIZE_HOOKS.with(|h| {
        h.try_borrow()
            .ok()
            .map(|map| {
                map.values()
                    .flat_map(|component_hooks| component_hooks.iter().cloned())
                    .collect()
            })
            .unwrap_or_default()
    });

    for hook in hooks {
        // `try_borrow_mut` keeps a nested dispatch from panicking if a hook
        // re-enters this dispatcher while it is already running. The outer
        // invocation still completes normally, and no registry map guard is
        // held across either call.
        if let Ok(mut callback) = hook.try_borrow_mut() {
            callback(width, height);
        }
    }
}

/// Compatibility name matching the renderer's other global lifecycle helpers.
pub fn run_all_resize_hooks(width: u32, height: u32) {
    run_resize_hooks(width, height);
}

/// Clear all resize hooks for a component (call when component is destroyed).
pub fn clear_resize_hooks(id: ComponentId) {
    RESIZE_HOOKS.with(|h| {
        if let Ok(mut map) = h.try_borrow_mut() {
            map.remove(&id);
        }
    });
}

/// Check whether a component has any registered resize hooks.
///
/// A contended registry reports `false` rather than panicking.
pub fn has_resize_hooks(id: ComponentId) -> bool {
    RESIZE_HOOKS.with(|h| match h.try_borrow() {
        Ok(map) => map.get(&id).is_some_and(|hooks| !hooks.is_empty()),
        Err(_) => false,
    })
}

/// Run *all* queued `on_mounted` hooks that have not yet fired.
///
/// Mounted hooks are removed after they run, so this call is idempotent — the
/// renderer may safely call it at the top of every `RedrawRequested` frame.
/// A contended registry runs nothing instead of panicking.
pub fn run_all_mounted_hooks() {
    let ids: Vec<ComponentId> = MOUNTED_HOOKS.with(|h| {
        h.try_borrow()
            .ok()
            .map(|map| map.keys().copied().collect())
            .unwrap_or_default()
    });
    for id in ids {
        run_mounted_hooks(id);
    }
}

/// Run all `on_updated` hooks for every mounted component.
pub fn run_all_updated_hooks() {
    let ids: Vec<ComponentId> = UPDATED_HOOKS.with(|h| {
        h.try_borrow()
            .ok()
            .map(|map| map.keys().copied().collect())
            .unwrap_or_default()
    });
    for id in ids {
        run_updated_hooks(id);
    }
}

/// Run (and remove) all `before_destroy` / `on_unmounted` hooks for every
/// component and clear their `on_updated` and resize hooks. The renderer calls
/// this from `CloseRequested` (window close) and from the drop guard around the
/// event loop.
pub fn run_all_destroy_hooks() {
    let ids: Vec<ComponentId> = DESTROY_HOOKS.with(|h| {
        h.try_borrow()
            .ok()
            .map(|map| map.keys().copied().collect())
            .unwrap_or_default()
    });
    for id in ids {
        run_destroy_hooks(id);
    }
    UPDATED_HOOKS.with(|h| {
        if let Ok(mut map) = h.try_borrow_mut() {
            map.clear();
        }
    });
    RESIZE_HOOKS.with(|h| {
        if let Ok(mut map) = h.try_borrow_mut() {
            map.clear();
        }
    });
    MOUNTED_HOOKS.with(|h| {
        if let Ok(mut map) = h.try_borrow_mut() {
            map.clear();
        }
    });
    // `DESTROY_HOOKS` entries were already removed by `run_destroy_hooks`; the
    // clear above covers cases where cleanup_component was not yet called.
}

/// Clean up all hooks for a component (call when component is fully destroyed)
pub fn cleanup_component(id: ComponentId) {
    MOUNTED_HOOKS.with(|h| {
        if let Ok(mut map) = h.try_borrow_mut() {
            map.remove(&id);
        }
    });
    DESTROY_HOOKS.with(|h| {
        if let Ok(mut map) = h.try_borrow_mut() {
            map.remove(&id);
        }
    });
    UPDATED_HOOKS.with(|h| {
        if let Ok(mut map) = h.try_borrow_mut() {
            map.remove(&id);
        }
    });
    clear_resize_hooks(id);
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

/// Register a hook to run when the viewport is resized.
///
/// Pass a closure with two logical-pixel arguments:
/// ```rust,ignore
/// velox_core::lifecycle::set_current_component(velox_core::lifecycle::generate_component_id());
/// velox_core::on_resize!(|width, height| println!("resized: {width}x{height}"));
/// ```
#[macro_export]
macro_rules! on_resize {
    ({ $($body:tt)* }) => {
        $crate::lifecycle::on_resize(move |width, height| { $($body)* })
    };
    ($f:expr) => {
        $crate::lifecycle::on_resize($f)
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
