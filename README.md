[![Crates.io](https://img.shields.io/crates/v/cancel-this?style=flat-square)](https://crates.io/crates/cancel-this)
[![Api Docs](https://img.shields.io/badge/docs-api-yellowgreen?style=flat-square)](https://docs.rs/cancel-this/)
[![Continuous integration](https://img.shields.io/github/actions/workflow/status/daemontus/cancel-this/build.yml?branch=main&style=flat-square)](https://github.com/daemontus/cancel-this/actions/workflows/build.yml)
[![Benchmarks](https://img.shields.io/github/actions/workflow/status/daemontus/cancel-this/bench_base.yml?branch=main&style=flat-square&label=bench)](https://bencher.dev/perf/cancel-this/)
[![Coverage](https://img.shields.io/codecov/c/github/daemontus/cancel-this?style=flat-square)](https://codecov.io/gh/daemontus/cancel-this)
[![GitHub issues](https://img.shields.io/github/issues/daemontus/cancel-this?style=flat-square)](https://github.com/daemontus/cancel-this/issues)
[![GitHub last commit](https://img.shields.io/github/last-commit/daemontus/cancel-this?style=flat-square)](https://github.com/daemontus/cancel-this/commits/main)
[![Crates.io](https://img.shields.io/crates/l/cancel-this?style=flat-square)](https://github.com/daemontus/cancel-this/blob/main/LICENSE)

# `cancel_this` (Rust co-op cancellation)

This crate provides a user-friendly way to implement cooperative 
cancellation in Rust based on a wide range of criteria, including
*triggers*, *timers*, *OS signals* (Ctrl+C), or the *Python 
interpreter linked using PyO3*. It also provides liveness monitoring
of "cancellation aware" code.

**Why not use `async` instead of cooperative cancellation?** In principle,
`async` was designed to solve a different problem, and that's executing IO-bound 
tasks in a non-blocking fashion. It is not *really* designed for CPU-bound tasks. 
Consequently, using `async` adds a lot of unnecessary overhead to your project
which `cancel_this` does not have (see also the *Performance* section below).

**Why not use [`stop-token`](https://crates.io/crates/stop-token), 
[`CancellationToken`](https://docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html) 
or other cooperative cancellation crates?** So far, all crates I have seen require you
to pass the cancellation token around and generally do not make it easy to
combine the effects of multiple tokens. In `cancel_this`, the goal was to 
make cancellation dead simple: You register however many cancellation triggers 
you want, each trigger is valid within a specific scope (and thread), and can be checked
by a macro anywhere in your code.

### Current features

 - Scoped cancellation using thread-local "cancellation triggers".
 - Out-of-the box support for triggers based on atomics and timers.
 - With feature `ctrlc` enabled, support for cancellation using `SIGINT` signals.
 - With feature `pyo3` enabled, support for cancellation using `Python::check_signals`.
 - With feature `liveness` enabled, you can register a per-thread handler invoked
   once the thread becomes unresponsive (i.e. cancellation is not checked periodically
   withing the desired interval).
 - Practically no overhead in cancellable code when cancellation is not enabled.
 - Very small overhead for "atomic-based" cancellation triggers, acceptable overhead for PyO3 cancellation.
 - All triggers and guards generate [`log`](https://crates.io/crates/log) messages (`trace` for normal operation, 
   `warn` for issues where panic can be avoided).

### Simple example

A simple counter that is eventually cancelled by a one-second timeout:

```rust
use std::time::Duration;
use cancel_this::{Cancellable, is_cancelled};

fn cancellable_counter(count: usize) -> Cancellable<()> {
   for _ in 0..count {
      is_cancelled!()?;
      std::thread::sleep(Duration::from_millis(10));
   }
   Ok(())
}

fn main() {
   let one_s = Duration::from_secs(1);
   let result: Cancellable<()> = cancel_this::on_timeout(one_s, || {
      cancellable_counter(5)?;
      cancellable_counter(10)?;
      cancellable_counter(100)?;
      Ok(())
   });
    
   assert!(result.is_err());   
}
```

### Performance

The overall overhead of adding cancellation checks will **heavily** depend on how often they are performed.
Under ideal conditions, you don't want to run them too often. However, delaying cancellation too much can make
your code seem unresponsive. In `./benches`, we provide a benchmark to illustrate the impact of cancellation
on simple code. Here, we intentionally use cancellation checks too often to gain significant overhead. In your
own code, it is typically sufficient to run cancellation every few milliseconds.

#### Sample results

Benchmarks with `liveness=true` are running with liveness monitoring (this adds additional overhead). 
The `synchronous` benchmark is a baseline without any cancellation support. 
The `async::tokio` benchmark implements cancellation using `async` functions.
The `cancellable::none` benchmark implements cancellation using `cancel_this`, but with no trigger registered.
Remaining benchmarks test different "cancellation triggers" implemented in `cancel_this`.

These results were obtained
on a M2 Max Macbook Pro using `cargo bench` (the exact output is simplified for brevity). Latest results 
from a more stable desktop environment are also available on [bencher.dev](https://bencher.dev/perf/cancel-this/)
or in the relevant [CI run](https://github.com/daemontus/cancel-this/actions/workflows/bench_base.yml).

```
hash::synchronous; (data=1024, liveness=false)           4.0006 µs

hash::async::tokio; (data=1024, liveness=false)          17.076 µs

hash::cancellable:none; (data=1024, liveness=false)      4.0369 µs
hash::cancellable:none; (data=1024, liveness=true)       7.6464 µs

hash:::cancellable::atomic; (data=1024, liveness=false)  4.9599 µs
hash:::cancellable::atomic; (data=1024, liveness=true)   7.6691 µs

hash:::cancellable::timeout; (data=1024, liveness=false) 4.9626 µs
hash:::cancellable::timeout; (data=1024, liveness=true)  7.7143 µs

hash:::cancellable::sigint; (data=1024, liveness=false)  4.9717 µs
hash:::cancellable::sigint; (data=1024, liveness=true)   7.7038 µs

hash:::cancellable::python; (data=1024, liveness=false)  79.738 µs
hash:::cancellable::python; (data=1024, liveness=true)   82.695 µs
```

To run the benchmarks locally, simply use `cargo bench --all-features` (with liveness turned on) or 
`cargo bench --features=ctrlc --features=pyo3` (liveness turned off).