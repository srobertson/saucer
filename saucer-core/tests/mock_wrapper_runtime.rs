// Executes the wrapped app end-to-end to ensure codegen builds and runs.
#[test]
fn mock_wrapper_runtime_runs() {
    use mock_wrapper_app::build_runtime;
    use std::time::Duration;
    use tokio::runtime::Builder;

    let runtime = build_runtime();

    let rt = Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("tokio runtime");

    rt.block_on(async {
        tokio::time::timeout(Duration::from_secs(1), runtime.run())
            .await
            .expect("runtime should complete");
    });
}
