use crate::{CancelAtomic, CancellationTrigger, Cancelled};
use pyo3::exceptions::PyInterruptedError;
use pyo3::{PyErr, Python};

/// Run the given `action`, cancelling it if signalled by the PyO3 Python
/// interpreter using [`CancelPython`].
///
/// Note that when `pyo3` feature is enabled, [`Cancelled`] can be automatically converted
/// to [`PyInterruptedError`], so [`crate::is_cancelled`] should work in all
/// functions returning [`pyo3::PyResult`].
///
///
/// ```rust
/// # use cancel_this::{is_cancelled, Cancellable};
/// # use std::time::Duration;
/// # use pyo3::{pyfunction, PyResult, Python};
///
/// // Calling cancellable counter from python should support cancellation using normal
/// // Python interrupts.
/// #[pyfunction]
/// fn cancellable_counter(count: usize) -> PyResult<()> {
///     cancel_this::on_python(|| {
///         for _ in 0..count {
///             is_cancelled!()?;
///             std::thread::sleep(Duration::from_millis(10));
///         }
///         Ok(())
///     })
/// }
///
/// // Ideally, we would be using cancellable_counter directly in Python code, but
/// // that's really hard to do in these tests, so we try the next best thing.
///
/// // Interpreter needs to be initialized if we are to check signals on it.
/// pyo3::Python::initialize();
///
/// let result_fast = cancel_this::on_python(|| {
///     cancel_this::on_timeout(Duration::from_millis(100), || cancellable_counter(5))
/// });
/// assert!(result_fast.is_ok());
///
/// let result_slow = cancel_this::on_python(|| {
///     cancel_this::on_timeout(Duration::from_millis(100), || cancellable_counter(50))
/// });
/// assert!(result_slow.is_err());
/// ```
pub fn on_python<R, E, Action>(action: Action) -> Result<R, E>
where
    Action: FnOnce() -> Result<R, E>,
    E: From<Cancelled>,
{
    crate::on_trigger(CancelPython::default(), action)
}

/// Implementation of [`CancellationTrigger`] that is cancelled by a PyO3 Python signal
/// (see also [`Python::check_signals`]).
///
/// See also [`crate::on_python`].
#[derive(Debug, Clone, Default)]
pub struct CancelPython(CancelAtomic);

impl CancellationTrigger for CancelPython {
    fn is_cancelled(&self) -> bool {
        if self.0.is_cancelled() {
            true
        } else {
            let signal = Python::attach(|py| py.check_signals()).is_err();
            if signal {
                self.0.cancel();
                true
            } else {
                false
            }
        }
    }

    fn type_name(&self) -> &'static str {
        "CancelPython"
    }
}

impl From<Cancelled> for PyErr {
    fn from(value: Cancelled) -> Self {
        PyInterruptedError::new_err(value.to_string())
    }
}
