[![Crates.io](https://img.shields.io/crates/v/cancel-this?style=flat-square)](https://crates.io/crates/cancel-this)
[![Api Docs](https://img.shields.io/badge/docs-api-yellowgreen?style=flat-square)](https://docs.rs/cancel-this/)
[![Continuous integration](https://img.shields.io/github/actions/workflow/status/daemontus/cancel-this/build.yml?branch=main&style=flat-square)](https://github.com/daemontus/cancel-this/actions?query=workflow%3Abuild)
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

**Why not use `async` instead of cooperative cancellation?** Simply put, `async`
adds a lot of other "weight" to your project that you might not need/want. With
`cancel_this`, you can add your own cancellation logic with minimal impact on
your project's footprint.

**Why not use [`stop-token`](https://crates.io/crates/stop-token) or other 
cooperative cancellation crates?** So far, all crates I have seen require you
to pass the cancellation token around and generally do not make it easy to
combine the effects of multiple tokens. In `cancel_this`, the goal was to 
make cancellation dead simple: You register however many cancellation triggers 
you want, each trigger is valid within a specific scope, and can be checked
by a macro anywhere in your code.

### Current features

 - Scoped cancellation using thread-local "cancellation triggers".
 - Out-of-the box support for triggers based on atomics and timers.
 - With feature `ctrlc` enabled, support for cancellation using `SIGINT` signals.
 - With feature `pyo3` enabled, support for cancellation using `Python::check_signals`.
 - With feature `liveness` enabled, you can register a per-thread handler which is invoked
   every time the thread becomes unresponsive (i.e. cancellation check has not been performed
   withing the prescribed interval).

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
    let result: Cancellable<()> = cancel_this::on_timeout(Duration::from_secs(1), || {
        cancellable_counter(5)?;
        cancellable_counter(10)?;
        cancellable_counter(100)?;
        Ok(())
    });
    
    assert!(result.is_err());   
}
```
