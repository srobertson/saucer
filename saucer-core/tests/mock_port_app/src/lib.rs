pub mod runtime {
    include!(concat!(env!("OUT_DIR"), "/runtime.rs"));
}

pub use runtime::mock_port_app::app;
use runtime::sync::Runtime;

/// Build a runtime instance for tests/hosts to drive.
pub fn build_runtime() -> Runtime<
    impl FnOnce() -> (app::Model, runtime::Cmd<app::Msg>),
    impl Fn(app::Model, app::Msg) -> (app::Model, runtime::Cmd<app::Msg>),
    impl Fn(&app::Model) -> (),
    impl FnMut(&(), &saucer_core::SendToManager<saucer_core::CoreManager, runtime::SelfMsg>),
    app::Model,
    (),
> {
    Runtime::new(
        app::init,
        app::update,
        app::view,
        saucer_core::no_op_reconciler(),
        saucer_core::tracing_observer(),
    )
}
