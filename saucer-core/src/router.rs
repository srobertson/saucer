use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

/// Channel-backed router used by capability managers to emit app events and
/// self-messages. Payloads crossing the thread boundary must be `Send + 'static`.
#[derive(Clone)]
pub struct Router<AppEvent, SelfMsg> {
    app_tx: UnboundedSender<AppEvent>,
    self_tx: UnboundedSender<SelfMsg>,
}

impl<AppEvent, SelfMsg> Router<AppEvent, SelfMsg> {
    /// Create a new router around existing channels.
    pub fn new(app_tx: UnboundedSender<AppEvent>, self_tx: UnboundedSender<SelfMsg>) -> Self {
        Self { app_tx, self_tx }
    }

    /// Send an event to the application event loop.
    pub fn send_to_app(&self, event: AppEvent)
    where
        AppEvent: Send + 'static,
    {
        let _ = self.app_tx.send(event);
    }

    /// Clone the underlying app channel sender (primarily for spawning tasks).
    pub fn app_sender(&self) -> UnboundedSender<AppEvent> {
        self.app_tx.clone()
    }

    /// Clone the underlying self-message channel sender.
    pub fn self_sender(&self) -> UnboundedSender<SelfMsg> {
        self.self_tx.clone()
    }

    /// Send a self-message back to the manager.
    pub fn send_to_self(&self, msg: SelfMsg)
    where
        SelfMsg: Send + 'static,
    {
        let _ = self.self_tx.send(msg);
    }
}

/// Paired channel endpoints owned by the runtime loop.
pub struct RouterChannels<AppEvent, SelfMsg> {
    pub router: Router<AppEvent, SelfMsg>,
    pub app_rx: UnboundedReceiver<AppEvent>,
    pub self_rx: UnboundedReceiver<SelfMsg>,
}

impl<AppEvent, SelfMsg> RouterChannels<AppEvent, SelfMsg> {
    /// Allocate an app/self channel pair and return the router plus receivers.
    pub fn new() -> Self {
        let (app_tx, app_rx) = unbounded_channel();
        let (self_tx, self_rx) = unbounded_channel();
        Self {
            router: Router::new(app_tx, self_tx),
            app_rx,
            self_rx,
        }
    }
}
