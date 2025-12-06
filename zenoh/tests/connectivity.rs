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

    const SLEEP: Duration = Duration::from_millis(100);

    /// Test that transports() returns an iterator of Transport objects
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_info_transports() {
        zenoh_util::init_log_from_env_or("error");

        // Create first peer
        let mut config1 = zenoh::Config::default();
        config1
            .listen
            .endpoints
            .set(vec!["tcp/127.0.0.1:17447".parse().unwrap()])
            .unwrap();
        config1.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session1 = zenoh::open(config1).await.unwrap();

        // Create second peer that connects to first
        let mut config2 = zenoh::Config::default();
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
        let mut config1 = zenoh::Config::default();
        config1
            .listen
            .endpoints
            .set(vec!["tcp/127.0.0.1:17448".parse().unwrap()])
            .unwrap();
        config1.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session1 = zenoh::open(config1).await.unwrap();

        // Create second peer that connects to first
        let mut config2 = zenoh::Config::default();
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
        let mut config1 = zenoh::Config::default();
        config1
            .listen
            .endpoints
            .set(vec!["tcp/127.0.0.1:17449".parse().unwrap()])
            .unwrap();
        config1.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session1 = zenoh::open(config1).await.unwrap();

        // Create second peer that connects to first
        let mut config2 = zenoh::Config::default();
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
        let mut config1 = zenoh::Config::default();
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
        let mut config2 = zenoh::Config::default();
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
        let mut config1 = zenoh::Config::default();
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
        let mut config2 = zenoh::Config::default();
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
        let mut config1 = zenoh::Config::default();
        config1
            .listen
            .endpoints
            .set(vec!["tcp/127.0.0.1:17452".parse().unwrap()])
            .unwrap();
        config1.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session1 = zenoh::open(config1).await.unwrap();

        // Create second peer that connects to first
        let mut config2 = zenoh::Config::default();
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
}
