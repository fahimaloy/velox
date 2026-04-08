use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

thread_local! {
    static NEXT_TICK_QUEUE: RefCell<VecDeque<Box<dyn FnOnce()>>> = RefCell::new(VecDeque::new());
    static FLUSHING: Cell<bool> = const { Cell::new(false) };
}

/// Guard to ensure FLUSHING flag is always reset, even if a callback panics.
struct FlushGuard;

impl Drop for FlushGuard {
    fn drop(&mut self) {
        FLUSHING.with(|f| f.set(false));
    }
}

/// Queue a callback for the next tick.
/// Callbacks are executed after the current reactive flush completes.
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
    // If already flushing, the current flush will pick up new callbacks
    if FLUSHING.with(|f| f.get()) {
        return;
    }

    // Use a single synchronous flush - this ensures callbacks run
    // immediately after the current execution context, similar to microtasks.
    // In a browser this would use queueMicrotask; in a native app we flush
    // synchronously at the end of the current event handling.
    flush_sync();
}

/// Synchronously flush all pending next_tick callbacks.
/// This processes callbacks in a loop to handle nested next_tick calls.
pub fn flush_sync() {
    // Prevent re-entrant flush
    if FLUSHING.with(|f| f.get()) {
        return;
    }

    FLUSHING.with(|f| f.set(true));
    let _guard = FlushGuard; // Ensures FLUSHING is reset even on panic

    loop {
        // Take all current callbacks
        let callbacks: Vec<_> = NEXT_TICK_QUEUE.with(|q| {
            let mut q_borrow = q.borrow_mut();
            std::mem::take(&mut *q_borrow).into_iter().collect()
        });

        // If no callbacks, we're done
        if callbacks.is_empty() {
            break;
        }

        // Execute callbacks - any new callbacks queued during this
        // will be picked up in the next loop iteration
        for cb in callbacks {
            cb();
        }
    }

    // Guard will reset FLUSHING when it drops
}
