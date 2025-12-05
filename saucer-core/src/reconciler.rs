use crate::sender::{EffectManager, SendToManager};

/// Core manager - used for apps without view reconciliation.
/// Has SelfMsg = () since it doesn't process self-messages.
pub struct CoreManager;

impl EffectManager for CoreManager {
    type SelfMsg = ();
}

//TODO: no_op_reconcile doesn't do anything so sholud not use SendToManager
// codgen is suppose introspcet call sites and know thi

/// No-op reconciler for apps that don't need view reconciliation.
/// Returns a closure that does nothing when called.
pub fn no_op_reconciler<V>() -> impl FnMut(&V, &SendToManager<CoreManager, ()>) {
    |_view: &V, _sender: &SendToManager<CoreManager, ()>| {
        // No-op: nothing to reconcile
    }
}
