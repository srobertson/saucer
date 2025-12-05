pub mod runtime {
    include!(concat!(env!("OUT_DIR"), "/runtime.rs"));
}

pub use runtime::mock_port_http_app::app;
use runtime::sync::Runtime;
use std::time::Duration;
use tokio::runtime::Builder;

/// Run the app end-to-end (used by tests).
pub fn run_app() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let rt = Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("tokio runtime");

    let runtime = Runtime::new(
        app::init,
        app::update,
        app::view,
        saucer_core::no_op_reconciler(),
        saucer_core::tracing_observer(),
    );
    let ports = runtime.ports();
    ports.trigger.send();

    rt.block_on(async {
        tokio::time::timeout(Duration::from_secs(1), runtime.run())
            .await
            .expect("runtime should complete before timeout")
    });
}
