//! App module - template transformed by saucer_core build script.
//!
//! Fictional imports are rewritten by the generator:
//! - `saucer_core::Cmd` -> `super::Cmd`
//! - `mock_time_manager::command::notify_after` -> `super::notify_after`
//! - `mock_http_manager::command::get` -> `super::get`
//! - `saucer_core::command::shutdown` -> `super::shutdown`

use saucer_core::Cmd;
use saucer_core::command::shutdown;
use mock_time_manager::command::notify_after;
use mock_http_manager::command::get;

use chrono::{DateTime, Utc};

pub struct Model {
    pub req_time: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Msg {
    GotTime(String),
    GotApiResponse(String),
}

fn format_timestamp(ts: f64) -> String {
    DateTime::<Utc>::from_timestamp(ts as i64, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| format!("invalid timestamp: {}", ts))
}

pub fn init() -> (Model, Cmd<Msg>) {
    let model = Model { req_time: None };
    let cmd = notify_after(0, ::std::time::Duration::from_millis(0), || chrono::Utc::now().timestamp_millis() as f64)
        .map(format_timestamp)
        .map(Msg::GotTime);
    (model, cmd)
}

pub fn update(model: Model, msg: Msg) -> (Model, Cmd<Msg>) {
    match msg {
        Msg::GotTime(formatted) => {
            let model = Model { req_time: Some(formatted.clone()), ..model };
            let url = format!("https://api.example.com/data?ts={}", formatted);
            (model, get(url, |resp: Result<mock_http_manager::HttpResponse, mock_http_manager::HttpError>| {
                match resp {
                    Ok(r) => Msg::GotApiResponse(String::from_utf8_lossy(&r.body).to_string()),
                    Err(e) => Msg::GotApiResponse(format!("error: {}", e.message)),
                }
            }))
        }
        Msg::GotApiResponse(_response) => {
            (model, shutdown())
        }
    }
}

pub fn view(_model: &Model) -> () { () }
