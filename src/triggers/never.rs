use crate::liveness::LivenessInterceptor;
use crate::{CancelChain, CancellationTrigger, Cancelled, TRIGGER};

/// Run the given `action` by overriding current cancellation criteria with [`CancelNever`],
/// meaning they do not apply and the action is never cancelled.
///
/// This method is only meaningful if you want to use the same code with and without cancellation.
/// Code that never needs to be cancelled can simply never check cancellation.
///
/// Any cancellation triggers that are registered *within* this context still do apply to their
/// respective scopes.
///
/// ```rust
/// # use std::time::Duration;
/// # use cancel_this::{is_cancelled, CancelAtomic, Cancelled};
///
/// fn cancellable_counter(count: usize) -> Result<(), Cancelled> {
///     for _ in 0..count {
///         is_cancelled!()?;
///         std::thread::sleep(Duration::from_millis(10));
///     }
///     Ok(())
/// }
///
/// let trigger = CancelAtomic::new();
/// let _ = cancel_this::on_atomic(trigger.clone(), || {
///     // The first call should be ok, because the trigger is not cancelled yet.
///     cancellable_counter(5).unwrap();
///     // Now we explicitly cancel the trigger.
///     trigger.cancel();
///     // However, if running in the `never` scope, the cancellation does not apply.
///     cancel_this::never(|| cancellable_counter(5)).unwrap();
///     // Any other action outside the `never` scope is still cancelled.
///     let result = cancellable_counter(5);
///     assert!(result.is_err());
///     result
/// });
/// ```
///
pub fn never<TResult, TError, TAction>(action: TAction) -> Result<TResult, TError>
where
    TAction: FnOnce() -> Result<TResult, TError>,
    TError: From<Cancelled>,
{
    let mut set_aside = LivenessInterceptor::<CancelChain>::default();
    TRIGGER.with_borrow_mut(|value| std::mem::swap(value, &mut set_aside));
    let result = crate::on_trigger(CancelNever, action);
    TRIGGER.with_borrow_mut(|value| std::mem::swap(value, &mut set_aside));
    result
}

/// Implementation of [`CancellationTrigger`] that is never cancelled.
///
/// See also [`crate::never`].
#[derive(Debug, Clone, Copy, Default)]
pub struct CancelNever;

impl CancellationTrigger for CancelNever {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn type_name(&self) -> &'static str {
        "CancelNever"
    }
}
