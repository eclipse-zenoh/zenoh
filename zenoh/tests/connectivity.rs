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

    use zenoh::Config;

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

    /// Test that transport_events_listener() delivers events when transports open and close
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
            .transport_events_listener()
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

    /// Test that linkl_events_listener() delivers events when links are added and removed
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
            .link_events_listener()
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
        let transport_events_listener = session1
            .info()
            .transport_events_listener()
            .history(true)
            .with(flume::bounded(32))
            .await;

        // Should immediately receive event for existing transport
        let event = tokio::time::timeout(
            Duration::from_secs(5),
            transport_events_listener.recv_async(),
        )
        .await
        .expect("Timeout waiting for history transport event")
        .expect("Channel closed");

        assert!(event.is_open(), "History event should be Put (opened)");
        println!("History: Transport {}", event.transport().zid());

        // Subscribe to link events WITH history - should get existing link
        let linkl_events_listener = session1
            .info()
            .link_events_listener()
            .history(true)
            .with(flume::bounded(32))
            .await;

        // Should immediately receive event for existing link
        let event =
            tokio::time::timeout(Duration::from_secs(5), linkl_events_listener.recv_async())
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

    /// Test that links() can be filtered by transport ZID
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_links_filter_by_transport() {
        zenoh_util::init_log_from_env_or("error");

        // Create first peer with listener
        let mut config1 = Config::default();
        config1
            .listen
            .endpoints
            .set(vec!["tcp/127.0.0.1:17458".parse().unwrap()])
            .unwrap();
        config1.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session1 = zenoh::open(config1).await.unwrap();

        // Create two peers that connect to first
        let mut config2 = Config::default();
        config2
            .connect
            .endpoints
            .set(vec!["tcp/127.0.0.1:17458".parse().unwrap()])
            .unwrap();
        config2.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session2 = zenoh::open(config2).await.unwrap();

        let mut config3 = Config::default();
        config3
            .connect
            .endpoints
            .set(vec!["tcp/127.0.0.1:17458".parse().unwrap()])
            .unwrap();
        config3.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session3 = zenoh::open(config3).await.unwrap();

        // Wait for connections
        tokio::time::sleep(SLEEP).await;

        // Get all transports
        let transports = session1.info().transports().await;
        let transport_zids: Vec<_> = transports.map(|t| *t.zid()).collect();
        assert_eq!(
            transport_zids.len(),
            2,
            "Should have 2 transports (one for each peer)"
        );

        // Get all links (without filter)
        let all_links: Vec<_> = session1.info().links().await.collect();
        assert_eq!(all_links.len(), 2, "Should have 2 links in total");

        // Get links for first transport only
        let filtered_links: Vec<_> = session1
            .info()
            .links()
            .transport(transport_zids[0])
            .await
            .collect();
        assert_eq!(
            filtered_links.len(),
            1,
            "Should have 1 link for first transport"
        );
        assert_eq!(
            filtered_links[0].zid(),
            &transport_zids[0],
            "Filtered link should belong to specified transport"
        );

        // Get links for second transport only
        let filtered_links2: Vec<_> = session1
            .info()
            .links()
            .transport(transport_zids[1])
            .await
            .collect();
        assert_eq!(
            filtered_links2.len(),
            1,
            "Should have 1 link for second transport"
        );
        assert_eq!(
            filtered_links2[0].zid(),
            &transport_zids[1],
            "Filtered link should belong to specified transport"
        );

        println!("Successfully verified links() filtering by transport");

        session1.close().await.unwrap();
        session2.close().await.unwrap();
        session3.close().await.unwrap();
    }

    /// Test that linkl_events_listener() can be filtered by transport ZID
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_link_events_filter_by_transport() {
        zenoh_util::init_log_from_env_or("error");

        // Create first peer with listener
        let mut config1 = Config::default();
        config1
            .listen
            .endpoints
            .set(vec!["tcp/127.0.0.1:17459".parse().unwrap()])
            .unwrap();
        config1.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session1 = zenoh::open(config1).await.unwrap();

        // Create second peer first (we'll filter for this one)
        let mut config2 = Config::default();
        config2
            .connect
            .endpoints
            .set(vec!["tcp/127.0.0.1:17459".parse().unwrap()])
            .unwrap();
        config2.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session2 = zenoh::open(config2).await.unwrap();

        // Wait for connection
        tokio::time::sleep(SLEEP).await;

        // Get the transport ZID for session2's connection
        let transports: Vec<_> = session1.info().transports().await.collect();
        assert_eq!(transports.len(), 1, "Should have 1 transport");
        let target_zid = *transports[0].zid();

        // Track events received
        let events_received = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let events_received_clone = events_received.clone();

        // Subscribe to link events with filter for target_zid
        let _events = session1
            .info()
            .link_events_listener()
            .transport(target_zid)
            .history(false)
            .callback(move |_event| {
                events_received_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
            .await;

        // Create third peer that connects - should NOT trigger events (different transport)
        let mut config3 = Config::default();
        config3
            .connect
            .endpoints
            .set(vec!["tcp/127.0.0.1:17459".parse().unwrap()])
            .unwrap();
        config3.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session3 = zenoh::open(config3).await.unwrap();

        // Wait for potential events
        tokio::time::sleep(SLEEP).await;

        // Should NOT have received events (filtered out)
        let count = events_received.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            count, 0,
            "Should not receive events for different transport"
        );

        // Close and reconnect session2 - SHOULD trigger events
        session2.close().await.unwrap();
        tokio::time::sleep(SLEEP).await;

        let mut config2_new = Config::default();
        config2_new
            .connect
            .endpoints
            .set(vec!["tcp/127.0.0.1:17459".parse().unwrap()])
            .unwrap();
        config2_new
            .scouting
            .multicast
            .set_enabled(Some(false))
            .unwrap();
        let _session2_new = zenoh::open(config2_new).await.unwrap();

        // Wait for events (poll with timeout)
        let start = std::time::Instant::now();
        let mut final_count;
        loop {
            final_count = events_received.load(std::sync::atomic::Ordering::SeqCst);
            if final_count > 0 {
                break;
            }
            if start.elapsed() > Duration::from_secs(5) {
                panic!("Did not receive filtered link events within timeout");
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        println!(
            "Successfully verified linkl_events_listener() filtering by transport (received {} events)",
            final_count
        );

        session1.close().await.unwrap();
        session3.close().await.unwrap();
    }
}
