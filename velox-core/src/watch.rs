use std::cell::RefCell;
use std::rc::Rc;

use crate::signal::effect;

/// Watch a reactive source and call `callback(new, old)` when it changes.
/// - Runs the source once to capture dependencies (no callback on first run)
/// - Triggers callback only when `new != old`
///
/// Example:
/// watch(|| count.get(), |new, old| { println!("{old} -> {new}"); });
pub fn watch<T, S, F>(mut source: S, mut callback: F)
where
    T: PartialEq + Clone + 'static,
    S: FnMut() -> T + 'static,
    F: FnMut(&T, &T) + 'static,
{
    let prev: Rc<RefCell<Option<T>>> = Rc::new(RefCell::new(None));

    effect({
        let prev = prev.clone();
        move || {
            let next = source();

            // Borrow prev, compare, and update before calling user callback
            // so the callback can freely mutate signals.
            let mut prev_borrow = prev.borrow_mut();
            match &*prev_borrow {
                Some(old) => {
                    if *old != next {
                        let old_clone = old.clone();
                        let next_clone = next.clone();
                        *prev_borrow = Some(next);
                        drop(prev_borrow); // release borrow before user code
                        callback(&next_clone, &old_clone);
                    }
                }
                None => {
                    // First evaluation: record baseline, do not call callback
                    *prev_borrow = Some(next);
                }
            }
        }
    });
}
