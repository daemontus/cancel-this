use std::fmt::{Debug, Display, Formatter};

/// Cancellation error type. Should include the cause of cancellation (name of the
/// [`crate::CancellationTrigger`] type that caused the error).
///
/// In cases where the operation itself can result in an error `E`, make sure to implement
/// `From<Cancelled>` for `E`, meaning you'll still be able to use
/// the `is_cancelled` macro and other features of this crate.
#[derive(Clone, Debug)]
pub struct Cancelled {
    cause: &'static str,
}

/// A result of a cancellable operation.
pub type Cancellable<TResult> = Result<TResult, Cancelled>;

impl Cancelled {
    /// Create a new [`Cancelled`] with a cause type.
    pub fn new(cause: &'static str) -> Self {
        Cancelled { cause }
    }
}

impl Display for Cancelled {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Operation cancelled (caused by `{}`)", self.cause)
    }
}

impl std::error::Error for Cancelled {}

impl Default for Cancelled {
    fn default() -> Self {
        Cancelled::new(crate::UNKNOWN_CAUSE)
    }
}

impl Cancelled {
    /// The name of the [`crate::CancellationTrigger`] that caused the error. If the cause is unknown,
    /// use [`crate::UNKNOWN_CAUSE`].
    pub fn cause(&self) -> &'static str {
        self.cause
    }
}
