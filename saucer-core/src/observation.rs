use std::time::SystemTime;

/// Observation variants emitted by the sync runtime
///
/// Intentionally carries the manager self-message type directly so observers
/// can pattern-match without stringification. No trait bounds are imposed here;
/// helpers add whatever bounds they need.
pub enum Observation<EventType, CommandType, SelfMsgType> {
    Event {
        ts: SystemTime,
        data: EventType,
    },
    Effect {
        ts: SystemTime,
        data: CommandType,
    },
    ManagerMsg {
        ts: SystemTime,
        manager: &'static str,
        data: SelfMsgType,
    },
}
