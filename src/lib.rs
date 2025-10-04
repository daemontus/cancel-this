//! This crate provides a user-friendly way to implement cooperative
//! cancellation in Rust based on a wide range of criteria, including
//! *triggers*, *timers*, *OS signals* (Ctrl+C), or the *Python
//! interpreter linked using PyO3*. It also provides liveness monitoring
//! of "cancellation-aware" code.
//!
//! *Why not use `async` instead of cooperative cancellation?* Simply put, `async`
//! adds a lot of other "weight" to your project that you might not need/want. With
//! `cancel_this`, you can add your own cancellation logic with minimal impact on
//! your project's footprint.
//!
//! *Why not use [`stop-token`](https://crates.io/crates/stop-token) or other
//! cooperative cancellation crates?* So far, all crates I have seen require you
//! to pass the cancellation token around and generally do not make it easy to
//! combine the effects of multiple tokens. In `cancel_this`, the goal was to
//! make cancellation dead simple: You register however many cancellation triggers
//! you want, each trigger is valid within a specific scope, and can be checked
//! by a macro anywhere in your code.
//!
//! ### Current features
//!
//! - Scoped cancellation using thread-local "cancellation triggers".
//! - Out-of-the box support for triggers based on atomics and timers.
//! - With feature `ctrlc` enabled, support for cancellation using `SIGINT` signals.
//! - With feature `pyo3` enabled, support for cancellation using `Python::check_signals`.
//! - With feature `liveness` enabled, you can register a per-thread handler which is invoked
//!   every time the thread becomes unresponsive (i.e. cancellation check has not been performed
//!   withing the prescribed interval).
//!
//! ### Simple example
//!
//! A simple counter that is eventually cancelled by a one-second timeout:
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
//! This example uses most of the features, including error conversion, never-cancel blocks,
//! and liveness monitoring.
//!
//! ```rust
//! use std::time::Duration;
//! use cancel_this::{Cancelled, is_cancelled, LivenessGuard};
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
//! let guard = LivenessGuard::new(Duration::from_secs(2), |is_alive| {
//!     eprintln!("Thread has not responded in the last two seconds.");
//! });
//!
//! let result: Result<String, ComputeError> = cancel_this::on_timeout(Duration::from_millis(200), || {
//!     let r1 = cancel_this::on_sigint(|| {
//!         // This operation can be canceled using Ctrl+C, but the timeout still applies.
//!         compute(5)
//!     })?;
//!
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
//! // The liveness monitoring is active while `guard` is in scope. Once `guard` is dropped here,
//! // the liveness monitoring is turned off as well.
//! ```
//!

/// Cancellation error type.
mod error;

/// Various types of triggers, including corresponding `when_*` helper functions.
mod triggers;

/// Implements a "liveness guard", which monitors the frequency with which cancellations are
/// checked, making sure the process
#[cfg(feature = "liveness")]
mod liveness;
#[cfg(feature = "liveness")]
pub use liveness::*;

#[cfg(not(feature = "liveness"))]
mod liveness {
    #[derive(Clone, Default)]
    pub(crate) struct LivenessInterceptor<R: crate::CancellationTrigger + Clone>(R);

    impl<R: crate::CancellationTrigger + Clone> LivenessInterceptor<R> {
        pub fn as_inner_mut(&mut self) -> &mut R {
            &mut self.0
        }

        pub fn as_inner(&self) -> &R {
            &self.0
        }
    }

    impl LivenessInterceptor<crate::CancelChain> {
        pub fn clone_and_flatten(&self) -> crate::triggers::DynamicCancellationTrigger {
            // If liveness monitoring is off, we can just use normal flattening.
            self.as_inner().clone_and_flatten()
        }
    }

    impl<R: crate::CancellationTrigger + Clone> crate::CancellationTrigger for LivenessInterceptor<R> {
        fn is_cancelled(&self) -> bool {
            // If liveness monitoring is off, we do nothing.
            self.0.is_cancelled()
        }

        fn type_name(&self) -> &'static str {
            self.0.type_name()
        }
    }
}

pub use error::*;
use liveness::LivenessInterceptor;
use std::cell::RefCell;
pub use triggers::*;

/// The "default" [`crate::Cancelled`] cause, reported when the trigger type is unknown.
pub const UNKNOWN_CAUSE: &str = "UnknownCancellationTrigger";

thread_local! {
    /// The correct usage of this value lies in the fact that references to the `CancelChain`
    /// will never leak out of this crate (we can hand out copies to the downstream users).
    /// Within the crate, the triggers are either read when checking cancellation status, or
    /// written when entering/leaving scope. However, these two actions are never performed
    /// simultaneously.
    static TRIGGER: RefCell<LivenessInterceptor<CancelChain>> = RefCell::new(LivenessInterceptor::default());
}

/// Call this macro every time your code wants to check for cancellation. It returns
/// `Result<(), Cancelled>`, which can typically be propagated using the `?` operator.
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
    TRIGGER.with_borrow_mut(|thread_trigger| thread_trigger.as_inner_mut().push(trigger));
    let result = action();
    TRIGGER.with_borrow_mut(|thread_trigger| thread_trigger.as_inner_mut().pop());
    result
}
