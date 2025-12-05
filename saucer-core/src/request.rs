/// Core requests (runtime-level, no callbacks)
#[derive(Clone, Debug, serde::Serialize)]
pub enum CoreRequest {
    Shutdown,
}

pub fn shutdown() -> CoreRequest {
    CoreRequest::Shutdown
}
