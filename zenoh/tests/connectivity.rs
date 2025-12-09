//
// Copyright (c) 2024 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//

#[cfg(feature = "unstable")]
mod tests {
    use std::time::Duration;

    use zenoh::{cancellation::CancellationToken, Config};

    const SLEEP: Duration = Duration::from_millis(100);

    /// Test that transports() returns an iterator of Transport objects
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_info_transports() {
        zenoh_util::init_log_from_env_or("error");

        // Create first peer
        let mut config1 = Config::default();
        config1
            .listen
            .endpoints
            .set(vec!["tcp/127.0.0.1:17447".parse().unwrap()])
            .unwrap();
        config1.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session1 = zenoh::open(config1).await.unwrap();

        // Create second peer that connects to first
        let mut config2 = Config::default();
        config2
            .connect
            .endpoints
            .set(vec!["tcp/127.0.0.1:17447".parse().unwrap()])
            .unwrap();
        config2.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session2 = zenoh::open(config2).await.unwrap();

        // Wait for connection to establish
        tokio::time::sleep(SLEEP).await;

        // Query transports from session1
        let transports: Vec<_> = session1.info().transports().await.collect();

        // Should have at least one transport (to session2)
        assert!(
            !transports.is_empty(),
            "Session1 should have at least one transport"
        );

        // Verify transport fields are populated
        for transport in &transports {
            println!(
                "Transport from session1: zid={}, whatami={:?}",
                transport.zid(),
                transport.whatami()
            );

            // ZID should not be empty
            assert_ne!(
                transport.zid().to_string(),
                "",
                "Transport ZID should not be empty"
            );
        }

        // Query transports from session2
        let transports2: Vec<_> = session2.info().transports().await.collect();
        assert!(
            !transports2.is_empty(),
            "Session2 should have at least one transport"
        );

        session1.close().await.unwrap();
        session2.close().await.unwrap();
    }

    /// Test that links() returns an iterator of Link objects
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_info_links() {
        zenoh_util::init_log_from_env_or("error");

        // Create first peer
        let mut config1 = Config::default();
        config1
            .listen
            .endpoints
            .set(vec!["tcp/127.0.0.1:17448".parse().unwrap()])
            .unwrap();
        config1.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session1 = zenoh::open(config1).await.unwrap();

        // Create second peer that connects to first
        let mut config2 = Config::default();
        config2
            .connect
            .endpoints
            .set(vec!["tcp/127.0.0.1:17448".parse().unwrap()])
            .unwrap();
        config2.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session2 = zenoh::open(config2).await.unwrap();

        // Wait for connection to establish
        tokio::time::sleep(SLEEP).await;

        // Query links from session1
        let links: Vec<_> = session1.info().links().await.collect();

        // Should have at least one link
        assert!(!links.is_empty(), "Session1 should have at least one link");

        // Verify link fields are populated
        for link in &links {
            println!("Link from session1: {} -> {}", link.src(), link.dst());

            // Source and destination should not be empty
            assert_ne!(
                link.src().to_string(),
                "",
                "Link source should not be empty"
            );
            assert_ne!(
                link.dst().to_string(),
                "",
                "Link destination should not be empty"
            );
        }

        // Query links from session2
        let links2: Vec<_> = session2.info().links().await.collect();
        assert!(!links2.is_empty(), "Session2 should have at least one link");

        session1.close().await.unwrap();
        session2.close().await.unwrap();
    }

    /// Test that the iterator pattern works correctly
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_info_iterator_pattern() {
        zenoh_util::init_log_from_env_or("error");

        // Create first peer
        let mut config1 = Config::default();
        config1
            .listen
            .endpoints
            .set(vec!["tcp/127.0.0.1:17449".parse().unwrap()])
            .unwrap();
        config1.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session1 = zenoh::open(config1).await.unwrap();

        // Create second peer that connects to first
        let mut config2 = Config::default();
        config2
            .connect
            .endpoints
            .set(vec!["tcp/127.0.0.1:17449".parse().unwrap()])
            .unwrap();
        config2.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session2 = zenoh::open(config2).await.unwrap();

        // Wait for connection to establish
        tokio::time::sleep(SLEEP).await;

        let mut count = 0;
        for transport in session1.info().transports().await {
            println!("Transport {}: zid={}", count, transport.zid());
            count += 1;
        }
        assert!(
            count > 0,
            "Should have iterated over at least one transport"
        );

        // Test using for loop
        let mut link_count = 0;
        for link in session1.info().links().await {
            println!("Link {}: {} -> {}", link_count, link.src(), link.dst());
            link_count += 1;
        }
        assert!(
            link_count > 0,
            "Should have iterated over at least one link"
        );

        session1.close().await.unwrap();
        session2.close().await.unwrap();
    }

    /// Test that transport_events() delivers events when transports open and close
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_transport_events() {
        zenoh_util::init_log_from_env_or("error");

        // Create first peer with listener
        let mut config1 = Config::default();
        config1
            .listen
            .endpoints
            .set(vec!["tcp/127.0.0.1:17450".parse().unwrap()])
            .unwrap();
        config1.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session1 = zenoh::open(config1).await.unwrap();

        // Subscribe to transport events with history
        let events = session1
            .info()
            .transport_events()
            .history(true)
            .with(flume::bounded(32))
            .await;

        // Create second peer that connects to first
        let mut config2 = Config::default();
        config2
            .connect
            .endpoints
            .set(vec!["tcp/127.0.0.1:17450".parse().unwrap()])
            .unwrap();
        config2.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session2 = zenoh::open(config2).await.unwrap();

        // Wait for connection to establish
        tokio::time::sleep(SLEEP).await;

        // Should receive transport opened event with SampleKind::Put
        let event = tokio::time::timeout(Duration::from_secs(5), events.recv_async())
            .await
            .expect("Timeout waiting for transport event")
            .expect("Channel closed");

        assert!(event.is_open(), "Event should be an 'open' event");
        assert_eq!(
            event.kind(),
            zenoh::sample::SampleKind::Put,
            "Event kind should be Put for opened transport"
        );
        println!("Transport opened: {}", event.transport().zid());

        // Close session2 to trigger transport close event
        session2.close().await.unwrap();
        tokio::time::sleep(SLEEP).await;

        // Should receive transport closed event with SampleKind::Delete
        let event = tokio::time::timeout(Duration::from_secs(5), events.recv_async())
            .await
            .expect("Timeout waiting for transport close event")
            .expect("Channel closed");

        assert!(event.is_closed(), "Event should be a 'closed' event");
        assert_eq!(
            event.kind(),
            zenoh::sample::SampleKind::Delete,
            "Event kind should be Delete for closed transport"
        );
        println!("Transport closed");

        session1.close().await.unwrap();
    }

    /// Test that link_events() delivers events when links are added and removed
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_link_events() {
        zenoh_util::init_log_from_env_or("error");

        // Create first peer with listener
        let mut config1 = Config::default();
        config1
            .listen
            .endpoints
            .set(vec!["tcp/127.0.0.1:17451".parse().unwrap()])
            .unwrap();
        config1.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session1 = zenoh::open(config1).await.unwrap();

        // Subscribe to link events with history
        let events = session1
            .info()
            .link_events()
            .history(true)
            .with(flume::bounded(32))
            .await;

        // Create second peer that connects to first
        let mut config2 = Config::default();
        config2
            .connect
            .endpoints
            .set(vec!["tcp/127.0.0.1:17451".parse().unwrap()])
            .unwrap();
        config2.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session2 = zenoh::open(config2).await.unwrap();

        // Wait for connection to establish
        tokio::time::sleep(SLEEP).await;

        // Should receive link added event with SampleKind::Put
        let event = tokio::time::timeout(Duration::from_secs(5), events.recv_async())
            .await
            .expect("Timeout waiting for link event")
            .expect("Channel closed");

        assert!(event.is_added(), "Event should be an 'added' event");
        assert_eq!(
            event.kind(),
            zenoh::sample::SampleKind::Put,
            "Event kind should be Put for added link"
        );
        println!(
            "Link added: {} -> {} (transport: {})",
            event.link().src(),
            event.link().dst(),
            event.transport_zid()
        );

        // Close session2 to trigger link removal event
        session2.close().await.unwrap();
        tokio::time::sleep(SLEEP).await;

        // Should receive link removed event with SampleKind::Delete
        let event = tokio::time::timeout(Duration::from_secs(5), events.recv_async())
            .await
            .expect("Timeout waiting for link removal event")
            .expect("Channel closed");

        assert!(event.is_removed(), "Event should be a 'removed' event");
        assert_eq!(
            event.kind(),
            zenoh::sample::SampleKind::Delete,
            "Event kind should be Delete for removed link"
        );
        println!("Link removed");

        session1.close().await.unwrap();
    }

    /// Test that event history works correctly - sends existing transports/links as Put events
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_event_history() {
        zenoh_util::init_log_from_env_or("error");

        // Create first peer with listener
        let mut config1 = Config::default();
        config1
            .listen
            .endpoints
            .set(vec!["tcp/127.0.0.1:17452".parse().unwrap()])
            .unwrap();
        config1.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session1 = zenoh::open(config1).await.unwrap();

        // Create second peer that connects to first
        let mut config2 = Config::default();
        config2
            .connect
            .endpoints
            .set(vec!["tcp/127.0.0.1:17452".parse().unwrap()])
            .unwrap();
        config2.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session2 = zenoh::open(config2).await.unwrap();

        // Wait for connection to establish
        tokio::time::sleep(SLEEP).await;

        // Subscribe to transport events WITH history - should get existing transport
        let transport_events = session1
            .info()
            .transport_events()
            .history(true)
            .with(flume::bounded(32))
            .await;

        // Should immediately receive event for existing transport
        let event = tokio::time::timeout(Duration::from_secs(5), transport_events.recv_async())
            .await
            .expect("Timeout waiting for history transport event")
            .expect("Channel closed");

        assert!(event.is_open(), "History event should be Put (opened)");
        println!("History: Transport {}", event.transport().zid());

        // Subscribe to link events WITH history - should get existing link
        let link_events = session1
            .info()
            .link_events()
            .history(true)
            .with(flume::bounded(32))
            .await;

        // Should immediately receive event for existing link
        let event = tokio::time::timeout(Duration::from_secs(5), link_events.recv_async())
            .await
            .expect("Timeout waiting for history link event")
            .expect("Channel closed");

        assert!(event.is_added(), "History event should be Put (added)");
        println!(
            "History: Link {} -> {}",
            event.link().src(),
            event.link().dst()
        );

        session1.close().await.unwrap();
        session2.close().await.unwrap();
    }

    /// Test that cancellation token stops transport events
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_transport_events_cancellation() {
        zenoh_util::init_log_from_env_or("error");

        // Create first peer with listener
        let mut config1 = Config::default();
        config1
            .listen
            .endpoints
            .set(vec!["tcp/127.0.0.1:17453".parse().unwrap()])
            .unwrap();
        config1.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session1 = zenoh::open(config1).await.unwrap();

        // Track events received
        let events_received = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let events_received_clone = events_received.clone();

        // Create cancellation token and subscribe with callback
        let ct = CancellationToken::default();
        let _subscriber = session1
            .info()
            .transport_events()
            .history(false)
            .callback(move |_event| {
                events_received_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
            .cancellation_token(ct.clone())
            .await;

        // Create second peer that connects to first
        let mut config2 = Config::default();
        config2
            .connect
            .endpoints
            .set(vec!["tcp/127.0.0.1:17453".parse().unwrap()])
            .unwrap();
        config2.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session2 = zenoh::open(config2).await.unwrap();

        // Wait for connection and events (poll with timeout to handle system load)
        let start = std::time::Instant::now();
        let mut count_before;
        loop {
            count_before = events_received.load(std::sync::atomic::Ordering::SeqCst);
            if count_before > 0 {
                break;
            }
            if start.elapsed() > Duration::from_secs(5) {
                panic!("Did not receive any transport events within timeout");
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        println!("Received {} events before cancellation", count_before);

        // Cancel the token
        ct.cancel().await.unwrap();
        println!("Cancellation token cancelled");

        // Close session2 and create session3
        session2.close().await.unwrap();
        tokio::time::sleep(SLEEP).await;

        let mut config3 = Config::default();
        config3
            .connect
            .endpoints
            .set(vec!["tcp/127.0.0.1:17453".parse().unwrap()])
            .unwrap();
        config3.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session3 = zenoh::open(config3).await.unwrap();

        // Wait for potential events
        tokio::time::sleep(SLEEP).await;

        // Should NOT have received any more events after cancellation
        let count_after = events_received.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            count_before, count_after,
            "Should not receive events after cancellation (before: {}, after: {})",
            count_before, count_after
        );

        println!("Successfully verified no events received after cancellation");

        session1.close().await.unwrap();
        session3.close().await.unwrap();
    }

    /// Test that cancellation token stops link events
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_link_events_cancellation() {
        zenoh_util::init_log_from_env_or("error");

        // Create first peer with listener
        let mut config1 = Config::default();
        config1
            .listen
            .endpoints
            .set(vec!["tcp/127.0.0.1:17454".parse().unwrap()])
            .unwrap();
        config1.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session1 = zenoh::open(config1).await.unwrap();

        // Track events received
        let events_received = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let events_received_clone = events_received.clone();

        // Create cancellation token and subscribe with callback
        let ct = CancellationToken::default();
        let _subscriber = session1
            .info()
            .link_events()
            .history(false)
            .callback(move |_event| {
                events_received_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
            .cancellation_token(ct.clone())
            .await;

        // Create second peer that connects to first
        let mut config2 = Config::default();
        config2
            .connect
            .endpoints
            .set(vec!["tcp/127.0.0.1:17454".parse().unwrap()])
            .unwrap();
        config2.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session2 = zenoh::open(config2).await.unwrap();

        // Wait for connection and events (poll with timeout to handle system load)
        let start = std::time::Instant::now();
        let mut count_before;
        loop {
            count_before = events_received.load(std::sync::atomic::Ordering::SeqCst);
            if count_before > 0 {
                break;
            }
            if start.elapsed() > Duration::from_secs(5) {
                panic!("Did not receive any link events within timeout");
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        println!("Received {} events before cancellation", count_before);

        // Cancel the token
        ct.cancel().await.unwrap();
        println!("Cancellation token cancelled");

        // Close session2 and create session3
        session2.close().await.unwrap();
        tokio::time::sleep(SLEEP).await;

        let mut config3 = Config::default();
        config3
            .connect
            .endpoints
            .set(vec!["tcp/127.0.0.1:17454".parse().unwrap()])
            .unwrap();
        config3.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session3 = zenoh::open(config3).await.unwrap();

        // Wait for potential events
        tokio::time::sleep(SLEEP).await;

        // Should NOT have received any more events after cancellation
        let count_after = events_received.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            count_before, count_after,
            "Should not receive events after cancellation (before: {}, after: {})",
            count_before, count_after
        );

        println!("Successfully verified no events received after cancellation");

        session1.close().await.unwrap();
        session3.close().await.unwrap();
    }

    /// Test that cancellation waits for in-flight callbacks to complete
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_transport_events_cancellation_synchronization() {
        zenoh_util::init_log_from_env_or("error");

        // Create first peer with listener
        let mut config1 = Config::default();
        config1
            .listen
            .endpoints
            .set(vec!["tcp/127.0.0.1:17455".parse().unwrap()])
            .unwrap();
        config1.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session1 = zenoh::open(config1).await.unwrap();

        // Create cancellation token
        let ct = CancellationToken::default();

        // Track callback execution
        let callback_started = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let callback_completed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let callback_started_clone = callback_started.clone();
        let callback_completed_clone = callback_completed.clone();

        // Subscribe with a callback that takes time to execute
        let _events = session1
            .info()
            .transport_events()
            .history(false)
            .callback(move |_event| {
                callback_started_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                // Simulate processing time - long enough to test synchronization (2 seconds)
                std::thread::sleep(Duration::from_millis(2000));
                callback_completed_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            })
            .cancellation_token(ct.clone())
            .await;

        // Create second peer to trigger event
        let mut config2 = Config::default();
        config2
            .connect
            .endpoints
            .set(vec!["tcp/127.0.0.1:17455".parse().unwrap()])
            .unwrap();
        config2.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session2 = zenoh::open(config2).await.unwrap();

        // Wait for callback to start (poll with timeout)
        let start = std::time::Instant::now();
        while !callback_started.load(std::sync::atomic::Ordering::SeqCst) {
            if start.elapsed() > Duration::from_secs(5) {
                panic!("Callback did not start within timeout");
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // Verify callback has started but not yet completed (still in progress)
        assert!(
            callback_started.load(std::sync::atomic::Ordering::SeqCst),
            "Callback should have started"
        );
        assert!(
            !callback_completed.load(std::sync::atomic::Ordering::SeqCst),
            "Callback should still be in progress before cancellation"
        );
        println!("Verified callback is in progress");

        // Cancel while callback is still executing
        ct.cancel().await.unwrap();

        // After cancel completes, callback should have completed
        assert!(
            callback_completed.load(std::sync::atomic::Ordering::SeqCst),
            "Callback should have completed before cancel returned"
        );

        println!("Successfully verified cancellation waits for callback completion");

        session1.close().await.unwrap();
        session2.close().await.unwrap();
    }

    /// Test that using an already-cancelled token prevents registration
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_transport_events_pre_cancelled_token() {
        zenoh_util::init_log_from_env_or("error");

        // Create first peer with listener
        let mut config1 = Config::default();
        config1
            .listen
            .endpoints
            .set(vec!["tcp/127.0.0.1:17456".parse().unwrap()])
            .unwrap();
        config1.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session1 = zenoh::open(config1).await.unwrap();

        // Track events received
        let events_received = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let events_received_clone = events_received.clone();

        // Create and immediately cancel the token
        let ct = CancellationToken::default();
        ct.cancel().await.unwrap();

        // Subscribe with already-cancelled token
        let _subscriber = session1
            .info()
            .transport_events()
            .history(false)
            .callback(move |_event| {
                events_received_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
            .cancellation_token(ct.clone())
            .await;

        // Create second peer to trigger event
        let mut config2 = Config::default();
        config2
            .connect
            .endpoints
            .set(vec!["tcp/127.0.0.1:17456".parse().unwrap()])
            .unwrap();
        config2.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session2 = zenoh::open(config2).await.unwrap();

        // Wait for connection
        tokio::time::sleep(SLEEP).await;

        // Should NOT receive any events because token was already cancelled
        let count = events_received.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            count, 0,
            "Should not receive events with pre-cancelled token (received: {})",
            count
        );

        println!("Successfully verified pre-cancelled token prevents event registration");

        session1.close().await.unwrap();
        session2.close().await.unwrap();
    }

    /// Test multiple subscribers with independent cancellation tokens
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_transport_events_multiple_subscribers_cancellation() {
        zenoh_util::init_log_from_env_or("error");

        // Create first peer with listener
        let mut config1 = Config::default();
        config1
            .listen
            .endpoints
            .set(vec!["tcp/127.0.0.1:17457".parse().unwrap()])
            .unwrap();
        config1.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session1 = zenoh::open(config1).await.unwrap();

        // Track events received by each subscriber
        let events1_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let events2_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let events1_count_clone = events1_count.clone();
        let events2_count_clone = events2_count.clone();

        // Create two cancellation tokens and subscribers
        let ct1 = CancellationToken::default();
        let _sub1 = session1
            .info()
            .transport_events()
            .history(false)
            .callback(move |_event| {
                events1_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
            .cancellation_token(ct1.clone())
            .await;

        let ct2 = CancellationToken::default();
        let _sub2 = session1
            .info()
            .transport_events()
            .history(false)
            .callback(move |_event| {
                events2_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
            .cancellation_token(ct2.clone())
            .await;

        // Create second peer to trigger event
        let mut config2 = Config::default();
        config2
            .connect
            .endpoints
            .set(vec!["tcp/127.0.0.1:17457".parse().unwrap()])
            .unwrap();
        config2.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session2 = zenoh::open(config2).await.unwrap();

        // Wait for connection and events (poll with timeout to handle system load)
        let start = std::time::Instant::now();
        loop {
            let count1 = events1_count.load(std::sync::atomic::Ordering::SeqCst);
            let count2 = events2_count.load(std::sync::atomic::Ordering::SeqCst);
            if count1 > 0 && count2 > 0 {
                break;
            }
            if start.elapsed() > Duration::from_secs(5) {
                panic!("Both subscribers did not receive events within timeout");
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let count1_before = events1_count.load(std::sync::atomic::Ordering::SeqCst);
        let count2_before = events2_count.load(std::sync::atomic::Ordering::SeqCst);
        println!(
            "Before cancellation: subscriber1={}, subscriber2={}",
            count1_before, count2_before
        );

        // Cancel only the first token
        ct1.cancel().await.unwrap();

        // Close and reconnect to trigger new events
        session2.close().await.unwrap();
        tokio::time::sleep(SLEEP).await;

        let mut config3 = Config::default();
        config3
            .connect
            .endpoints
            .set(vec!["tcp/127.0.0.1:17457".parse().unwrap()])
            .unwrap();
        config3.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session3 = zenoh::open(config3).await.unwrap();

        // Wait for new connection and potential events
        tokio::time::sleep(SLEEP).await;

        // Subscriber 1 should NOT have received new events
        let count1_after = events1_count.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            count1_before, count1_after,
            "Subscriber 1 should not receive events after cancellation"
        );

        // Subscriber 2 SHOULD have received new events
        let count2_after = events2_count.load(std::sync::atomic::Ordering::SeqCst);
        assert!(
            count2_after > count2_before,
            "Subscriber 2 should still receive events (before: {}, after: {})",
            count2_before,
            count2_after
        );

        println!(
            "After cancellation: subscriber1={}, subscriber2={}",
            count1_after, count2_after
        );
        println!("Successfully verified independent cancellation of multiple subscribers");

        session1.close().await.unwrap();
        session3.close().await.unwrap();
    }
}
