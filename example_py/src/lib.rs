use cancel_this::{is_cancelled, LivenessGuard};
use pyo3::prelude::*;
use std::hash::{DefaultHasher, Hasher};
use std::time::Duration;

#[pyclass]
#[allow(unused)] // We only have to guarantee it is not dropped. We don't need to actually read it.
struct Liveness(LivenessGuard);

#[pymethods]
impl Liveness {
    #[new]
    pub fn new() -> Liveness {
        let guard = LivenessGuard::new(Duration::from_secs(1), |it| {
            if it {
                println!(" >> [Liveness] Computation became responsive.")
            } else {
                println!(" >> [Liveness] Computation became unresponsive.")
            }
        });

        Liveness(guard)
    }
}

#[pyfunction]
fn hash_data(data: Vec<u64>) -> PyResult<u64> {
    cancel_this::on_python(|| {
        let mut hasher = DefaultHasher::new();
        for x in data {
            is_cancelled!()?;
            hasher.write_u64(x);
            // Slow down the computation so that we don't have to use large data buffers.
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok(hasher.finish())
    })
}

#[pyfunction]
fn hash_data_unchecked(data: Vec<u64>) -> PyResult<u64> {
    let mut hasher = DefaultHasher::new();
    for x in data {
        hasher.write_u64(x);
        // Slow down the computation so that we don't have to use large data buffers.
        std::thread::sleep(Duration::from_millis(1));
    }
    Ok(hasher.finish())
}

/// A Python module implemented in Rust. The name of this function must match
/// the `lib.name` setting in the `Cargo.toml`, else Python will not be able to
/// import the module.
#[pymodule]
fn example_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(hash_data, m)?)?;
    m.add_function(wrap_pyfunction!(hash_data_unchecked, m)?)?;
    m.add_class::<Liveness>()?;
    Ok(())
}
