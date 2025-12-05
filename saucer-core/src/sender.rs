use std::marker::PhantomData;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

/// Trait for effect managers - defines associated SelfMsg type.
pub trait EffectManager {
    type SelfMsg;
}

/// Restricted sender that only allows sending messages to a specific manager.
/// This prevents reconcilers from accidentally sending app events directly.
pub struct SendToManager<Manager: EffectManager, WireMsg> {
    sender: UnboundedSender<WireMsg>,
    map: Arc<dyn Fn(Manager::SelfMsg) -> WireMsg + Send + Sync>,
    _phantom: PhantomData<Manager>,
}

impl<Manager: EffectManager, WireMsg: Send + 'static> SendToManager<Manager, WireMsg> {
    /// Create a new sender backed by an unbounded channel with a mapper that
    /// converts the manager's self-message into the shared wire type.
    pub fn new(
        sender: UnboundedSender<WireMsg>,
        map: impl Fn(Manager::SelfMsg) -> WireMsg + Send + Sync + 'static,
    ) -> Self {
        Self {
            sender,
            map: Arc::new(map),
            _phantom: PhantomData,
        }
    }

    /// Send a self-message to the manager
    pub fn send(&self, msg: Manager::SelfMsg) {
        let wire = (self.map)(msg);
        let _ = self.sender.send(wire);
    }
}
