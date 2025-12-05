use saucer_core::Cmd;

// Stateless widget that can emit a command
pub struct Model {}

#[derive(Clone, Debug)]
pub enum Msg {
    Triggered,
}

pub fn init() -> (Model, Cmd<Msg>) {
    (Model {}, Cmd::batch([]))
}

pub fn update(_model: Model, msg: Msg) -> (Model, Cmd<Msg>) {
    match msg {
        Msg::Triggered => (Model {}, Cmd::none()),
    }
}
