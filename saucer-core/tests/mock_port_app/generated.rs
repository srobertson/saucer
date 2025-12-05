// Reference runtime for mock-port-app (hand-crafted ports prototype).
// This file is for human review only; it is NOT included or compiled.

use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;

use saucer_core::{CoreCmd, CoreManager, CoreRequest, Router, SendToManager};

type AppMsg = mock_port_app::app::Msg;

// === Internal Request enum (adds Ports alongside Core) ===

pub(crate) enum PortsRequest {
    OutboundCount { count: u32 },
}

pub(crate) enum Request<Msg> {
    Core(CoreRequest),
    Ports(PortsRequest, PhantomData<Msg>),
}

// === Cmd alias (map unused in this prototype) ===

pub type Cmd<Msg> = CoreCmd<Request<Msg>>;

// === Command helpers ===

pub fn shutdown<Msg>() -> Cmd<Msg> {
    CoreCmd::single(Request::Core(saucer_core::shutdown()))
}

pub fn outbound_count<Msg>(count: u32) -> Cmd<Msg> {
    CoreCmd::single(Request::Ports(PortsRequest::OutboundCount { count }, PhantomData))
}

// === Ports plumbing (sync, in-memory like the mock Router) ===

#[derive(Clone)]
pub struct InPort<T, E> {
    router: Rc<Router<E, ()>>,
    constructor: fn(T) -> E,
}

impl<T, E> InPort<T, E> {
    fn new(router: Rc<Router<E, ()>>, constructor: fn(T) -> E) -> Self {
        Self { router, constructor }
    }

    pub fn send(&self, payload: T) {
        let event = (self.constructor)(payload);
        self.router.send_to_app(event);
    }
}

#[derive(Clone, Default)]
pub struct OutPort<T> {
    subscribers: Rc<RefCell<Vec<Box<dyn Fn(T)>>>>,
}

impl<T: Clone + 'static> OutPort<T> {
    pub fn new() -> Self {
        Self {
            subscribers: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn subscribe<F>(&self, handler: F)
    where
        F: Fn(T) + 'static,
    {
        self.subscribers.borrow_mut().push(Box::new(handler));
    }

    pub fn dispatch(&self, payload: T) {
        for handler in self.subscribers.borrow().iter() {
            handler(payload.clone());
        }
    }
}

#[derive(Clone)]
pub struct Ports {
    pub increment_port: InPort<u32, AppMsg>,
    pub outbound_count: OutPort<u32>,
}

impl Ports {
    fn new(router: Rc<Router<AppMsg, ()>>) -> Self {
        Self {
            increment_port: InPort::new(router, MsgConstructor::increment),
            outbound_count: OutPort::new(),
        }
    }
}

// === Runtime ===

pub struct Runtime {
    router: Rc<Router<AppMsg, ()>>,
    ports: Ports,
}

impl Runtime {
    /// Static signature used by the generator; no-op here.
    pub fn run<Model, View, R>(
        _init: impl FnOnce() -> (Model, Cmd<AppMsg>),
        _update: impl Fn(Model, AppMsg) -> (Model, Cmd<AppMsg>),
        _view: impl Fn(&Model) -> View,
        _reconciler: R,
    ) where
        R: FnMut(&View, &SendToManager<CoreManager>),
    {
    }

    pub fn new() -> Self {
        let router = Rc::new(Router::new());
        let ports = Ports::new(router.clone());
        Self { router, ports }
    }

    pub fn ports(&self) -> Ports {
        self.ports.clone()
    }

    pub fn run_instance<Model, View, R>(
        self,
        init: impl FnOnce() -> (Model, Cmd<AppMsg>),
        update: impl Fn(Model, AppMsg) -> (Model, Cmd<AppMsg>),
        view: impl Fn(&Model) -> View,
        mut reconciler: R,
    ) where
        R: FnMut(&View, &SendToManager<CoreManager>),
    {
        let sender: SendToManager<CoreManager> = SendToManager::new(Rc::new(RefCell::new(Vec::new())));

        let (mut model, mut cmd) = init();
        let ports = self.ports.clone();
        let router = self.router.clone();

        loop {
            for req in cmd.into_inner() {
                match req {
                    Request::Core(CoreRequest::Shutdown) => return,
                    Request::Ports(PortsRequest::OutboundCount { count }, _) => {
                        ports.outbound_count.dispatch(count);
                    }
                }
            }

            let view_val = view(&model);
            reconciler(&view_val, &sender);

            let events = router.drain_app_events();
            if events.is_empty() {
                break;
            }

            let mut next_cmds = Vec::new();
            for event in events {
                let (new_model, new_cmd) = update(model, event);
                model = new_model;
                next_cmds.push(new_cmd);
            }

            cmd = CoreCmd::batch(next_cmds);
        }
    }
}

// === Template module (transformed) ===

pub mod mock_port_app {
    pub mod app {
        use super::super::{outbound_count, Cmd, shutdown};

        pub struct Model {
            pub count: u32,
        }

        #[derive(Clone, Debug)]
        pub enum Msg {
            Increment(u32),
        }

        pub fn init() -> (Model, Cmd<Msg>) {
            (Model { count: 0 }, Cmd::none())
        }

        pub fn update(mut model: Model, msg: Msg) -> (Model, Cmd<Msg>) {
            match msg {
                Msg::Increment(delta) => {
                    model.count += delta;
                    let count = model.count;
                    let send = outbound_count(count);
                    if count >= 5 {
                        return (model, Cmd::batch([send, shutdown()]));
                    }
                    (model, send)
                }
            }
        }

        pub fn view(_m: &Model) -> () {
            ()
        }
    }
}

// === Msg constructor used by incoming port ===

struct MsgConstructor;

impl MsgConstructor {
    fn increment(delta: u32) -> mock_port_app::app::Msg {
        mock_port_app::app::Msg::Increment(delta)
    }
}
