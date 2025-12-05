pub mod runtime {
    include!(concat!(env!("OUT_DIR"), "/runtime.rs"));
}

pub use runtime::mock_app::app;
use runtime::sync::Runtime;
use std::time::Duration;
use tokio::runtime::Builder;

pub fn run_app() {
    // Minimal logger to keep output quiet during tests
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

    rt.block_on(async {
        tokio::time::timeout(Duration::from_secs(1), runtime.run())
            .await
            .expect("runtime should complete before timeout")
    });
}
