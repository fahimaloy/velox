use std::cell::RefCell;
use std::collections::VecDeque;

use crate::signal::effect;

thread_local! {
    static NEXT_TICK_QUEUE: RefCell<VecDeque<Box<dyn FnOnce()>>> = RefCell::new(VecDeque::new());
    static FLUSHING: RefCell<bool> = const { RefCell::new(false) };
}

/// Queue a callback for the next tick
pub fn next_tick<F>(callback: F)
where
    F: FnOnce() + 'static,
{
    NEXT_TICK_QUEUE.with(|q| {
        q.borrow_mut().push_back(Box::new(callback));
    });

    schedule_flush();
}

fn schedule_flush() {
    NEXT_TICK_QUEUE.with(|q| {
        if q.borrow().is_empty() {
            return;
        }
        let is_flushing = FLUSHING.with(|f| *f.borrow());
        if is_flushing {
            return;
        }
        if !is_flushing {
            effect(move || {
                FLUSHING.with(|f| {
                    let mut f_borrow = f.borrow_mut();
                    if *f_borrow {
                        return;
                    }
                    *f_borrow = true;
                    drop(f_borrow);
                    let callbacks: Vec<_> = NEXT_TICK_QUEUE.with(|q| {
                        let mut q_borrow = q.borrow_mut();
                        std::mem::take(&mut *q_borrow).into_iter().collect()
                    });
                    for cb in callbacks {
                        cb();
                    }
                    let mut f_borrow = f.borrow_mut();
                    *f_borrow = false;
                });
            });
        }
    });
}
