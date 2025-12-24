use dyn_clone::{DynClone, clone_trait_object};

mod timer;
pub use timer::*;

mod chain;
pub use chain::*;

mod never;
pub use never::*;

mod atomic;
pub use atomic::*;

#[cfg(feature = "ctrlc")]
mod ctrlc;
#[cfg(feature = "ctrlc")]
pub use ctrlc::*;

#[cfg(feature = "pyo3")]
mod pyo3;
#[cfg(feature = "pyo3")]
pub use pyo3::*;

/// Defines an object that can be used to trigger cancellation.
///
/// In general, we only require that the object can be shared between threads and that it
/// can be safely cloned.
///
/// **The expectation is that cloning a cancellation trigger produces an object that reacts to
/// the same signal, i.e., is cancelled if and only if the original object is cancelled.**
///
pub trait CancellationTrigger: Send + Sync + DynClone {
    /// Returns true if this trigger is cancelled.
    ///
    /// In normal conditions, once a trigger is cancelled, it stays cancelled and should
    /// not be able to reset.
    fn is_cancelled(&self) -> bool;

    /// Return the type name of this [`CancellationTrigger`], or in case of "composite"
    /// triggers, *the type name of the trigger that actually signalled the cancellation*.
    fn type_name(&self) -> &'static str;
}

clone_trait_object!(CancellationTrigger);

/// A dynamic boxed [`CancellationTrigger`].
pub type DynamicCancellationTrigger = Box<dyn CancellationTrigger>;

impl CancellationTrigger for DynamicCancellationTrigger {
    fn is_cancelled(&self) -> bool {
        self.as_ref().is_cancelled()
    }

    fn type_name(&self) -> &'static str {
        self.as_ref().type_name()
    }
}
