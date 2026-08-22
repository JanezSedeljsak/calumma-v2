use crate::engine::Inner;
use calumma_core::limits::{AUTOSAVE_INTERVAL_MS, AUTOSAVE_MAX_SKIPPED_TICKS};
use parking_lot::{Condvar, Mutex};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

struct EnginePtr(*const Mutex<Inner>);
unsafe impl Send for EnginePtr {}

/// Runs `Inner::autosave` on its own cadence instead of piggybacking on `calm_engine_render`,
/// so a slow SQLite write can never land inside a render-thread frame budget. The engine
/// pointer outlives the thread by construction: `calm_engine_free` calls `stop` (which joins)
/// before it drops the `Box<Mutex<Inner>>` the pointer was carved from.
///
/// Moving the *call* off the render path was only half of it: both threads still contend for
/// the one `Mutex<Inner>`, so a blocking lock here just relocated the stall — the render thread
/// would wait out a whole SQLite transaction at an arbitrary point in a frame, with no
/// back-pressure and nothing to recover it. So the tick takes the lock only if it is free.
/// A skipped tick costs 800 ms of staleness and nobody notices; a blocked frame is visible.
pub(crate) struct AutosaveThread {
    stop: Arc<(Mutex<bool>, Condvar)>,
    handle: JoinHandle<()>,
}

pub(crate) fn spawn(engine: *const Mutex<Inner>) -> AutosaveThread {
    let engine = EnginePtr(engine);
    let stop = Arc::new((Mutex::new(false), Condvar::new()));
    let stop_thread = stop.clone();
    let handle = std::thread::spawn(move || {
        let engine = engine;
        let mut skipped: u32 = 0;
        loop {
            let mut stopped = stop_thread.0.lock();
            if *stopped {
                return;
            }
            let timed_out = stop_thread
                .1
                .wait_for(&mut stopped, Duration::from_millis(AUTOSAVE_INTERVAL_MS))
                .timed_out();
            if *stopped {
                return;
            }
            drop(stopped);
            if timed_out {
                let _ = catch_unwind(AssertUnwindSafe(|| {
                    let mutex = unsafe { &*engine.0 };
                    // A run of contended ticks does have to resolve eventually, or a document
                    // being drawn on continuously would never reach disk. After enough skips
                    // the save takes priority over the frame it delays.
                    if skipped >= AUTOSAVE_MAX_SKIPPED_TICKS {
                        mutex.lock().autosave();
                        skipped = 0;
                    } else if let Some(mut inner) = mutex.try_lock() {
                        inner.autosave();
                        skipped = 0;
                    } else {
                        skipped += 1;
                    }
                }));
            }
        }
    });
    AutosaveThread { stop, handle }
}

impl AutosaveThread {
    pub(crate) fn stop(self) {
        *self.stop.0.lock() = true;
        self.stop.1.notify_all();
        let _ = self.handle.join();
    }
}
