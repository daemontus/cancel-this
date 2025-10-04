use cancel_this::{CancelAtomic, Cancellable, is_cancelled};
use criterion::{Criterion, criterion_group, criterion_main};
use std::hash::{DefaultHasher, Hasher};
use std::hint::black_box;
use std::time::Duration;

/// A function that hashes given data using the default hash function.
fn default_hash_data(data: &[u64]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for x in data {
        hasher.write_u64(*x);
    }
    hasher.finish()
}

/// The same as [`default_hash_data`], but allows the task to be cancelled during each iteration.
fn cancellable_hash_data(data: &[u64]) -> Cancellable<u64> {
    let mut hasher = DefaultHasher::new();
    for x in data {
        is_cancelled!()?;
        hasher.write_u64(*x);
    }
    Ok(hasher.finish())
}

/// The same as [`default_hash_data`], but allows the task to be cancelled during each iteration.
fn cached_cancellable_hash_data(data: &[u64]) -> Cancellable<u64> {
    let trigger = cancel_this::active_triggers();
    let mut hasher = DefaultHasher::new();
    for x in data {
        is_cancelled!(trigger)?;
        hasher.write_u64(*x);
    }
    Ok(hasher.finish())
}

/// Finally, the same as [`cancellable_hash_data`], but using async functions.
async fn async_hash_data(data: &[u64]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for x in data {
        // This is actually important to allow the task to be cancelled, because cancellation
        // can only happen at await points.
        tokio::task::yield_now().await;
        hasher.write_u64(*x);
    }
    hasher.finish()
}

fn criterion_benchmark(c: &mut Criterion) {
    // Some not-so-random test data.
    let data = (0u64..(1 << 10)).collect::<Vec<_>>();

    // Check benchmark parameters to create a proper key/prefix.
    let liveness_enabled = cfg!(feature = "liveness");
    let bench_key = format!("(data={}, liveness={})", data.len(), liveness_enabled);
    let bench_prefix = "hash";

    // Check performance when the operation cannot be cancelled.
    c.bench_function(
        format!("{bench_prefix}::synchronous; {bench_key}").as_str(),
        |b| b.iter(|| default_hash_data(black_box(&data))),
    );

    // Run the same code using tokio as async operation.
    let rt = tokio::runtime::Runtime::new().unwrap();
    c.bench_function(
        format!("{bench_prefix}::async::tokio; {bench_key}").as_str(),
        |b| b.to_async(&rt).iter(|| async_hash_data(black_box(&data))),
    );

    // Check performance when the operation is cancellable but no cancellation
    // handler is registered at runtime.
    c.bench_function(
        format!("{bench_prefix}::cancellable::none; {bench_key}").as_str(),
        |b| b.iter(|| cancellable_hash_data(black_box(&data))),
    );

    // Check performance when the operation is cancellable but no cancellation
    // handler is registered at runtime.
    c.bench_function(
        format!("{bench_prefix}::cancellable::none::cached; {bench_key}").as_str(),
        |b| b.iter(|| cached_cancellable_hash_data(black_box(&data))),
    );

    // Check cancellation using atomic trigger.
    let trigger = CancelAtomic::default();
    let r: Cancellable<()> = cancel_this::on_atomic(trigger, || {
        c.bench_function(
            format!("{bench_prefix}::cancellable::atomic; {bench_key}").as_str(),
            |b| b.iter(|| cancellable_hash_data(black_box(&data))),
        );
        Ok(())
    });
    assert!(r.is_ok());

    let trigger = CancelAtomic::default();
    let r: Cancellable<()> = cancel_this::on_atomic(trigger, || {
        c.bench_function(
            format!("{bench_prefix}::cancellable::atomic::cached; {bench_key}").as_str(),
            |b| b.iter(|| cached_cancellable_hash_data(black_box(&data))),
        );
        Ok(())
    });
    assert!(r.is_ok());

    /*
       Fundamentally, these should not be any slower,
       because internally they use atomic triggers.
    */

    // Check cancellation using timeout.
    let r: Cancellable<()> = cancel_this::on_timeout(Duration::from_secs(600), || {
        c.bench_function(
            format!("{bench_prefix}::cancellable::timeout; {bench_key}").as_str(),
            |b| b.iter(|| cancellable_hash_data(black_box(&data))),
        );
        Ok(())
    });
    assert!(r.is_ok());

    // Check cancellation using SIGINT.
    let r: Cancellable<()> = cancel_this::on_sigint(|| {
        c.bench_function(
            format!("{bench_prefix}::cancellable::sigint; {bench_key}").as_str(),
            |b| b.iter(|| cancellable_hash_data(black_box(&data))),
        );
        Ok(())
    });
    assert!(r.is_ok());

    // Check cancellation using Python interpreter.
    // Ideally, this would be using real Python functions, but that's
    // a bit cumbersome to actually setup.
    pyo3::Python::initialize();
    let r: Cancellable<()> = cancel_this::on_python(|| {
        c.bench_function(
            format!("{bench_prefix}::cancellable::python; {bench_key}").as_str(),
            |b| b.iter(|| cancellable_hash_data(black_box(&data))),
        );
        Ok(())
    });
    assert!(r.is_ok());
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
