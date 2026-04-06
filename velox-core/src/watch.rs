use std::cell::RefCell;
use std::rc::Rc;

use crate::signal::effect;

/// Watch a reactive source and call `callback(new, old)` when it changes.
/// - Runs the source once to capture dependencies (no callback on first run)
/// - Triggers callback only when `new != old`
///
/// Example:
/// watch(|| count.get(), |new, old| { println!("{old} -> {new}"); });
#[derive(Clone, Default)]
pub struct WatchOptions {
    pub deep: bool,
    pub immediate: bool,
}

pub struct WatchHandle {
    // Placeholder for future cleanup
}

impl WatchHandle {
    pub fn stop(self) {}
}

pub fn watch<T, S, F>(mut source: S, callback: F, options: WatchOptions) -> WatchHandle
where
    T: PartialEq + Clone + 'static,
    S: FnMut() -> T + 'static,
    F: FnMut(T, T) + 'static,
{
    let prev: Rc<RefCell<Option<T>>> = Rc::new(RefCell::new(None));

    effect({
        let prev = prev.clone();
        let mut callback = callback;
        move || {
            let next = source();

            let mut prev_borrow = prev.borrow_mut();
            match &mut *prev_borrow {
                Some(old) => {
                    if *old != next {
                        let old_clone = old.clone();
                        let next_clone = next.clone();
                        *prev_borrow = Some(next_clone.clone());
                        drop(prev_borrow);
                        callback(next_clone, old_clone);
                    }
                }
                None => {
                    let initial = next.clone();
                    *prev_borrow = Some(initial.clone());
                    drop(prev_borrow);
                    if options.immediate {
                        callback(initial.clone(), initial);
                    }
                }
            }
        }
    });
    WatchHandle {}
}

pub fn watch_effect<F>(f: F, _options: WatchOptions) -> WatchHandle
where
    F: FnMut() + 'static,
{
    effect(f);
    WatchHandle {}
}
