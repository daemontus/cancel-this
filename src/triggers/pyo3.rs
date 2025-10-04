use crate::{CancellationTrigger, Cancelled};
use lazy_static::lazy_static;
use pyo3::exceptions::PyInterruptedError;
use pyo3::{PyErr, Python};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Run the given `action`, cancelling it if signalled by the PyO3 Python
/// interpreter using [`CancelPython`].
///
/// Note that when `pyo3` feature is enabled, [`Cancelled`] can be automatically converted
/// to [`PyInterruptedError`], so [`crate::is_cancelled`] should work in all
/// functions returning [`pyo3::PyResult`].
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

lazy_static! {
    /// A counter that increments every time we detect Python signal interrupt.
    static ref PYTHON_GLOBAL_MONITOR: Arc<AtomicU64> = {
        let monitor = Arc::new(AtomicU64::new(0));
        let thread_monitor = monitor.clone();

        // Start an observer thread that wakes up every millisecond and checks
        // if cancellation has happened or not. This thread never stops.
        std::thread::spawn(move || {
            loop {
                Python::try_attach(|py| {
                    if py.check_signals().is_err() {
                        thread_monitor.fetch_add(1, Ordering::SeqCst);
                    }
                });
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        });

        monitor
    };
}

/// Implementation of [`CancellationTrigger`] that is cancelled by a PyO3 Python signal
/// (see also [`Python::check_signals`]). With the current implementation,
/// [`Python::check_signals`] is called at most once every millisecond, meaning cancellation
/// more granular than `1ms` is not supported.
///
/// See also [`crate::on_python`].
#[derive(Debug, Clone)]
pub struct CancelPython(u64);

impl Default for CancelPython {
    fn default() -> Self {
        CancelPython(PYTHON_GLOBAL_MONITOR.load(Ordering::SeqCst))
    }
}

impl CancellationTrigger for CancelPython {
    fn is_cancelled(&self) -> bool {
        let current = PYTHON_GLOBAL_MONITOR.load(Ordering::SeqCst);
        current != self.0
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
