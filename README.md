# `cancel_this` (Rust co-op cancellation)

This crate provides a user-friendly way to implement cooperative 
cancellation in Rust based on a wide range of criteria, including
*triggers*, *timers*, *OS signals* (Ctrl+C), or the *Python 
interpreter linked using PyO3*.

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

### Simple example

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
