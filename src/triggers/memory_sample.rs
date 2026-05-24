use crate::{CancelAtomic, CancellationTrigger, Cancelled};
use log::{trace, warn};
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;
use std::time::Duration;

/// Run the given `action`, cancelling it using [`CancelMemorySample`] if the overall memory
/// consumption of the whole process exceeds the given memory `limit` (in bytes).
///
/// Memory usage is sampled by a background thread at roughly one-millisecond intervals.
/// This makes cancellation checks much cheaper than [`crate::on_memory_poll`], but the observed memory
/// usage can be slightly stale. Unlike polling, the first memory check happens only after the
/// first sampling interval elapses (not immediately when the trigger is created). Also note that
/// sampling can trigger between cancellation points, so the sampler can in theory see higher
/// usage even if the affected memory is only used in-between cancellation points.
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
/// let result_ok = cancel_this::on_memory_sample(15_000_000, || cancellable_allocator(5));
/// assert!(result_ok.is_ok());
///
/// // The second action requires ~800MB of memory and has an effective ~3-8MB limit.
/// let result_err = cancel_this::on_memory_sample(15_000_000, || cancellable_allocator(100_000));
/// assert!(result_err.is_err());
/// ```
pub fn on_memory_sample<TResult, TError, TAction>(
    limit: usize,
    action: TAction,
) -> Result<TResult, TError>
where
    TAction: FnOnce() -> Result<TResult, TError>,
    TError: From<Cancelled>,
{
    on_memory_sample_with_interval(limit, Duration::from_millis(1), action)
}

/// Same as [`on_memory_sample`], but with a custom sampling `interval`.
///
/// The `sample_interval` must be greater than zero.
pub fn on_memory_sample_with_interval<TResult, TError, TAction>(
    limit: usize,
    sample_interval: Duration,
    action: TAction,
) -> Result<TResult, TError>
where
    TAction: FnOnce() -> Result<TResult, TError>,
    TError: From<Cancelled>,
{
    crate::on_trigger(CancelMemorySample::start(limit, sample_interval), action)
}

/// Implementation of [`CancellationTrigger`] that is canceled when the given memory limit
/// is exceeded (monitored via sampling).
///
/// This uses the `memory-stats` crate to observe memory usage. Unlike [`crate::CancelMemoryPoll`], memory
/// usage is monitored by a background thread at a fixed interval. As a consequence, this is not
/// a hard memory limit (the execution still only stops at cancellation points), and the observed
/// memory usage can be slightly stale, but cancellation checks themselves are cheap.
///
/// The sampler is started immediately upon creation, but the first memory check is performed
/// only after the first sampling interval elapses.
///
/// See also [`on_memory_sample`], [`on_memory_sample_with_interval`], and [`crate::CancelMemoryPoll`].
///
/// ## Logging
///  - `[trace]` Every time a sampler is started or the memory limit is exceeded (i.e., upon cancellation).
///  - `[warn]` If the sampler is dropped, but the sampler thread cannot be safely destroyed.
#[derive(Debug, Clone)]
// The trigger is storing its "core data", but it won't access them. It only needs to keep them
// around so that they are dropped once all copies of the trigger are destroyed as well.
#[allow(dead_code)]
pub struct CancelMemorySample(CancelAtomic, Arc<CancelMemorySampleCore>);

impl CancellationTrigger for CancelMemorySample {
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }

    fn type_name(&self) -> &'static str {
        "CancelMemorySample"
    }
}

impl CancelMemorySample {
    /// Create a new [`CancelMemorySample`] that will be canceled once the given memory `limit`
    /// (in bytes) is exceeded. Memory usage is sampled at the given `sample_interval`.
    ///
    /// The `sample_interval` must be greater than zero.
    pub fn start(limit: usize, sample_interval: Duration) -> Self {
        let trigger = CancelAtomic::default();
        let core = CancelMemorySampleCore::start(trigger.clone(), limit, sample_interval);
        trace!(
            "`CancelMemorySample[{:p}]` started; Sampling every {}ms (limit: {} bytes).",
            trigger.id_ref(),
            sample_interval.as_millis(),
            limit
        );
        CancelMemorySample(trigger, Arc::new(core))
    }
}

/// An internal data structure that manages the sampler thread required by [`CancelMemorySample`].
/// In particular, it is responsible for safely shutting down the sampler thread once the trigger
/// is no longer needed.
#[derive(Debug)]
struct CancelMemorySampleCore {
    trigger: CancelAtomic,
    sampler_thread: Option<JoinHandle<()>>,
    stop_trigger: Sender<()>,
}

impl CancelMemorySampleCore {
    pub fn start(trigger: CancelAtomic, mem_limit_bytes: usize, sample_interval: Duration) -> Self {
        assert!(
            !sample_interval.is_zero(),
            "`sample_interval` must be greater than zero"
        );
        let trigger_copy = trigger.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            loop {
                // If this is `Ok`, it means the sampler got stopped.
                // If it is `Err`, it means the sampling interval elapsed.
                match receiver.recv_timeout(sample_interval) {
                    Ok(()) => break,
                    Err(_) => {
                        if let Some(stats) = memory_stats::memory_stats()
                            && stats.physical_mem > mem_limit_bytes
                        {
                            trace!(
                                "`CancelMemorySample[{:p}]` canceled (limit: {}; used: {}).",
                                trigger_copy.id_ref(),
                                mem_limit_bytes,
                                stats.physical_mem
                            );
                            trigger_copy.cancel();
                            break;
                        }
                    }
                }
            }
        });
        CancelMemorySampleCore {
            trigger,
            sampler_thread: Some(handle),
            stop_trigger: sender,
        }
    }
}

impl Drop for CancelMemorySampleCore {
    fn drop(&mut self) {
        let thread = self
            .sampler_thread
            .take()
            .expect("Invariant violation: Sampler thread removed before drop.");

        let join = match self.stop_trigger.send(()) {
            Ok(()) => thread.join(),
            Err(_) => {
                // The receiver has already been deallocated, meaning the sampler most likely
                // detected a memory limit breach and the thread should be dead.
                if !thread.is_finished() {
                    warn!(
                        "Sampler of `CancelMemorySample[{:p}]` cannot be stopped. Possible thread leak.",
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
            panic!("Sampler thread of `CancelMemorySample` trigger panicked.");
        }
    }
}
