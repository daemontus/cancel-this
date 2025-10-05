use crate::{CancelAtomic, CancellationTrigger, Cancelled};
use lazy_static::lazy_static;
use log::warn;
use pyo3::exceptions::{PyInterruptedError, PyKeyboardInterrupt};
use pyo3::prelude::PyAnyMethods;
use pyo3::{PyErr, PyResult, Python};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Run the given `action`, cancelling it if signalled by the PyO3 Python
/// interpreter using [`CancelPython`].
///
/// **Error handling:** When the `pyo3` feature is enabled, [`Cancelled`] can be
/// automatically converted to [`PyInterruptedError`], so [`crate::is_cancelled`] should work
/// in all functions returning [`pyo3::PyResult`].
///
/// **Multi-threading:** Using [`CancelPython`] only *works* if cancellation is checked on the
/// *main thread* of the Python interpreter. As such, Python cancellation is not truly "thread
/// safe", because copies of [`CancelPython`] running on different threads will be triggered
/// only if cancellation is actively checked by the main thread too. However, the current
/// implementation should guarantee that if one copy of the same [`CancelPython`] instance is
/// cancelled (the one running on the main thread), then all copies are cancelled. So it can
/// be still used to cancel multithreaded operations, but cancellation has to be actively
/// checked by the main thread.
///
/// **To notify about this issue, [`on_python`] raises a `warn` log message anytime it is
/// used outside the main thread.**
///
///
/// ```rust
/// # use cancel_this::{is_cancelled, Cancellable};
/// # use std::time::Duration;
/// # use pyo3::{pyfunction, PyResult, Python};
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
/// // Still, this initialization method does not actually create any "main thread",
/// // so interrupts can't *really* be checked.
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
    if !IS_MAIN_THREAD.try_with(|it| *it).unwrap_or_default() {
        warn!(
            "Using `cancel_this::on_python` outside main Python thread. Cancellation may not work."
        );
    }
    crate::on_trigger(CancelPython::default(), action)
}

lazy_static! {
    /// A counter that increments every millisecond so that
    /// we can "debounce" the cancellation checks.
    static ref PYTHON_DEBOUNCE_COUNTER: Arc<AtomicU64> = {
        let monitor = Arc::new(AtomicU64::new(0));
        let thread_monitor = monitor.clone();

        // Start an observer thread that wakes up every millisecond and updates
        // the atomic value to notify cancellation triggers that they should check
        // signals again. Once started, this thread runs until the application
        // is terminated.
        std::thread::spawn(move || {
            loop {
                thread_monitor.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        });

        monitor
    };
}

thread_local! {
    /// A thread-local constant which evaluates to `true` if this thread
    /// is the main interpreter thread.
    static IS_MAIN_THREAD: bool = check_main_thread();
}

/// Implementation of [`CancellationTrigger`] that is cancelled by a PyO3 Python signal
/// (see also [`Python::check_signals`]). To reduce overhead, current implementation only
/// calls [`Python::check_signals`] at most once every millisecond, meaning cancellation
/// more granular than `1ms` is not supported.
///
/// See also [`crate::on_python`].
#[derive(Debug, Clone)]
pub struct CancelPython(Arc<AtomicU64>, CancelAtomic);

impl Default for CancelPython {
    fn default() -> Self {
        let recent = Arc::new(AtomicU64::new(
            PYTHON_DEBOUNCE_COUNTER.load(Ordering::SeqCst),
        ));
        CancelPython(recent, CancelAtomic::default())
    }
}

impl CancellationTrigger for CancelPython {
    fn is_cancelled(&self) -> bool {
        // If this trigger was cancelled before, we should see this on the internal trigger.
        if self.1.is_cancelled() {
            return true;
        }

        // It only makes sense to check the "actual" cancellation on the main thread.
        if IS_MAIN_THREAD.try_with(|it| *it).unwrap_or_default() {
            let current = PYTHON_DEBOUNCE_COUNTER.load(Ordering::SeqCst);
            let recent = self.0.load(Ordering::SeqCst);
            if current != recent {
                // If these values are different, it means it has been more than one millisecond
                // since we last checked the Python interpreter...
                self.0.store(current, Ordering::SeqCst);

                let is_cancelled =
                    Python::try_attach(|py| py.check_signals().is_err()).unwrap_or_default();

                if is_cancelled {
                    self.1.cancel();
                    return true;
                }
            }
        }

        false
    }

    fn type_name(&self) -> &'static str {
        "CancelPython"
    }
}

impl From<Cancelled> for PyErr {
    fn from(value: Cancelled) -> Self {
        if value.cause() == "CancelPython" {
            PyKeyboardInterrupt::new_err(value.to_string())
        } else {
            PyInterruptedError::new_err(value.to_string())
        }
    }
}

/// Uses Python API to detect if the current code is running on the main thread.
///
/// This is a relatively costly operation and its results should probably be cached
/// as much as possible.
fn check_main_thread() -> bool {
    fn check(py: Python) -> PyResult<bool> {
        // Import the 'threading' module
        let threading = py.import("threading")?;

        // Get references to main_thread and current_thread
        let main_thread = threading.getattr("main_thread")?.call0()?;
        let current_thread = threading.getattr("current_thread")?.call0()?;

        // Compare if the current thread is the main thread
        Ok(main_thread.is(&current_thread))
    }

    Python::try_attach(|py| check(py).unwrap_or_default()).unwrap_or_default()
}
