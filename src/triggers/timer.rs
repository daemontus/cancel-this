use crate::{CancelAtomic, CancellationTrigger, Cancelled};
use log::{trace, warn};
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;
use std::time::Duration;

/// Run the given `action`, cancelling it if the provided `duration` of time has elapsed,
/// measured by the [`CancelTimer`].
///
/// ```rust
/// # use std::time::Duration;
/// # use cancel_this::{is_cancelled, Cancelled};
///
/// fn cancellable_counter(count: usize) -> Result<(), Cancelled> {
///     for _ in 0..count {
///         is_cancelled!()?;
///         std::thread::sleep(Duration::from_millis(10));
///     }
///     Ok(())
/// }
///
/// let result_fast = cancel_this::on_timeout(Duration::from_millis(100), || cancellable_counter(5));
/// assert!(result_fast.is_ok());
///
/// let result_slow = cancel_this::on_timeout(Duration::from_millis(100), || cancellable_counter(50));
/// assert!(result_slow.is_err());
/// ```
pub fn on_timeout<TResult, TError, TAction>(
    duration: Duration,
    action: TAction,
) -> Result<TResult, TError>
where
    TAction: FnOnce() -> Result<TResult, TError>,
    TError: From<Cancelled>,
{
    crate::on_trigger(CancelTimer::start(duration), action)
}

/// Implementation of [`CancellationTrigger`] that is cancelled once the specified [`Duration`]
/// elapsed. The "timer" is started immediately upon creation.
///
/// ## Logging
///  - `[trace]` Every time a timer is started or elapsed (i.e. upon cancellation).
///  - `[warn]` If the timer is dropped, but the timer thread cannot be safely destroyed.
#[derive(Debug, Clone)]
// The trigger is storing its "core data", but it won't access them. It only needs to keep them
// around so that they are dropped once all copies of the trigger are destroyed as well.
#[allow(dead_code)]
pub struct CancelTimer(CancelAtomic, Arc<CancelTimerCore>);

impl CancellationTrigger for CancelTimer {
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }

    fn type_name(&self) -> &'static str {
        "CancelTimer"
    }
}

impl CancelTimer {
    /// Create a new [`CancelTimer`] that will be cancelled once the given `duration` elapsed.
    pub fn start(duration: Duration) -> Self {
        let trigger = CancelAtomic::default();
        let core = CancelTimerCore::start(trigger.clone(), duration);
        trace!(
            "`CancelTimer[{:p}]` started; Waiting for {}ms.",
            trigger.id_ref(),
            duration.as_millis()
        );
        CancelTimer(trigger, Arc::new(core))
    }
}

/// An internal data structure that manages the timer required by [`CancelTimer`]. In particular,
/// it is responsible for safely shutting down the timer thread once the timer is no longer
/// needed (to avoid leaking a million timer threads in applications where the timeout is long
/// but is used very often).
#[derive(Debug)]
struct CancelTimerCore {
    trigger: CancelAtomic,
    timer_thread: Option<JoinHandle<()>>,
    stop_trigger: Sender<()>,
}

impl CancelTimerCore {
    pub fn start(trigger: CancelAtomic, duration: Duration) -> Self {
        let trigger_copy = trigger.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            // If this is `Ok`, it means the timer got cancelled.
            // If it is `Err`, it means the duration elapsed.
            // In practice, this distinction should be irrelevant, since the timer can only
            // be cancelled if the whole cancellation trigger is dropped, meaning it is no
            // longer observed by anyone...
            match receiver.recv_timeout(duration) {
                Ok(()) => (),
                Err(_) => {
                    trace!(
                        "`CancelTimer[{:p}]` elapsed. Canceling.",
                        trigger_copy.id_ref()
                    );
                    trigger_copy.cancel()
                }
            }
        });
        CancelTimerCore {
            trigger,
            timer_thread: Some(handle),
            stop_trigger: sender,
        }
    }
}

impl Drop for CancelTimerCore {
    fn drop(&mut self) {
        let thread = self
            .timer_thread
            .take()
            .expect("Invariant violation: Timer thread removed before drop.");

        let join = match self.stop_trigger.send(()) {
            Ok(()) => thread.join(),
            Err(_) => {
                // The receiver has already been deallocated, meaning the timer most likely
                // elapsed and the thread should be dead.
                if !thread.is_finished() {
                    warn!(
                        "Timer of `CancelTimer[{:p}]` cannot be stopped. Possible thread leak.`",
                        self.trigger.id_ref()
                    );
                    return;
                } else {
                    thread.join()
                }
            }
        };
        if join.is_err() {
            // The thread panicked, meaning we probably want to propagate it.
            panic!("Timer thread of `CancelTimer` trigger panicked.");
        }
    }
}
