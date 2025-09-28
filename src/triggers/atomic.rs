use crate::{CancellationTrigger, Cancelled};
use log::trace;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Run the given `action`, cancelling it if the provided [`CancelAtomic`] `trigger` is cancelled
/// by some external mechanism.
///
/// ```rust
/// # use std::time::Duration;
/// # use cancel_this::{is_cancelled, CancelAtomic, Cancelled};
///
/// fn cancellable_counter(count: usize) -> Result<(), Cancelled> {
///     for _ in 0..count {
///         is_cancelled!()?;
///         std::thread::sleep(Duration::from_millis(10));
///     }
///     Ok(())
/// }
///
/// let trigger = CancelAtomic::new();
///
/// // Run two actions, one fast, the other slow. Both cancel the `trigger` once they are done,
/// // but only the fast action should be able to finish. The slow action should end up being
/// // cancelled.
///
/// let trigger_slow = trigger.clone();
/// let t1 = std::thread::spawn(move || {
///     let result_slow = cancel_this::on_atomic(trigger_slow.clone(), || cancellable_counter(50));
///     trigger_slow.cancel();
///     assert!(result_slow.is_err());
/// });
///
/// let trigger_fast = trigger.clone();
/// let t2 = std::thread::spawn(move || {
///     let result_fast = cancel_this::on_atomic(trigger_fast.clone(), || cancellable_counter(5));
///     trigger_fast.cancel();
///     assert!(result_fast.is_ok());
/// });
///
/// t1.join().unwrap();
/// t2.join().unwrap();
/// ```
pub fn on_atomic<TResult, TError, TAction>(
    trigger: CancelAtomic,
    action: TAction,
) -> Result<TResult, TError>
where
    TAction: FnOnce() -> Result<TResult, TError>,
    TError: From<Cancelled>,
{
    crate::on_trigger(trigger, action)
}

/// Implementation of [`CancellationTrigger`] that is cancelled manually by calling
/// [`CancelAtomic::cancel`].
///
/// It is safe to cancel this trigger multiple times, and once cancelled, the trigger
/// cannot be reset.
///
/// ## Logging
///  - `[trace]` Every time the trigger is canceled.
#[derive(Debug, Clone, Default)]
pub struct CancelAtomic(Arc<AtomicBool>);

impl CancellationTrigger for CancelAtomic {
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

impl CancelAtomic {
    /// Create a new trigger instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Cancel this trigger.
    ///
    /// Can be safely called multiple times, but once triggered, the instance is considered
    /// cancelled and cannot be reset.
    pub fn cancel(&self) {
        let first_caller = self
            .0
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok();
        if first_caller {
            trace!("`CancelAtomic[{:p}]` cancelled.", self.id_ref());
        } else {
            // The atomic swap can only fail if the value is already `true`.
            trace!("`CancelAtomic[{:p}]` already cancelled.`", self.id_ref());
        }
    }

    /// Provides a reference which "identifies" this trigger when logging.
    pub(crate) fn id_ref(&self) -> &AtomicBool {
        self.0.as_ref()
    }
}
