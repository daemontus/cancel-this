use crate::{CancelChain, CancellationTrigger, DynamicCancellationTrigger};
use log::{trace, warn};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;
use std::time::Duration;

thread_local! {
    static CANCELLATION_STAMP: Arc<AtomicU64> = Arc::new(AtomicU64::default());
}

/// Liveness guard observes [`crate::is_cancelled`] calls and reports situations where the
/// thread becomes "unresponsive", meaning cancellation has not been checked for at least
/// the specified amount of time.
///
/// **Each [`LivenessGuard`] is bound to the specific thread it has been started on, and
/// monitors cancellation tasks in that thread only.** If the thread is blocked for a long
/// time due to some external reason (e.g. waiting for IO), this is still considered as
/// "becoming unresponsive". As such, it is generally a good idea to set the responsiveness
/// threshold reasonably high (e.g. at least a few seconds) to avoid spurious
/// reports of inactivity.
///
/// ```rust
/// # use std::sync::Arc;
/// # use std::sync::atomic::{AtomicBool, Ordering};
/// # use std::time::Duration;
/// # use cancel_this::{LivenessGuard, is_cancelled, Cancellable};
/// let expect_alive = Arc::new(AtomicBool::new(true));
///
/// let expect_alive_guard = expect_alive.clone();
/// let guard = LivenessGuard::new(Duration::from_millis(100), move |is_alive| {
///     assert_eq!(is_alive, expect_alive_guard.load(Ordering::SeqCst));
/// });
///
/// let r: Cancellable<()> = cancel_this::never(|| {
///     let mut sleep_time = 10;
///     // First, start increasing sleep time until the task is considered "not live".
///     for _ in 0..5 {
///         is_cancelled!()?;
///         expect_alive.store(sleep_time < 100, Ordering::SeqCst);
///         std::thread::sleep(Duration::from_millis(sleep_time));
///         sleep_time += sleep_time;
///     }
///
///     // Then run a bunch of quick sleep intervals to show that the task
///     // is in fact still "alive".
///     expect_alive.store(true, Ordering::SeqCst);
///     for _ in 0..5 {
///         is_cancelled!()?;
///         std::thread::sleep(Duration::from_millis(40));
///     }
///     Ok(())
/// });
///
/// ```
pub struct LivenessGuard {
    monitor_thread: Option<JoinHandle<()>>,
    stop_monitor: Sender<()>,
}

impl LivenessGuard {
    /// Create a new liveness guard for the current thread using the provided callback.
    ///
    /// The callback is invoked every time the liveness changes, which can happen periodically,
    /// approximately every time the threshold duration elapses. The callback receives the new
    /// liveness status as argument. If the liveness status has not changed, the callback is
    /// not invoked.
    ///
    pub fn new<TAction: Fn(bool) + Send + Sync + 'static>(
        threshold: Duration,
        status_change: TAction,
    ) -> LivenessGuard {
        let (sender, receiver) = std::sync::mpsc::channel();
        let cancellation_token = CANCELLATION_STAMP.try_with(|it| it.clone()).unwrap();
        let monitor_thread = std::thread::spawn(move || {
            let mut is_alive = true;
            let mut last_stamp = 0u64;
            loop {
                // If this is `Ok`, it means the monitor is being destroyed.
                // If it is `Err`, it means the duration elapsed.
                match receiver.recv_timeout(threshold) {
                    Ok(()) => return,
                    Err(_) => {
                        trace!("`LivenessGuard` waking up to evaluate task activity...");
                        let current_stamp = cancellation_token.load(Ordering::SeqCst);
                        let new_is_alive = current_stamp != last_stamp;
                        if new_is_alive != is_alive {
                            is_alive = new_is_alive;
                            last_stamp = current_stamp;
                            status_change(is_alive);
                        }
                    }
                }
            }
        });
        LivenessGuard {
            monitor_thread: Some(monitor_thread),
            stop_monitor: sender,
        }
    }
}

impl Drop for LivenessGuard {
    fn drop(&mut self) {
        let thread = self
            .monitor_thread
            .take()
            .expect("Invariant violation: Monitor thread removed before drop.");

        let join = match self.stop_monitor.send(()) {
            Ok(()) => thread.join(),
            Err(_) => {
                // The receiver has already been deallocated, meaning the monitor is most likely dead.
                if !thread.is_finished() {
                    warn!("`LivenessGuard` cannot be stopped. Possible thread leak.`");
                    return;
                } else {
                    thread.join()
                }
            }
        };
        if join.is_err() {
            // The thread panicked, meaning we probably want to propagate it.
            panic!("Monitor thread of `LivenessGuard` panicked.");
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct LivenessInterceptor<R: CancellationTrigger + Clone>(R);

/// Used to implement "liveness transfers" between threads, allowing one thread to notify
/// the liveness stamp of the parent thread.
#[derive(Clone)]
pub(crate) struct TransferredLivenessInterceptor<R: CancellationTrigger + Clone> {
    inner: R,
    stamp: Arc<AtomicU64>,
}

impl<R: CancellationTrigger + Clone> LivenessInterceptor<R> {
    pub fn as_inner_mut(&mut self) -> &mut R {
        &mut self.0
    }

    pub fn as_inner(&self) -> &R {
        &self.0
    }
}

impl LivenessInterceptor<CancelChain> {
    pub fn clone_and_flatten(&self) -> DynamicCancellationTrigger {
        let chain = self.as_inner().clone_and_flatten();
        match CANCELLATION_STAMP.try_with(Arc::clone) {
            Ok(stamp) => Box::new(TransferredLivenessInterceptor {
                inner: chain,
                stamp,
            }),
            Err(e) => {
                warn!("`LivenessGuard` cannot access the cancellation stamp: {e:?}");
                chain
            }
        }
    }
}

impl<R: CancellationTrigger + Clone> CancellationTrigger for LivenessInterceptor<R> {
    fn is_cancelled(&self) -> bool {
        let result = CANCELLATION_STAMP.try_with(|it| it.fetch_add(1, Ordering::SeqCst));
        if let Err(e) = result {
            warn!("`LivenessGuard` cannot update the cancellation stamp: {e:?}");
        }
        self.0.is_cancelled()
    }

    fn type_name(&self) -> &'static str {
        self.0.type_name()
    }
}

impl<R: CancellationTrigger + Clone> CancellationTrigger for TransferredLivenessInterceptor<R> {
    fn is_cancelled(&self) -> bool {
        self.stamp.fetch_add(1, Ordering::SeqCst);
        self.inner.is_cancelled()
    }

    fn type_name(&self) -> &'static str {
        self.inner.type_name()
    }
}
