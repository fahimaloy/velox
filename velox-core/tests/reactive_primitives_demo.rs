//! Reactive primitives demo for Velox.
//!
//! This file is the "walking skeleton" for the velox-core reactive toolkit. It
//! demonstrates every primitive working together against a tiny simulated
//! component lifecycle — the same shape an SFC `<script setup>` block takes when
//! compiled by `velox-sfc`:
//!
//! - `ref!` / `shallow!` / `signal!` — reactive state
//! - `computed_ref` / `signal::computed` — derived state
//! - `watch` / `watch_effect` — side-effect subscriptions
//! - `on_mounted!` / `on_updated!` / `on_unmounted!` — lifecycle hooks
//!
//! An SFC `<script setup>` block is embedded verbatim into a generated Rust
//! module, so it calls the hooks through the same fully-qualified
//! `velox_core::` paths shown below.

use std::cell::RefCell as StdRefCell;
use std::rc::Rc;

use velox_core as vc;

type Log = Rc<StdRefCell<Vec<String>>>;

/// RAII scope that sets the current component context for the duration of a
/// render, mirroring how an SFC render pass brackets the `<script setup>` block.
struct ComponentScope {
    id: usize,
}

impl ComponentScope {
    fn new(id: usize) -> Self {
        vc::lifecycle::set_current_component(id);
        Self { id }
    }
}

impl Drop for ComponentScope {
    fn drop(&mut self) {
        vc::lifecycle::clear_current_component();
    }
}

#[test]
fn demo_reactive_primitives_together() {
    let log: Log = Rc::new(StdRefCell::new(Vec::new()));

    // ---- lifecycle drive: enter the component's render scope ----
    let comp = ComponentScope::new(vc::lifecycle::generate_component_id());

    // ---- reactive state (Vue-like ref) ----
    let count = vc::r#ref!(0_i32);
    let fast = vc::shallow!(0u32); // Copy-only, lighter weight

    // ---- reactive state (signal! + derived/computed) ----
    let names = vc::signal!(names = vec![String::from("a"), String::from("b")]);
    let total_len = vc::computed_ref({
        let names = names.clone();
        move || names.get().iter().map(String::len).sum::<usize>()
    });

    // ---- lifecycle hooks registered inside the component scope ----
    let log_m = log.clone();
    vc::on_mounted! {
        {
            log_m.borrow_mut().push("mounted".to_string());
        }
    }
    // The macro wraps a block body in `move ||`, so anything it reads must be
    // moved in; clones let the outer scope keep using the same signal.
    let count_in_hook = count.clone();
    let log_u = log.clone();
    vc::on_updated! {
        {
            log_u
                .borrow_mut()
                .push(format!("updated:count={}", count_in_hook.get()));
        }
    }
    let log_u = log.clone();
    vc::on_unmounted! {
        {
            log_u.borrow_mut().push("unmounted".to_string());
        }
    }

    // Fire the mounted hook.
    vc::lifecycle::run_mounted_hooks(comp.id);
    assert_eq!(*log.borrow(), vec!["mounted".to_string()]);

    // ---- side-effect subscription via watch ----
    let watch_log = Rc::new(StdRefCell::new(Vec::<(i32, i32)>::new()));
    // `watch` consumes a closure that reads source signals (capturing `count`
    // by Rc clone). Keep the handle alive: dropping it stops the effect.
    let count_src = count.clone();
    let watch_log_cb = watch_log.clone();
    let _watch = vc::watch::watch(
        move || count_src.get(),
        move |new, old| watch_log_cb.borrow_mut().push((new, old)),
        vc::watch::WatchOptions::default(),
    );

    // ---- mutate and observe ----
    count.update(|v| v + 1);
    count.update(|v| v + 1);
    fast.update(|v| v + 5);

    // Derived value recomputed automatically.
    assert_eq!(total_len.get(), 2);
    names.update(|mut list| {
        list.push(String::from("ccc"));
        list
    });
    assert_eq!(total_len.get(), 5);

    // watch fires with (new, old) for each change.
    assert_eq!(*watch_log.borrow(), vec![(1, 0), (2, 1)]);

    // ---- updated hook fires on every re-render (stays registered) ----
    vc::lifecycle::run_updated_hooks(comp.id);
    vc::lifecycle::run_updated_hooks(comp.id);
    assert_eq!(
        *log.borrow(),
        vec![
            "mounted".to_string(),
            "updated:count=2".to_string(),
            "updated:count=2".to_string(),
        ]
    );

    // ---- unmount ----
    vc::lifecycle::run_unmounted_hooks(comp.id);
    vc::lifecycle::cleanup_component(comp.id);
    assert_eq!(log.borrow().last().unwrap(), "unmounted");

    // Scoped state stays alive past the render scope (held by strong Rc).
    assert_eq!(count.get(), 2);
    assert_eq!(fast.get(), 5);
}

#[test]
fn demo_lifecycle_hooks_require_component_context() {
    // Outside a component context the hooks warn and do NOT register, rather
    // than panicking, so generated SFC render code stays robust.
    assert!(vc::lifecycle::current_component_id().is_none());
    vc::on_mounted!(|| {});
    vc::on_updated!(|| {});
    vc::on_unmounted!(|| {});
}