//
// Copyright (c) 2026 ZettaScale Technology
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
#[cfg(all(feature = "unstable", feature = "internal", feature = "test"))]
mod runtime_state_weak_tests {
    use crate::api::session::open;
    use crate::net::runtime::Runtime;
    use crate::{api::config::Config, Session};
    use std::time::Duration;
    use tokio::time::sleep;
    use zenoh_config::{ModeDependentValue, WhatAmI, WhatAmIMatcher};
    use zenoh_link::EndPoint;

    // Helper to get the internal static runtime of a session (only present for non-plugin sessions)
    fn get_static_runtime(session: &Session) -> Option<&Runtime> {
        session.static_runtime()
    }

    async fn create_session(
        mode: WhatAmI,
        listen_endpoints: Vec<EndPoint>,
        connect_endpoints: Vec<EndPoint>,
    ) -> Session {
        let mut config = Config::default();
        config.connect.endpoints.set(connect_endpoints).unwrap();
        config.listen.endpoints.set(listen_endpoints).unwrap();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();

        config
            .routing
            .peer
            .set_mode(Some("linkstate".to_string()))
            .unwrap();
        config
            .scouting
            .gossip
            .set_autoconnect(Some(ModeDependentValue::Unique(WhatAmIMatcher::empty())))
            .unwrap();

        let _ = config.set_mode(Some(mode)).unwrap();
        open(config).await.unwrap()
    }

    // Helper to create a session with a specific mode and no network endpoints.
    async fn create_silent_session(mode: WhatAmI) -> Session {
        create_session(mode, vec![], vec![]).await
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_session_runtime_state_weak_alive_before_close() {
        let session = create_silent_session(WhatAmI::Client).await;
        let runtime = get_static_runtime(&session).expect("runtime should be present");
        let weak = runtime.state_weak();
        assert!(weak.upgrade().is_some());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_session_runtime_state_weak_still_alive_after_close() {
        let session = create_silent_session(WhatAmI::Client).await;
        let runtime = get_static_runtime(&session).unwrap();
        let weak = runtime.state_weak();
        session.close().await.unwrap();
        // The runtime is still held by the session (since session is not dropped yet)
        assert!(weak.upgrade().is_some());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_session_runtime_state_weak_dead_after_drop() {
        let session = create_silent_session(WhatAmI::Client).await;
        let runtime = get_static_runtime(&session).unwrap();
        let weak = runtime.state_weak();
        session.close().await.unwrap(); // ensure tasks are cancelled
        drop(session); // drop the session, releasing the last strong reference to the runtime
        assert!(weak.upgrade().is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_multiple_sessions_runtime_state_weak_independent() {
        let session1 = create_silent_session(WhatAmI::Client).await;
        let runtime1 = get_static_runtime(&session1).unwrap();
        let weak1 = runtime1.state_weak();

        let session2 = create_silent_session(WhatAmI::Client).await;
        let runtime2 = get_static_runtime(&session2).unwrap();
        let weak2 = runtime2.state_weak();

        // Both weak references should be valid.
        assert!(weak1.upgrade().is_some());
        assert!(weak2.upgrade().is_some());

        // Close session1 and drop it.
        session1.close().await.unwrap();
        drop(session1);
        assert!(weak1.upgrade().is_none());
        assert!(weak2.upgrade().is_some());

        // Close session2 and drop it.
        session2.close().await.unwrap();
        drop(session2);
        assert!(weak2.upgrade().is_none());
    }

    // Helper to create a client and a peer that are connected.
    async fn create_clique(num: usize) -> Vec<Session> {
        let port = 7448; // fixed port for simplicity; adjust if needed
        let peer_endpoint = format!("tcp/127.0.0.1:{}", port);
        let main =
            create_session(WhatAmI::Peer, vec![peer_endpoint.parse().unwrap()], vec![]).await;
        sleep(Duration::from_millis(100)).await;

        let mut result = vec![main];

        for _ in 0..num {
            let client =
                create_session(WhatAmI::Peer, vec![], vec![peer_endpoint.parse().unwrap()]).await;
            result.push(client);
        }

        sleep(Duration::from_millis(1000)).await;

        result
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_interconnected_sessions_runtime_state_weak() {
        let num_clients = 3;
        let sessions = create_clique(num_clients).await;

        // All weak references should be alive initially.
        for session in &sessions {
            let runtime = get_static_runtime(session).expect("runtime should be present");
            assert!(runtime.state_weak().upgrade().is_some());
        }

        // Close and drop sessions one by one.
        for session in sessions {
            let runtime_state = get_static_runtime(&session)
                .expect("runtime should be present")
                .state_weak();
            assert!(runtime_state.upgrade().is_some());

            session.close().await.unwrap();
            drop(session);

            assert!(runtime_state.upgrade().is_none());
        }
    }
}
