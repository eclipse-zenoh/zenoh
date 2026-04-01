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
    use test_case::test_matrix;
    use zenoh_config::{ModeDependentValue, WhatAmI, WhatAmIMatcher};
    use zenoh_link::EndPoint;

    use crate::{
        api::{config::Config, session::open},
        net::runtime::Runtime,
        Session,
    };

    // Helper to get the internal static runtime of a session (only present for non-plugin sessions)
    fn get_static_runtime(session: &Session) -> Option<&Runtime> {
        session.static_runtime()
    }

    async fn create_session(
        mode: WhatAmI,
        listen_endpoints: Vec<EndPoint>,
        connect_endpoints: Vec<EndPoint>,
        gossip: bool,
    ) -> Session {
        let mut config = Config::default();
        let _ = config.set_mode(Some(mode)).unwrap();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();

        config.connect.endpoints.set(connect_endpoints).unwrap();
        config.listen.endpoints.set(listen_endpoints).unwrap();

        config.scouting.gossip.set_enabled(Some(gossip)).unwrap();
        if gossip {
            config
                .scouting
                .gossip
                .set_autoconnect(Some(ModeDependentValue::Unique(WhatAmIMatcher::empty())))
                .unwrap();
        }

        open(config).await.unwrap()
    }

    // Helper to create a session with a specific mode and no network endpoints.
    async fn create_silent_session(mode: WhatAmI, gossip: bool) -> Session {
        create_session(mode, vec![], vec![], gossip).await
    }

    #[test_matrix(
        [WhatAmI::Peer],
        [true, false]
    )]
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn alive_before_close(mode: WhatAmI, gossip: bool) {
        let session = create_silent_session(mode, gossip).await;
        let runtime = get_static_runtime(&session).expect("runtime should be present");
        let weak = runtime.state_weak();
        assert!(weak.upgrade().is_some());
    }

    #[test_matrix(
        [WhatAmI::Peer],
        [true, false]
    )]
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn still_alive_after_close(mode: WhatAmI, gossip: bool) {
        let session = create_silent_session(mode, gossip).await;
        let runtime = get_static_runtime(&session).unwrap();
        let weak = runtime.state_weak();
        session.close().await.unwrap();
        // The runtime is still held by the session (since session is not dropped yet)
        assert!(weak.upgrade().is_some());
    }

    #[test_matrix(
        [WhatAmI::Peer],
        [true, false]
    )]
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn dead_after_drop(mode: WhatAmI, gossip: bool) {
        let session = create_silent_session(mode, gossip).await;
        let runtime = get_static_runtime(&session).unwrap();
        let weak = runtime.state_weak();
        session.close().await.unwrap(); // ensure tasks are cancelled
        drop(session); // drop the session, releasing the last strong reference to the runtime
        assert!(weak.upgrade().is_none());
    }

    #[test_matrix(
        [WhatAmI::Peer],
        [true, false]
    )]
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn multiple_sessions_independent(mode: WhatAmI, gossip: bool) {
        let session1 = create_silent_session(mode, gossip).await;
        let runtime1 = get_static_runtime(&session1).unwrap();
        let weak1 = runtime1.state_weak();

        let session2 = create_silent_session(mode, gossip).await;
        let runtime2 = get_static_runtime(&session2).unwrap();
        let weak2 = runtime2.state_weak();

        // Both weak references should be valid.
        assert!(weak1.upgrade().is_some());
        assert!(weak2.upgrade().is_some());

        // Close session1 and drop it.
        session1.close().await.unwrap();

        // Both weak references should STILL be valid.
        assert!(weak1.upgrade().is_some());
        assert!(weak2.upgrade().is_some());

        drop(session1);

        assert!(weak1.upgrade().is_none());
        assert!(weak2.upgrade().is_some());

        // Close session2 and drop it.
        session2.close().await.unwrap();

        assert!(weak1.upgrade().is_none());
        assert!(weak2.upgrade().is_some());

        drop(session2);

        assert!(weak1.upgrade().is_none());
        assert!(weak2.upgrade().is_none());
    }

    // Helper to create a client and a peer that are connected.
    async fn create_clique(num: usize, mode: WhatAmI, gossip: bool) -> Vec<Session> {
        let port_offset = calc_offset(mode, gossip);
        let port = 12450 + port_offset;
        let peer_endpoint = format!("tcp/127.0.0.1:{}", port);
        let main = create_session(mode, vec![peer_endpoint.parse().unwrap()], vec![], gossip).await;

        let mut result = vec![main];

        for _ in 0..num {
            let client =
                create_session(mode, vec![], vec![peer_endpoint.parse().unwrap()], gossip).await;
            result.push(client);
        }

        result
    }

    fn calc_offset(mode: WhatAmI, gossip: bool) -> u16 {
        let mode = match mode {
            WhatAmI::Router => 0,
            WhatAmI::Peer => 1,
            WhatAmI::Client => 2,
        };

        let gossip = match gossip {
            true => 0,
            false => 1,
        };

        mode + gossip * 3
    }

    #[test_matrix(
        [WhatAmI::Peer],
        [true, false]
    )]
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn dead_after_drop_many_times(mode: WhatAmI, gossip: bool) {
        let num_clients = 10;
        let sessions = create_clique(num_clients, mode, gossip).await;

        // All weak references should be alive initially.
        for session in &sessions {
            assert!(session.inner_weak().upgrade().is_some());
            let runtime = get_static_runtime(session).expect("runtime should be present");
            assert!(runtime.state_weak().upgrade().is_some());
        }

        // Close and drop sessions one by one.
        for session in sessions {
            let runtime_state = get_static_runtime(&session)
                .expect("runtime should be present")
                .state_weak();
            let session_inner = session.inner_weak();

            assert!(session_inner.upgrade().is_some());
            assert!(runtime_state.upgrade().is_some());

            session.close().await.unwrap();
            drop(session);

            assert!(session_inner.upgrade().is_none());
            assert!(runtime_state.upgrade().is_none());
        }
    }
}
