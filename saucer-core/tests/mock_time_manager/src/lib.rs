pub mod requests;
pub use requests::{clear, notify_after, notify_at, time_now};
use saucer_core::Router;
pub use saucer_time_manager::TimeRequest;

// === Effect Manager ===

pub struct TimeManager;

impl TimeManager {
    pub fn init() -> () {
        ()
    }

    pub fn on_effects<Msg: Send + 'static>(
        &self,
        router: &Router<Msg, ()>,
        state: (),
        effects: Vec<TimeRequest<Msg>>,
    ) -> () {
        for req in effects {
            match req {
                TimeRequest::Now { returns } => {
                    router.send_to_app(returns(1234567890.0));
                }
                TimeRequest::NotifyAfter { returns, .. } => {
                    router.send_to_app(returns());
                }
                TimeRequest::NotifyAt { returns, .. } => {
                    router.send_to_app(returns());
                }
                TimeRequest::Clear { returns, .. } => {
                    router.send_to_app(returns());
                }
            }
        }
        state
    }
}

// Provide a passthrough std module so generated code importing `mock_time_manager::std` compiles.
pub mod std {
    pub use std::*;
}
