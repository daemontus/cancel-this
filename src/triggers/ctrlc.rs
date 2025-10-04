use crate::{CancelAtomic, CancellationTrigger, Cancelled};
use lazy_static::lazy_static;
use log::trace;
use std::sync::Mutex;

/// Run the given `action`, cancelling it using [`CancelCtrlc`] if the `SIGINT` signal (Ctrl+C)
/// is detected.
///
/// ```rust
/// # use std::time::Duration;
/// # use cancel_this::{Cancelled, is_cancelled};
///
/// fn cancellable_counter(count: usize) -> Result<(), Cancelled> {
///     for _ in 0..count {
///         is_cancelled!()?;
///         std::thread::sleep(Duration::from_millis(10));
///     }
///     Ok(())
/// }
///
/// std::thread::spawn(|| {
///     // Wait for 100ms and then trigger SIGINT.
///     std::thread::sleep(Duration::from_millis(100));
///     let pid = std::process::id() as libc::pid_t; // Get current process ID
///     unsafe { assert_eq!(libc::kill(pid, libc::SIGINT), 0) }
/// });
///
/// // First action is fast (30ms) and should complete before SIGINT is triggered.
/// let result_fast = cancel_this::on_sigint(|| cancellable_counter(3));
/// assert!(result_fast.is_ok());
///
/// // Second action is slow and will be cancelled by SIGINT.
/// let result_slow = cancel_this::on_sigint(|| cancellable_counter(50));
/// assert!(result_slow.is_err());
/// ```
///
/// # Panics
/// The operation can panic if the internal handler of the `ctrlc` crate has been already set
/// by some other piece of code that is not controlled by this crate (multiple instances of
/// the [`CancelCtrlc`] trigger should be managed by this crate safely).
pub fn on_sigint<TResult, TError, TAction>(action: TAction) -> Result<TResult, TError>
where
    TAction: FnOnce() -> Result<TResult, TError>,
    TError: From<Cancelled>,
{
    crate::on_trigger(CancelCtrlc::default(), action)
}

/// Private global list of items waiting for ctrl+c to be pressed.
static WAITING_FOR_CTRLC: Mutex<Vec<CancelAtomic>> = Mutex::new(Vec::new());

lazy_static! {
    /// The result of ctrlc initialization, called exactly once before the first use of the
    /// ctrlc functionality.
    static ref CTRLC_INITIALIZED: Result<(), ctrlc::Error> = ctrlc::try_set_handler(|| {
        let mut guard = WAITING_FOR_CTRLC.lock()
            .expect("Global state of `CancelCtrlc` is corrupted.");

        // Go through all the pending triggers and cancel them.
        let total = guard.len();
        trace!("Received SIGINT. Cancelling triggers ({total} total).");
        while let Some(to_trigger) = guard.pop() {
            to_trigger.cancel();
        }
    });
}

/// Implementation of [`CancellationTrigger`] that is cancelled when SIGINT (Ctrl+C)
/// is triggered.
///
/// This uses the `ctrlc` crate to observe the SIGINT events. As such, it
/// needs to call `ctrlc::set_handler` upon first use, meaning it can fail if
/// other features in your code also use `ctrlc`.
///
/// See also [`crate::on_sigint`].
///
/// ## Logging
///  - [`trace`] Every time the SIGINT event is processed, the number of affected triggers
///    is listed. Each trigger should also produce a message once actually cancelled.
#[derive(Debug, Clone)]
pub struct CancelCtrlc(CancelAtomic);

impl CancellationTrigger for CancelCtrlc {
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }

    fn type_name(&self) -> &'static str {
        "CancelCtrlc"
    }
}

impl Default for CancelCtrlc {
    fn default() -> Self {
        Self::try_new().unwrap()
    }
}

impl CancelCtrlc {
    /// Try to create a new instance of [`CancelCtrlc`], returning an error if
    /// the initialization of the `ctrlc` handler wasn't successful.
    ///
    /// Note that the initialization only runs once, i.e. if the method fails once, it will
    /// fail every time.
    pub fn try_new() -> Result<Self, &'static ctrlc::Error> {
        match CTRLC_INITIALIZED.as_ref() {
            Err(e) => Err(e),
            Ok(_) => {
                let trigger = CancelAtomic::default();
                let mut guard = WAITING_FOR_CTRLC
                    .lock()
                    .expect("Global state of `CancelCtrlc` is corrupted.");
                guard.push(trigger.clone());
                Ok(CancelCtrlc(trigger))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::CancelCtrlc;

    #[test]
    fn ctrlc_twice() {
        ctrlc::set_handler(|| {
            unimplemented!();
        })
        .unwrap();

        let trigger = CancelCtrlc::try_new();
        assert!(trigger.is_err());
    }
}
