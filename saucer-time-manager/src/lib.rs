//! saucer-time-manager: time effect manager for saucer-core.
//!
//! Fictional imports expect helpers under `saucer_time_manager::command::*`.
//! The real helpers live in `requests.rs`; `command.rs` is an empty breadcrumb.

mod requests;

pub use requests::{clear, notify_after, notify_at, now, TimeRequest};

use saucer_core::Router;
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Tracks outstanding timer tasks so they can be cleared deterministically.
#[derive(Default)]
pub struct TimerState {
    tasks: HashMap<u64, tokio::task::JoinHandle<()>>,
}

/// Time effect manager using async timers.
pub struct TimeManager;

impl TimeManager {
    pub fn init() -> TimerState {
        TimerState::default()
    }

    pub fn on_effects<Msg: Send + 'static>(
        &self,
        router: &Router<Msg, ()>,
        mut state: TimerState,
        effects: Vec<TimeRequest<Msg>>,
    ) -> TimerState {
        for req in effects {
            match req {
                TimeRequest::Now { returns } => {
                    let ts = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_else(|_| Duration::from_secs(0))
                        .as_secs_f64();
                    router.send_to_app(returns(ts));
                }
                TimeRequest::NotifyAt {
                    id,
                    instant_secs,
                    returns,
                } => {
                    let app_sender = router.app_sender();
                    let returns = returns.clone();
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_else(|_| Duration::from_secs(0))
                        .as_secs();
                    let delay = if instant_secs > now {
                        Duration::from_secs(instant_secs - now)
                    } else {
                        Duration::from_secs(0)
                    };
                    let handle = tokio::spawn(async move {
                        tokio::time::sleep(delay).await;
                        let _ = app_sender.send(returns());
                    });
                    state.tasks.insert(id, handle);
                }
                TimeRequest::NotifyAfter {
                    id,
                    duration,
                    returns,
                } => {
                    let app_sender = router.app_sender();
                    let returns = returns.clone();
                    let handle = tokio::spawn(async move {
                        tokio::time::sleep(duration).await;
                        let _ = app_sender.send(returns());
                    });
                    state.tasks.insert(id, handle);
                }
                TimeRequest::Clear { id, returns } => {
                    if let Some(handle) = state.tasks.remove(&id) {
                        handle.abort();
                    }
                    router.send_to_app(returns());
                }
            }
        }
        state
    }
}
