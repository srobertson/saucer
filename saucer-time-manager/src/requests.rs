use std::sync::Arc;
use std::time::Duration;

/// Time effect requests (mirrors vandura-time shape, but with callbacks).
#[derive(Clone)]
pub enum TimeRequest<Msg> {
    Now {
        returns: Arc<dyn Fn(f64) -> Msg + Send + Sync>,
    },
    NotifyAt {
        id: u64,
        // seconds since epoch
        instant_secs: u64,
        returns: Arc<dyn Fn() -> Msg + Send + Sync>,
    },
    NotifyAfter {
        id: u64,
        duration: Duration,
        returns: Arc<dyn Fn() -> Msg + Send + Sync>,
    },
    Clear {
        id: u64,
        returns: Arc<dyn Fn() -> Msg + Send + Sync>,
    },
}

impl<Msg: 'static> TimeRequest<Msg> {
    pub fn map<Msg2>(
        self,
        f: impl Fn(Msg) -> Msg2 + Send + Sync + Clone + 'static,
    ) -> TimeRequest<Msg2> {
        let f = Arc::new(f);
        match self {
            TimeRequest::Now { returns } => TimeRequest::Now {
                returns: Arc::new(move |ts| f(returns(ts))),
            },
            TimeRequest::NotifyAt {
                id,
                instant_secs,
                returns,
            } => TimeRequest::NotifyAt {
                id,
                instant_secs,
                returns: Arc::new(move || f(returns())),
            },
            TimeRequest::NotifyAfter {
                id,
                duration,
                returns,
            } => TimeRequest::NotifyAfter {
                id,
                duration,
                returns: Arc::new(move || f(returns())),
            },
            TimeRequest::Clear { id, returns } => TimeRequest::Clear {
                id,
                returns: Arc::new(move || f(returns())),
            },
        }
    }
}

/// Fictional helpers used by templates: `use saucer_time_manager::command::<helper>;`
pub fn now<Msg>(returns: impl Fn(f64) -> Msg + Send + Sync + 'static) -> TimeRequest<Msg> {
    TimeRequest::Now {
        returns: Arc::new(returns),
    }
}

pub fn notify_at<Msg>(
    id: u64,
    instant_secs: u64,
    returns: impl Fn() -> Msg + Send + Sync + 'static,
) -> TimeRequest<Msg> {
    TimeRequest::NotifyAt {
        id,
        instant_secs,
        returns: Arc::new(returns),
    }
}

pub fn notify_after<Msg>(
    id: u64,
    duration: Duration,
    returns: impl Fn() -> Msg + Send + Sync + 'static,
) -> TimeRequest<Msg> {
    TimeRequest::NotifyAfter {
        id,
        duration,
        returns: Arc::new(returns),
    }
}

pub fn clear<Msg>(id: u64, returns: impl Fn() -> Msg + Send + Sync + 'static) -> TimeRequest<Msg> {
    TimeRequest::Clear {
        id,
        returns: Arc::new(returns),
    }
}

impl<Msg> std::fmt::Debug for TimeRequest<Msg> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TimeRequest::Now { .. } => f.write_str("TimeRequest::Now"),
            TimeRequest::NotifyAt { .. } => f.write_str("TimeRequest::NotifyAt"),
            TimeRequest::NotifyAfter { .. } => f.write_str("TimeRequest::NotifyAfter"),
            TimeRequest::Clear { .. } => f.write_str("TimeRequest::Clear"),
        }
    }
}
