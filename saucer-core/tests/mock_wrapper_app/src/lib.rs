// Library entrypoint to allow tests to depend on this crate.
pub mod runtime {
    include!(concat!(env!("OUT_DIR"), "/runtime.rs"));
}

// Make templates visible to codegen discovery.
#[allow(unused_imports)]
use runtime::mock_app::app;
#[allow(unused_imports)]
use runtime::mock_wrapper_app::wrapper;

type WrapperModel = runtime::mock_wrapper_app::wrapper::Model<runtime::mock_app::app::Model>;
type WrapperMsg = runtime::mock_wrapper_app::wrapper::Msg<runtime::mock_app::app::Msg>;

/// Build a runtime instance for the wrapped app. Used both for codegen
/// discovery and by the runtime smoke test.
pub fn build_runtime() -> runtime::sync::Runtime<
    impl FnOnce() -> (WrapperModel, runtime::Cmd<WrapperMsg>),
    impl Fn(WrapperModel, WrapperMsg) -> (WrapperModel, runtime::Cmd<WrapperMsg>),
    impl Fn(&WrapperModel) -> (),
    impl FnMut(&(), &saucer_core::SendToManager<saucer_core::CoreManager, runtime::SelfMsg>),
    WrapperModel,
    (),
    WrapperMsg,
> {
    use runtime::{mock_app::app, mock_wrapper_app::wrapper, sync::Runtime};

    let fns = wrapper::AppFns {
        init: app::init,
        update: app::update,
        view: app::view,
    };
    let wrapped = wrapper::make_app(fns);

    Runtime::new(
        move || (wrapped.init)(),
        move |m, msg| (wrapped.update)(m, msg),
        move |m| (wrapped.view)(m),
        saucer_core::no_op_reconciler(),
        saucer_core::tracing_observer(),
    )
}
