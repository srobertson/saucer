use crate::Observation;
use std::fmt::Debug;
use std::sync::Arc;

/// Observer function type
pub type ObserverFn<EventType, CommandType, SelfMsgType> =
    Arc<dyn Fn(&Observation<EventType, CommandType, SelfMsgType>) + Send + Sync>;

/// No-op observer
pub fn no_op_observer<EventType, CommandType, SelfMsgType>(
) -> ObserverFn<EventType, CommandType, SelfMsgType>
where
    EventType: 'static,
    CommandType: 'static,
    SelfMsgType: 'static,
{
    Arc::new(|_observation: &Observation<EventType, CommandType, SelfMsgType>| {})
}

/// Tracing observer - logs to tracing crate
pub fn tracing_observer<EventType, CommandType, SelfMsgType>(
) -> ObserverFn<EventType, CommandType, SelfMsgType>
where
    EventType: Debug + 'static,
    CommandType: Debug + 'static,
    SelfMsgType: Debug + 'static,
{
    Arc::new(
        move |observation: &Observation<EventType, CommandType, SelfMsgType>| match observation {
            Observation::Event { data, .. } => {
                tracing::info!(target: "saucer-core::Msg", "Msg({:?})", data);
            }
            Observation::Effect { data, .. } => {
                tracing::debug!(target: "saucer-core::Cmd", "Cmd({:?})", data);
            }
            Observation::ManagerMsg { manager, data, .. } => {
                tracing::debug!(target: "saucer-core::SelfMsg", "SelfMsg({:?}, {:?}", manager, data);
            }
        },
    )
}

/// Filter observer - include/exclude types
pub fn filter_observer<EventType, CommandType, SelfMsgType>(
    wrapped: ObserverFn<EventType, CommandType, SelfMsgType>,
    include_events: bool,
    include_manager_msgs: bool,
    include_effects: bool,
) -> ObserverFn<EventType, CommandType, SelfMsgType>
where
    EventType: 'static,
    CommandType: 'static,
    SelfMsgType: 'static,
{
    Arc::new(
        move |observation: &Observation<EventType, CommandType, SelfMsgType>| {
            let should_pass = match observation {
                Observation::Event { .. } => include_events,
                Observation::ManagerMsg { .. } => include_manager_msgs,
                Observation::Effect { .. } => include_effects,
            };

            if should_pass {
                wrapped(observation);
            }
        },
    )
}

/// Filter observer with custom predicate
pub fn filter_with<EventType, CommandType, SelfMsgType, F>(
    wrapped: ObserverFn<EventType, CommandType, SelfMsgType>,
    predicate: F,
) -> ObserverFn<EventType, CommandType, SelfMsgType>
where
    EventType: 'static,
    CommandType: 'static,
    SelfMsgType: 'static,
    F: Fn(&Observation<EventType, CommandType, SelfMsgType>) -> bool + Send + Sync + 'static,
{
    Arc::new(
        move |observation: &Observation<EventType, CommandType, SelfMsgType>| {
            if predicate(observation) {
                wrapped(observation);
            }
        },
    )
}

/// Tee observer - call multiple observers
pub fn tee_observer<EventType, CommandType, SelfMsgType>(
    observers: Vec<ObserverFn<EventType, CommandType, SelfMsgType>>,
) -> ObserverFn<EventType, CommandType, SelfMsgType>
where
    EventType: 'static,
    CommandType: 'static,
    SelfMsgType: 'static,
{
    Arc::new(
        move |observation: &Observation<EventType, CommandType, SelfMsgType>| {
            for observer in &observers {
                observer(observation);
            }
        },
    )
}
