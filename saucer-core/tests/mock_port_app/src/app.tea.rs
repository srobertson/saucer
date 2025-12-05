use saucer_core::{Cmd, Sub};
use saucer_core::command::shutdown;

pub struct Model { pub count: u32 }
#[derive(Clone, Debug)]
pub enum Msg { Increment, Set(u32) }

// Incoming ports (no payload and multi-arg coverage)
#[port]
pub fn increment_port() -> Sub <Msg> { 
    Msg::Increment 
}

#[port]
pub fn set_count(value: u32) -> Sub<Msg> { Msg::Set(value) }

// Outgoing port: app sends current count; host subscribes
#[port]
pub fn outbound_count(_count: u32) -> Cmd<Msg> { unreachable!() }

pub fn init() -> (Model, Cmd<Msg>) { (Model { count: 0 }, Cmd::none()) }

pub fn update(mut model: Model, msg: Msg) -> (Model, Cmd<Msg>) {
    match msg {
        Msg::Increment => {
            model.count += 1;
        }
        Msg::Set(v) => {
            model.count = v;
        }
    }

    let count = model.count;
    let send = outbound_count(count);
    if count >= 5 {
        (model, Cmd::batch([send, shutdown()]))
    } else {
        (model, send)
    }
}

pub fn view(_m: &Model) -> () { () }
