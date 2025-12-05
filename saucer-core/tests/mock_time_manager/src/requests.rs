use saucer_time_manager::TimeRequest;
use std::sync::Arc;

pub fn notify_after<Msg: 'static>(
    id: u64,
    duration: std::time::Duration,
    returns: impl Fn() -> Msg + Send + Sync + 'static,
) -> TimeRequest<Msg> {
    TimeRequest::NotifyAfter {
        id,
        duration,
        returns: Arc::new(returns),
    }
}

pub fn notify_at<Msg: 'static>(
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

pub fn time_now<Msg: 'static>(
    returns: impl Fn(f64) -> Msg + Send + Sync + 'static,
) -> TimeRequest<Msg> {
    TimeRequest::Now {
        returns: Arc::new(returns),
    }
}

pub fn clear<Msg: 'static>(
    id: u64,
    returns: impl Fn() -> Msg + Send + Sync + 'static,
) -> TimeRequest<Msg> {
    TimeRequest::Clear {
        id,
        returns: Arc::new(returns),
    }
}
