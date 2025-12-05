use std::time::Duration;

use saucer_core::RouterChannels;
use saucer_time_manager::{clear, notify_after, TimeManager};
use tokio::runtime::Builder;

#[test]
fn notify_after_delivers_event() {
    let rt = Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime");

    rt.block_on(async {
        tokio::time::pause();
        let RouterChannels {
            router, mut app_rx, ..
        } = RouterChannels::<String, ()>::new();
        let manager = TimeManager;
        let state = TimeManager::init();
        let state = manager.on_effects(
            &router,
            state,
            vec![notify_after(1, Duration::from_secs(5), || {
                "done".to_string()
            })],
        );

        // Advance virtual time to trigger the timer
        tokio::time::advance(Duration::from_secs(5)).await;

        let delivered = app_rx.recv().await.expect("timer should deliver message");
        assert_eq!(delivered, "done");

        // drop state to avoid unused warning
        drop(state);
    });
}

#[test]
fn clear_cancels_pending_timer() {
    let rt = Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime");

    rt.block_on(async {
        tokio::time::pause();
        let RouterChannels {
            router, mut app_rx, ..
        } = RouterChannels::<&'static str, ()>::new();
        let manager = TimeManager;
        let state = TimeManager::init();
        let state = manager.on_effects(
            &router,
            state,
            vec![
                notify_after(1, Duration::from_secs(10), || "late"),
                clear(1, || "cleared"),
            ],
        );

        // Advance past the original timer duration; cleared timer should not fire.
        tokio::time::advance(Duration::from_secs(15)).await;

        // The clear acknowledgment should be present; the timer payload should not.
        let first = app_rx.recv().await.expect("expect clear ack");
        assert_eq!(first, "cleared");
        assert!(
            app_rx.try_recv().is_err(),
            "timer should have been canceled"
        );

        drop(state);
    });
}
