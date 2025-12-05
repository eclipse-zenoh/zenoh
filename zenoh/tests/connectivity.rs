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
                transport.zid, transport.whatami
            );

            // ZID should not be empty
            assert_ne!(
                transport.zid.to_string(),
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
            println!("Link from session1: {} -> {}", link.src, link.dst);

            // Source and destination should not be empty
            assert_ne!(
                link.src.to_string(),
                "",
                "Link source should not be empty"
            );
            assert_ne!(
                link.dst.to_string(),
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
            println!("Transport {}: zid={}", count, transport.zid);
            count += 1;
        }
        assert!(count > 0, "Should have iterated over at least one transport");

        // Test using for loop
        let mut link_count = 0;
        for link in session1.info().links().await {
            println!("Link {}: {} -> {}", link_count, link.src, link.dst);
            link_count += 1;
        }
        assert!(link_count > 0, "Should have iterated over at least one link");

        session1.close().await.unwrap();
        session2.close().await.unwrap();
    }
}
