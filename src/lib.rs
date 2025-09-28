//!
//! ### Simple example
//!
//! ```rust
//! use std::time::Duration;
//! use cancel_this::{Cancellable, is_cancelled};
//!
//! fn cancellable_counter(count: usize) -> Cancellable<()> {
//!     for _ in 0..count {
//!         is_cancelled!()?;
//!         std::thread::sleep(Duration::from_millis(10));
//!     }
//!     Ok(())
//! }
//!
//! let result: Cancellable<()> = cancel_this::on_timeout(Duration::from_secs(1), || {
//!     cancellable_counter(5)?;
//!     cancellable_counter(10)?;
//!     cancellable_counter(100)?;
//!     Ok(())
//! });
//!
//! assert!(result.is_err());
//! ```
//!
//! ## Complex example
//!
//! This example uses most of the features, including error conversion and never-cancel blocks.
//!
//! ```rust
//! use std::time::Duration;
//! use cancel_this::{Cancelled, is_cancelled};
//!
//! enum ComputeError {
//!     Zero,
//!     Cancelled
//! }
//!
//! impl From<Cancelled> for ComputeError {
//!     fn from(value: Cancelled) -> Self {
//!        ComputeError::Cancelled
//!    }
//! }
//!
//!
//! fn compute(input: u32) -> Result<String, ComputeError> {
//!     if input == 0 {
//!         Err(ComputeError::Zero)
//!     } else {
//!         let mut total: u32 = 0;
//!         for _ in 0..input {
//!             total += input;
//!             is_cancelled!()?;
//!             std::thread::sleep(Duration::from_millis(10));
//!         }
//!         Ok(total.to_string())
//!     }
//! }
//!
//! let result: Result<String, ComputeError> = cancel_this::on_timeout(Duration::from_millis(200), || {
//!     // This one should go through
//!     let r1 = compute(5)?;
//!     assert_eq!(r1.as_str(), "25");
//!     // This will be cancelled. Instead of using `?`, we check
//!     // that the operation actually got cancelled.
//!     let r2 = compute(20);
//!     assert!(matches!(r2, Err(ComputeError::Cancelled)));
//!     // Even though the execution is now cancelled, we can still execute code in
//!     // the "cancel-never" blocks.
//!     let r3 = cancel_this::never(|| compute(10))?;
//!     assert_eq!(r3.as_str(), "100");
//!     compute(10) // This should get immediately canceled.
//! });
//!
//! ```
//!

/// Cancellation error type.
mod error;

/// Various types of triggers, including corresponding `when_*` helper functions.
mod triggers;

pub use error::*;
use std::cell::RefCell;
pub use triggers::*;

pub const UNKNOWN_CAUSE: &str = "UnknownCancellationTrigger";

thread_local! {
    /// The correct usage of this value lies in the fact that references to the `CancelChain`
    /// will never leak out of this crate (we can hand out copies to the downstream users).
    /// Within the crate, the triggers are either read when checking cancellation status, or
    /// written when entering/leaving scope. However, these two actions are never performed
    /// simultaneously.
    static TRIGGER: RefCell<CancelChain> = RefCell::new(CancelChain::default());
}

#[macro_export]
macro_rules! is_cancelled {
    () => {
        $crate::check_local_cancellation()
    };
    ($handler:ident) => {
        $crate::check_cancellation($handler)
    };
}

/// Returns [`Cancelled`] if [`CancellationTrigger::is_cancelled`] of the given
/// `trigger` is true. In typical situations, you don't use this method directly,
/// but instead use the [`is_cancelled`] macro.
pub fn check_cancellation<TCancel: CancellationTrigger>(
    trigger: &TCancel,
) -> Result<(), Cancelled> {
    if trigger.is_cancelled() {
        Err(Cancelled::new(trigger.type_name()))
    } else {
        Ok(())
    }
}

/// Check if the current thread-local cancellation trigger is cancelled. In typical situations,
/// you don't use this method directly, but instead use the [`is_cancelled`] macro.
///
/// To avoid a repeated borrow of the thread-local value in performance-sensitive applications,
/// you can use [`clone_trigger`] to cache the value in a local variable.
pub fn check_local_cancellation() -> Result<(), Cancelled> {
    TRIGGER.with_borrow(check_cancellation)
}

/// Get a snapshot of the current thread-local cancellation trigger.
///
/// This value can be either used to initialize triggers in a new thread using [`on_trigger`],
/// or used directly as argument to the [`is_cancelled`] macro to speed up cancellation checks.
pub fn clone_trigger() -> DynamicCancellationTrigger {
    TRIGGER.with_borrow(|trigger| trigger.clone_and_flatten())
}

/// Run the `action` in a context where a cancellation can be signaled using the given `trigger`.
///
/// Once the action is completed, the trigger is de-registered and does not apply
/// to further code execution.
pub fn on_trigger<TResult, TError, TCancel, TAction>(
    trigger: TCancel,
    action: TAction,
) -> Result<TResult, TError>
where
    TCancel: CancellationTrigger + 'static,
    TAction: FnOnce() -> Result<TResult, TError>,
    TError: From<Cancelled>,
{
    TRIGGER.with_borrow_mut(|thread_trigger| thread_trigger.push(trigger));
    let result = action();
    TRIGGER.with_borrow_mut(|thread_trigger| thread_trigger.pop());
    result
}
