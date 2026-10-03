//! Simulate the renderer's lifecycle drive without a window.
//!
//! The real renderer calls `run_all_mounted_hooks` on the first
//! `RedrawRequested`, `run_all_updated_hooks` after every `on_event` recompute,
//! and `run_all_destroy_hooks` on `CloseRequested`/drop. This test proves the
//! counting contract without needing a compositor.
use std::cell::RefCell;
use std::rc::Rc;
use velox_core::lifecycle::{
    before_destroy, clear_current_component, generate_component_id, on_mounted, on_updated,
    run_all_destroy_hooks, run_all_mounted_hooks, run_all_updated_hooks, set_current_component,
};

#[test]
fn renderer_sim_mounted_once_updated_per_recompute_destroy_once() {
    let mounted = Rc::new(RefCell::new(0usize));
    let updated = Rc::new(RefCell::new(0usize));
    let destroyed = Rc::new(RefCell::new(0usize));

    let id = generate_component_id();
    set_current_component(id);
    {
        let c = mounted.clone();
        on_mounted(move || *c.borrow_mut() += 1);
        let c = updated.clone();
        on_updated(move || *c.borrow_mut() += 1);
        let c = destroyed.clone();
        before_destroy(move || *c.borrow_mut() += 1);
    }
    clear_current_component();

    // --- renderer: first RedrawRequested triggers mounted once (idempotent) ---
    run_all_mounted_hooks();
    assert_eq!(*mounted.borrow(), 1);
    run_all_mounted_hooks();
    assert_eq!(*mounted.borrow(), 1, "mounted must not fire twice");

    // --- renderer: MouseInput -> on_event + recompute -> run_all_updated_hooks ---
    run_all_updated_hooks();
    assert_eq!(*updated.borrow(), 1);
    run_all_updated_hooks();
    assert_eq!(*updated.borrow(), 2);

    // --- renderer: CloseRequested -> run_all_destroy_hooks ---
    run_all_destroy_hooks();
    assert_eq!(*destroyed.borrow(), 1);
    // double destroy is idempotent
    run_all_destroy_hooks();
    assert_eq!(*destroyed.borrow(), 1);
    // updated after destroy must not fire
    run_all_updated_hooks();
    assert_eq!(*updated.borrow(), 2);

    let _ = id;
}

#[test]
fn renderer_sim_ensure_mounted_helper_idempotent() {
    let mounted = Rc::new(RefCell::new(0usize));
    let id = generate_component_id();
    set_current_component(id);
    on_mounted({
        let c = mounted.clone();
        move || *c.borrow_mut() += 1
    });
    clear_current_component();

    let mut flag = false;
    velox_renderer::ensure_mounted(&mut flag);
    assert_eq!(*mounted.borrow(), 1);
    velox_renderer::ensure_mounted(&mut flag);
    assert_eq!(*mounted.borrow(), 1, "ensure_mounted must be idempotent");
}

#[test]
fn renderer_cleanup_guard_drop_calls_destroy() {
    let destroyed = Rc::new(RefCell::new(0usize));
    let id = generate_component_id();
    set_current_component(id);
    {
        let c = destroyed.clone();
        before_destroy(move || *c.borrow_mut() += 1);
    }
    clear_current_component();

    {
        let _guard = velox_renderer::LifecycleCleanupGuard;
    }
    // The guard's Drop fires run_all_destroy_hooks (wrapped in catch_unwind).
    assert_eq!(*destroyed.borrow(), 1);
}
