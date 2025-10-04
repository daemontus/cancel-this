use crate::{CancelNever, CancellationTrigger, DynamicCancellationTrigger};

/// Implementation of [`CancellationTrigger`] which chains together several
/// trigger implementations.
///
/// This is mostly used internally by [`crate::on_trigger`] to implement chaining of
/// multiple cancellation scopes. However, it is still a normal [`CancellationTrigger`] and
/// thus can be used to combine triggers manually as well.
#[derive(Clone, Default)]
pub struct CancelChain(Vec<DynamicCancellationTrigger>);

impl CancellationTrigger for CancelChain {
    fn is_cancelled(&self) -> bool {
        // Should not really matter, but start checking from the "innermost" condition.
        self.0.iter().rev().any(|t| t.is_cancelled())
    }

    fn type_name(&self) -> &'static str {
        self.0
            .iter()
            .rev()
            .find(|t| t.is_cancelled())
            .map(|it| it.type_name())
            .unwrap_or("CancelChain")
    }
}

impl CancelChain {
    /// Remove the first trigger in the chain.
    pub fn pop(&mut self) -> Option<DynamicCancellationTrigger> {
        self.0.pop()
    }

    /// Add a new cancellation trigger. The new chain starts with the given trigger
    /// and continues with the already present ones.
    pub fn push<T: CancellationTrigger + 'static>(&mut self, trigger: T) {
        self.0.push(Box::new(trigger));
    }

    /// Make a copy of this trigger chain, but if the chain is empty, or only has a single element,
    /// replace it with a simplified trigger which does not need vector traversal.
    pub fn clone_and_flatten(&self) -> DynamicCancellationTrigger {
        if self.0.is_empty() {
            Box::new(CancelNever)
        } else if self.0.len() == 1 {
            self.0[0].clone()
        } else {
            Box::new(self.clone())
        }
    }
}
