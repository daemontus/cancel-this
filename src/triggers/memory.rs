use crate::{CancellationTrigger, Cancelled};
use log::trace;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;

/// Run the given `action`, cancelling it using [`CancelMemory`] if the overall memory consumption
/// of the whole process exceeds the given memory `limit` (in bytes).
///
/// *The only way to keep such a memory trigger accurate is to repeatedly monitor
/// memory consumption. While this is not prohibitively costly, it is still much more
/// expensive than all other cancellation triggers implemented in this crate.*
///
/// ```rust
/// # use cancel_this::{Cancelled, is_cancelled};
/// # let _ = env_logger::builder().is_test(true).try_init();
/// fn cancellable_allocator(count: usize) -> Result<Vec<usize>, Cancelled> {
///     let mut result = Vec::new();
///     for i in 0..count {
///         is_cancelled!()?;
///         result.extend(0..1000);
///     }
///     Ok(result)
/// }
///
/// // The test runner itself in debug mode needs ~7-12MB.
///
/// // The first action only requires ~40kB of memory and has an effective ~3-8 MB limit.
/// let result_ok = cancel_this::on_memory(15_000_000, || cancellable_allocator(5));
/// assert!(result_ok.is_ok());
///
/// // The second action requires ~800MB of memory and has an effective ~3-8MB limit.
/// let result_err = cancel_this::on_memory(15_000_000, || cancellable_allocator(100_000));
/// assert!(result_err.is_err());
/// ```
pub fn on_memory<TResult, TError, TAction>(limit: usize, action: TAction) -> Result<TResult, TError>
where
    TAction: FnOnce() -> Result<TResult, TError>,
    TError: From<Cancelled>,
{
    crate::on_trigger(CancelMemory::limit(limit), action)
}

/// Implementation of [`CancellationTrigger`] that is canceled when the given memory limit
/// is exceeded.
///
/// This uses the `memory-stats` crate to observe memory usage. The current implementation
/// polls the memory usage on every cancellation check. As a consequence, this is not a hard
/// memory limit (the execution still only stops at cancellation points), and it can add non-trivial
/// overhead to cancellation checks. We are trying to mitigate this by using the "faster" but
/// less accurate memory check method, but this can still be non-trivial.
///
/// See also [`on_memory`].
///
/// ## Logging
///  - Each trigger should produce a [`trace`] message when actually canceled.
#[derive(Debug, Clone)]
pub struct CancelMemory(usize, Arc<AtomicBool>);

impl CancellationTrigger for CancelMemory {
    fn is_cancelled(&self) -> bool {
        if self.1.load(SeqCst) {
            // The trigger is already canceled.
            return true;
        }

        if let Some(stats) = memory_stats::memory_stats()
            && stats.physical_mem > self.0
        {
            self.1.store(true, SeqCst); // Remember that this trigger is now canceled.
            trace!(
                "`CancelMemory[{:p}]` canceled (limit: {}; used: {}).",
                self, self.0, stats.physical_mem
            );
            return true;
        }

        false
    }

    fn type_name(&self) -> &'static str {
        "CancelMemory"
    }
}

impl CancelMemory {
    /// Create a new instance of [`CancelMemory`] with the given memory limit (in bytes).
    pub fn limit(limit: usize) -> CancelMemory {
        CancelMemory(limit, Default::default())
    }
}
