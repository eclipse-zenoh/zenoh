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
#[path = "common/mod.rs"]
mod common;

#[cfg(feature = "unstable")]
mod tests {
    use std::{
        sync::{atomic::AtomicUsize, Arc},
        time::Duration,
    };

    use crate::common::{
        close_session, open_session_connect, open_session_listen, open_session_unicast,
    };

    const SLEEP: Duration = Duration::from_millis(100);

    /// Test that transports() returns an iterator of Transport objects
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_info_transports() {
        zenoh_util::init_log_from_env_or("error");

        let (session1, session2) = open_session_unicast(&["tcp/127.0.0.1:17447"]).await;

        tokio::time::sleep(SLEEP).await;

        for transport in session1.info().transports().await {
            println!(
                "Transport from session1: zid={}, whatami={:?}",
                transport.zid(),
                transport.whatami()
            );
            assert_ne!(
                transport.zid().to_string(),
                "",
                "Transport ZID should not be empty"
            );
        }

        assert!(
            session2.info().transports().await.count() > 0,
            "Session2 should have at least one transport"
        );

        close_session(session1, session2).await;
    }

    /// Test that links() returns an iterator of Link objects
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_info_links() {
        zenoh_util::init_log_from_env_or("error");

        let (session1, session2) = open_session_unicast(&["tcp/127.0.0.1:17448"]).await;

        tokio::time::sleep(SLEEP).await;

        for link in session1.info().links().await {
            println!("Link from session1: {} -> {}", link.src(), link.dst());
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

        assert!(
            session2.info().links().await.count() > 0,
            "Session2 should have at least one link"
        );

        close_session(session1, session2).await;
    }

    /// Test that transport_events_listener() delivers events when transports open and close
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_transport_events() {
        zenoh_util::init_log_from_env_or("error");

        let session1 = open_session_listen(&["tcp/127.0.0.1:17450"]).await;

        // Subscribe to transport events with history
        let events = session1
            .info()
            .transport_events_listener()
            .history(true)
            .with(flume::bounded(32))
            .await;

        let session2 = open_session_connect(&["tcp/127.0.0.1:17450"]).await;

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

        let session1 = open_session_listen(&["tcp/127.0.0.1:17451"]).await;

        // Subscribe to link events with history
        let events = session1
            .info()
            .link_events_listener()
            .history(true)
            .with(flume::bounded(32))
            .await;

        let session2 = open_session_connect(&["tcp/127.0.0.1:17451"]).await;

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

        let (session1, session2) = open_session_unicast(&["tcp/127.0.0.1:17452"]).await;

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

        close_session(session1, session2).await;
    }

    /// Test that links() can be filtered by transport ZID
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_links_filter_by_transport() {
        zenoh_util::init_log_from_env_or("error");

        let session1 = open_session_listen(&["tcp/127.0.0.1:17458"]).await;
        let session2 = open_session_connect(&["tcp/127.0.0.1:17458"]).await;
        let session3 = open_session_connect(&["tcp/127.0.0.1:17458"]).await;

        // Wait for connections
        tokio::time::sleep(SLEEP).await;

        let transport_zids: Vec<_> = session1
            .info()
            .transports()
            .await
            .map(|t| *t.zid())
            .collect();
        assert_eq!(
            transport_zids.len(),
            2,
            "Should have 2 transports (one for each peer)"
        );

        assert_eq!(
            session1.info().links().await.count(),
            2,
            "Should have 2 links in total"
        );

        let link = session1
            .info()
            .links()
            .transport(transport_zids[0])
            .await
            .next()
            .unwrap();
        assert_eq!(
            link.zid(),
            &transport_zids[0],
            "Filtered link should belong to specified transport"
        );

        let link2 = session1
            .info()
            .links()
            .transport(transport_zids[1])
            .await
            .next()
            .unwrap();
        assert_eq!(
            link2.zid(),
            &transport_zids[1],
            "Filtered link should belong to specified transport"
        );

        println!("Successfully verified links() filtering by transport");

        close_session(session1, session2).await;
        session3.close().await.unwrap();
    }

    /// Test that linkl_events_listener() can be filtered by transport ZID
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_link_events_filter_by_transport() {
        zenoh_util::init_log_from_env_or("error");

        let session1 = open_session_listen(&["tcp/127.0.0.1:17459"]).await;
        let session2 = open_session_connect(&["tcp/127.0.0.1:17459"]).await;

        tokio::time::sleep(SLEEP).await;

        let target_zid = *session1.info().transports().await.next().unwrap().zid();

        // Track events received
        let events_received = Arc::new(AtomicUsize::new(0));
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
        let session3 = open_session_connect(&["tcp/127.0.0.1:17459"]).await;

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

        let _session2_new = open_session_connect(&["tcp/127.0.0.1:17459"]).await;

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

    /// Test that transport_events_listener() works in background mode
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_transport_events_background() {
        zenoh_util::init_log_from_env_or("error");

        let session1 = open_session_listen(&["tcp/127.0.0.1:17460"]).await;

        // Track events using atomic counters
        let opened_count = Arc::new(AtomicUsize::new(0));
        let closed_count = Arc::new(AtomicUsize::new(0));
        let opened_count_clone = opened_count.clone();
        let closed_count_clone = closed_count.clone();

        // Subscribe to transport events in background mode
        // Note: We don't keep a reference to the listener - it runs in background
        session1
            .info()
            .transport_events_listener()
            .history(false)
            .callback(move |event| {
                if event.is_open() {
                    opened_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    println!("Background: Transport opened: {}", event.transport().zid());
                } else if event.is_closed() {
                    closed_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    println!("Background: Transport closed");
                }
            })
            .background()
            .await
            .unwrap();

        let session2 = open_session_connect(&["tcp/127.0.0.1:17460"]).await;

        // Wait for connection to establish and event to be processed
        tokio::time::sleep(SLEEP * 2).await;

        // Should have received at least one transport opened event
        let opened = opened_count.load(std::sync::atomic::Ordering::SeqCst);
        assert!(
            opened > 0,
            "Should have received at least one transport opened event, got {}",
            opened
        );
        println!("Received {} transport opened events", opened);

        // Close session2 to trigger transport close event
        session2.close().await.unwrap();
        tokio::time::sleep(SLEEP * 2).await;

        // Should have received at least one transport closed event
        let closed = closed_count.load(std::sync::atomic::Ordering::SeqCst);
        assert!(
            closed > 0,
            "Should have received at least one transport closed event, got {}",
            closed
        );
        println!("Received {} transport closed events", closed);

        // Verify the background listener is still working by creating another connection
        let session3 = open_session_connect(&["tcp/127.0.0.1:17460"]).await;

        tokio::time::sleep(SLEEP * 2).await;

        // Should have received another opened event
        let opened_final = opened_count.load(std::sync::atomic::Ordering::SeqCst);
        assert!(
            opened_final > opened,
            "Should have received additional transport opened event after new connection"
        );
        println!(
            "Total transport opened events: {} (initial: {})",
            opened_final, opened
        );

        session1.close().await.unwrap();
        session3.close().await.unwrap();
    }
}
