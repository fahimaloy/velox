use std::cell::RefCell;
use std::rc::Rc;
use velox_core::lifecycle::{
    before_destroy, cleanup_component, clear_current_component, generate_component_id, on_mounted,
    on_updated, run_all_destroy_hooks, run_all_mounted_hooks, run_all_updated_hooks,
    run_destroy_hooks, run_mounted_hooks, run_updated_hooks, set_current_component,
};

/// Counting test that proves hooks fire the correct number of times when the
/// renderer drives the lifecycle correctly.
///
/// Before fix 0D this test FAILS because:
/// - `run_all_mounted_hooks` / `run_all_updated_hooks` / `run_all_destroy_hooks`
///   do not exist, and the renderer never calls `run_mounted_hooks` /
///   `run_updated_hooks` / `run_destroy_hooks` at all, so hooks leak.
#[test]
fn lifecycle_counting_via_per_component_api() {
    let mounted = Rc::new(RefCell::new(0usize));
    let updated = Rc::new(RefCell::new(0usize));
    let destroyed = Rc::new(RefCell::new(0usize));

    let id = generate_component_id();
    set_current_component(id);
    {
        let m = mounted.clone();
        on_mounted(move || *m.borrow_mut() += 1);
        let u = updated.clone();
        on_updated(move || *u.borrow_mut() += 1);
        let d = destroyed.clone();
        before_destroy(move || *d.borrow_mut() += 1);
    }
    clear_current_component();

    // --- mount: must fire exactly once ---
    run_mounted_hooks(id);
    assert_eq!(*mounted.borrow(), 1, "on_mounted should fire once on first mount");
    run_mounted_hooks(id);
    assert_eq!(*mounted.borrow(), 1, "on_mounted must not fire twice");

    // --- updated: must fire on every recompute ---
    run_updated_hooks(id);
    assert_eq!(*updated.borrow(), 1);
    run_updated_hooks(id);
    assert_eq!(*updated.borrow(), 2, "on_updated fires on every update");

    // --- destroy: must fire once and cleanup ---
    run_destroy_hooks(id);
    cleanup_component(id);
    assert_eq!(*destroyed.borrow(), 1, "on_unmounted/before_destroy should fire once");

    // after cleanup updated must not fire
    run_updated_hooks(id);
    assert_eq!(*updated.borrow(), 2, "updated must not fire after cleanup");
}

/// Same counting behaviour but through the global `run_all_*` helpers that the
/// renderer uses when it does not know the component id (root app case).
#[test]
fn lifecycle_counting_via_global_helpers() {
    let mounted = Rc::new(RefCell::new(0usize));
    let updated = Rc::new(RefCell::new(0usize));
    let destroyed = Rc::new(RefCell::new(0usize));

    let id = generate_component_id();
    set_current_component(id);
    {
        let m = mounted.clone();
        on_mounted(move || *m.borrow_mut() += 1);
        let u = updated.clone();
        on_updated(move || *u.borrow_mut() += 1);
        let d = destroyed.clone();
        before_destroy(move || *d.borrow_mut() += 1);
    }
    clear_current_component();

    // renderer: first RedrawRequested -> run_all_mounted_hooks once
    run_all_mounted_hooks();
    assert_eq!(*mounted.borrow(), 1);
    run_all_mounted_hooks();
    assert_eq!(*mounted.borrow(), 1, "global mounted must be idempotent");

    // renderer: after each on_event recompute -> run_all_updated_hooks
    run_all_updated_hooks();
    run_all_updated_hooks();
    assert_eq!(*updated.borrow(), 2);

    // renderer: on CloseRequested -> run_all_destroy_hooks + cleanup
    run_all_destroy_hooks();
    assert_eq!(*destroyed.borrow(), 1);
    // second destroy must not double-fire
    run_all_destroy_hooks();
    assert_eq!(*destroyed.borrow(), 1);

    // updated after destroy must not fire
    run_all_updated_hooks();
    assert_eq!(*updated.borrow(), 2);
}

/// Verify the RAII guard calls destroy on drop (used by renderer drop guard).
#[test]
fn lifecycle_guard_drop_calls_destroy() {
    use velox_core::lifecycle::LifecycleHandle;

    let destroyed = Rc::new(RefCell::new(0usize));
    let id = {
        let guard = LifecycleHandle::new();
        let d = destroyed.clone();
        set_current_component(guard.id());
        before_destroy(move || *d.borrow_mut() += 1);
        clear_current_component();
        guard.mount();
        // guard will be dropped at end of this block -> destroy
        guard.id()
    };
    // guard dropped, destroy should have fired
    assert_eq!(*destroyed.borrow(), 1, "guard drop must call destroy");
    // ensure no leak - running again must not fire
    velox_core::lifecycle::run_destroy_hooks(id);
    assert_eq!(*destroyed.borrow(), 1);
}
