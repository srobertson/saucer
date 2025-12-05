use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::runtime::Builder;

#[test]
fn mock_port_app_runs_and_emits_counts() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let rt = Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("tokio runtime");

    let runtime = mock_port_app::build_runtime();
    let ports = runtime.ports();

    let seen = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&seen);
    ports
        .outbound_count
        .subscribe(move |count| captured.lock().unwrap().push(count));

    // Drive ports before running the loop
    ports.set_count.send(2);
    ports.increment_port.send();
    ports.increment_port.send();
    ports.increment_port.send();

    rt.block_on(async {
        tokio::time::timeout(Duration::from_secs(1), runtime.run())
            .await
            .expect("runtime should complete");
    });

    assert_eq!(&*seen.lock().unwrap(), &[2, 3, 4, 5]);
}
