use saucer_core::{Cmd, Sub};
use saucer_core::command::shutdown;
use mock_http_manager::command::get;

pub struct Model {
    pub last: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Msg {
    Trigger,
    Got(String),
}

#[port]
pub fn trigger() -> Sub<Msg> {
    Msg::Trigger
}

#[port]
pub fn response_port(_value: String) -> Cmd<Msg> {
    unreachable!()
}

pub fn init() -> (Model, Cmd<Msg>) {
    (Model { last: None }, Cmd::none())
}

pub fn update(mut model: Model, msg: Msg) -> (Model, Cmd<Msg>) {
    match msg {
        Msg::Trigger => {
            let cmd = get("/ping", |resp| {
                let body = match resp {
                    Ok(r) => String::from_utf8_lossy(&r.body).to_string(),
                    Err(e) => format!("err: {}", e.message),
                };
                Msg::Got(body)
            });
            (model, cmd)
        }
        Msg::Got(body) => {
            model.last = Some(body.clone());
            let resp = response_port(body);
            let cmds = Cmd::batch([resp, shutdown()]);
            (model, cmds)
        }
    }
}

pub fn view(_m: &Model) -> () {
    ()
}
