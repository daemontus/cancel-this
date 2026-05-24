use crate::{CancellationTrigger, Cancelled};
use log::trace;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};

/// Run the given `action`, cancelling it using [`CancelMemoryPoll`] if the overall memory consumption
/// of the whole process exceeds the given memory `limit` (in bytes).
///
/// *The only way to keep such a memory trigger accurate is to repeatedly monitor
/// memory consumption. While this is not prohibitively costly, it is still much more
/// expensive than all other cancellation triggers implemented in this crate.*
///
/// Memory usage is checked on every cancellation check, including the first one. See
/// [`crate::on_memory_sample`] for a cheaper alternative that samples memory usage in a background
/// thread (with slightly stale readings and a delay before the first check).
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
/// let result_ok = cancel_this::on_memory_poll(15_000_000, || cancellable_allocator(5));
/// assert!(result_ok.is_ok());
///
/// // The second action requires ~800MB of memory and has an effective ~3-8MB limit.
/// let result_err = cancel_this::on_memory_poll(15_000_000, || cancellable_allocator(100_000));
/// assert!(result_err.is_err());
/// ```
pub fn on_memory_poll<TResult, TError, TAction>(
    limit: usize,
    action: TAction,
) -> Result<TResult, TError>
where
    TAction: FnOnce() -> Result<TResult, TError>,
    TError: From<Cancelled>,
{
    crate::on_trigger(CancelMemoryPoll::limit(limit), action)
}

/// Deprecated alias for [`on_memory_poll`].
#[deprecated(since = "0.4.0", note = "renamed to `on_memory_poll`")]
pub fn on_memory<TResult, TError, TAction>(limit: usize, action: TAction) -> Result<TResult, TError>
where
    TAction: FnOnce() -> Result<TResult, TError>,
    TError: From<Cancelled>,
{
    on_memory_poll(limit, action)
}

/// Implementation of [`CancellationTrigger`] that is canceled when the given memory limit
/// is exceeded (monitored via polling).
///
/// This uses the `memory-stats` crate to observe memory usage. The current implementation
/// polls the memory usage on every cancellation check. As a consequence, this is not a hard
/// memory limit (the execution still only stops at cancellation points), and it can add non-trivial
/// overhead to cancellation checks. We are trying to mitigate this by using the "faster" but
/// less accurate memory check method, but this can still be non-trivial.
///
/// See also [`on_memory_poll`], [`crate::CancelMemorySample`], and [`crate::on_memory_sample`].
///
/// ## Logging
///  - Each trigger should produce a [`trace`] message when actually canceled.
#[derive(Debug, Clone)]
pub struct CancelMemoryPoll {
    mem_limit_bytes: usize,
    is_cancelled: Arc<AtomicBool>,
}

impl CancellationTrigger for CancelMemoryPoll {
    fn is_cancelled(&self) -> bool {
        if self.is_cancelled.load(SeqCst) {
            // The trigger is already canceled.
            return true;
        }

        if let Some(stats) = memory_stats::memory_stats()
            && stats.physical_mem > self.mem_limit_bytes
        {
            self.is_cancelled.store(true, SeqCst); // Remember that this trigger is now canceled.
            trace!(
                "`CancelMemoryPoll[{:p}]` canceled (limit: {}; used: {}).",
                self, self.mem_limit_bytes, stats.physical_mem
            );
            return true;
        }

        false
    }

    fn type_name(&self) -> &'static str {
        "CancelMemoryPoll"
    }
}

/// Deprecated alias for [`CancelMemoryPoll`].
#[deprecated(since = "0.4.0", note = "renamed to `CancelMemoryPoll`")]
pub type CancelMemory = CancelMemoryPoll;

impl CancelMemoryPoll {
    /// Create a new instance of [`CancelMemoryPoll`] with the given memory limit (in bytes).
    pub fn limit(limit: usize) -> CancelMemoryPoll {
        CancelMemoryPoll {
            mem_limit_bytes: limit,
            is_cancelled: Default::default(),
        }
    }
}
