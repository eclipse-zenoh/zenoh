//
// Copyright (c) 2023 ZettaScale Technology
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
#![cfg(feature = "unstable")]
mod common;

use std::time::Duration;

use zenoh::{
    handlers::FifoChannelHandler,
    matching::{MatchingListener, MatchingStatus},
    sample::Locality,
    Result as ZResult, Session,
};
use zenoh_config::WhatAmI;
use zenoh_core::ztimeout;

use crate::common::TestSessions;

const TIMEOUT: Duration = Duration::from_secs(60);
const RECV_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug, PartialEq, Clone, Copy)]
enum TestType {
    SameSession,
    ClientPeer,
    PeerClient,
    PeerPeer,
    ClientRouterClient,
    RouterRouter,
    RouterClient,
}

async fn create_direct_session_pair(modes: (WhatAmI, WhatAmI)) -> (Session, Session) {
    let mut test_context = TestSessions::new();

    match modes {
        (WhatAmI::Peer, WhatAmI::Peer)
        | (WhatAmI::Router, WhatAmI::Router)
        | (WhatAmI::Peer, WhatAmI::Client)
        | (WhatAmI::Router, WhatAmI::Client) => {
            let mut listener_config = test_context.get_listener_config("tcp/127.0.0.1:0", 1);
            listener_config.set_mode(Some(modes.0)).unwrap();
            let session1 = test_context.open_listener_with_cfg(listener_config).await;

            let mut connector_config = test_context.get_connector_config();
            connector_config.set_mode(Some(modes.1)).unwrap();
            // We need to clear the listen endpoints if both of them are routers. Router mode will bind tcp/[::]:7447 automatically.
            if modes.0 == WhatAmI::Router && modes.1 == WhatAmI::Router {
                connector_config.listen.endpoints.set(vec![]).unwrap();
            }
            let session2 = test_context.open_connector_with_cfg(connector_config).await;
            (session1, session2)
        }
        (WhatAmI::Client, WhatAmI::Peer) => {
            let mut listener_config = test_context.get_listener_config("tcp/127.0.0.1:0", 1);
            listener_config.set_mode(Some(modes.1)).unwrap();
            let session2 = test_context.open_listener_with_cfg(listener_config).await;

            let mut connector_config = test_context.get_connector_config();
            connector_config.set_mode(Some(modes.0)).unwrap();
            let session1 = test_context.open_connector_with_cfg(connector_config).await;
            (session1, session2)
        }
        _ => unreachable!(),
    }
}

async fn create_routed_client_pair() -> (Session, Session, Session) {
    let mut test_context = TestSessions::new();

    let mut router_config = test_context.get_listener_config("tcp/127.0.0.1:0", 1);
    router_config.set_mode(Some(WhatAmI::Router)).unwrap();
    let router = test_context.open_listener_with_cfg(router_config).await;

    let mut client1_config = test_context.get_connector_config();
    client1_config.set_mode(Some(WhatAmI::Client)).unwrap();
    let session1 = test_context.open_connector_with_cfg(client1_config).await;

    let mut client2_config = test_context.get_connector_config();
    client2_config.set_mode(Some(WhatAmI::Client)).unwrap();
    let session2 = test_context.open_connector_with_cfg(client2_config).await;

    (session1, session2, router)
}

fn get_matching_listener_status(
    matching_listener: &MatchingListener<FifoChannelHandler<MatchingStatus>>,
) -> Option<bool> {
    let received_status = matching_listener.recv_timeout(RECV_TIMEOUT);
    received_status.ok().flatten().map(|s| s.matching())
}

fn is_locality_compatible(locality: Locality, same_session: bool) -> bool {
    match locality {
        Locality::SessionLocal => same_session,
        Locality::Remote => !same_session,
        Locality::Any => true,
    }
}

#[test_case::test_matrix(
    [Locality::SessionLocal, Locality::Remote, Locality::Any],
    [
        TestType::SameSession,
        TestType::ClientPeer, TestType::PeerClient, TestType::PeerPeer,
        TestType::ClientRouterClient, TestType::RouterClient, TestType::RouterRouter
    ]
)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_querier_matching_status(querier_locality: Locality, test_type: TestType) {
    zenoh_util::init_log_from_env_or("error");
    println!("Querier origin :{querier_locality:?}, test type: {test_type:?}");
    let same_session = test_type == TestType::SameSession;
    let key_expr = match querier_locality {
        Locality::SessionLocal => "zenoh_querier_matching_status_local_test",
        Locality::Remote => "zenoh_querier_matching_status_remote_test",
        Locality::Any => "zenoh_querier_matching_status_any_test",
    };

    let mut _router;
    let (session1, session2) = match test_type {
        TestType::ClientPeer => create_direct_session_pair((WhatAmI::Client, WhatAmI::Peer)).await,
        TestType::PeerClient => create_direct_session_pair((WhatAmI::Peer, WhatAmI::Client)).await,
        TestType::PeerPeer => create_direct_session_pair((WhatAmI::Peer, WhatAmI::Peer)).await,
        TestType::SameSession => {
            let mut config = zenoh_config::Config::default();
            config.scouting.multicast.set_enabled(Some(false)).unwrap();
            let s1 = ztimeout!(zenoh::open(config)).unwrap();
            let s2 = s1.clone();
            (s1, s2)
        }
        TestType::ClientRouterClient => {
            let (s1, s2, router) = create_routed_client_pair().await;
            _router = Some(router);
            (s1, s2)
        }
        TestType::RouterRouter => {
            create_direct_session_pair((WhatAmI::Router, WhatAmI::Router)).await
        }
        TestType::RouterClient => {
            create_direct_session_pair((WhatAmI::Router, WhatAmI::Client)).await
        }
    };
    let locality_compatible = is_locality_compatible(querier_locality, same_session);

    let querier1 = ztimeout!(session1
        .declare_querier(format!("{key_expr}/value"))
        .target(zenoh::query::QueryTarget::BestMatching)
        .allowed_destination(querier_locality))
    .unwrap();

    let querier2 = ztimeout!(session1
        .declare_querier(format!("{key_expr}/*"))
        .target(zenoh::query::QueryTarget::AllComplete)
        .allowed_destination(querier_locality))
    .unwrap();

    let matching_listener1 = ztimeout!(querier1.matching_listener()).unwrap();
    let matching_listener2 = ztimeout!(querier2.matching_listener()).unwrap();

    assert!(!ztimeout!(querier1.matching_status()).unwrap().matching());
    assert!(!ztimeout!(querier2.matching_status()).unwrap().matching());

    assert_eq!(get_matching_listener_status(&matching_listener1), None);
    assert_eq!(get_matching_listener_status(&matching_listener2), None);

    let qbl1 = ztimeout!(session2
        .declare_queryable(format!("{key_expr}/value"))
        .complete(false))
    .unwrap();
    let qbl2 = ztimeout!(session2
        .declare_queryable(format!("{key_expr}/*"))
        .complete(false))
    .unwrap();

    let _qbl3 = ztimeout!(session2
        .declare_queryable(format!("{key_expr}/value3"))
        .complete(true))
    .unwrap();

    assert_eq!(
        get_matching_listener_status(&matching_listener1),
        locality_compatible.then_some(true)
    );
    assert_eq!(get_matching_listener_status(&matching_listener2), None);

    // NOTE: _qbl3 is complete, but its keyexpr doesn't include the AllComplete querier's
    assert_eq!(
        ztimeout!(querier1.matching_status()).unwrap().matching(),
        locality_compatible
    );
    assert!(!ztimeout!(querier2.matching_status()).unwrap().matching());

    let qbl4 = ztimeout!(session2
        .declare_queryable(format!("{key_expr}/*"))
        .complete(true))
    .unwrap();
    // NOTE: now there is a complete queryable whose keyexpr includes that of the AllComplete querier
    assert_eq!(
        get_matching_listener_status(&matching_listener2),
        locality_compatible.then_some(true)
    );
    assert_eq!(
        ztimeout!(querier2.matching_status()).unwrap().matching(),
        locality_compatible
    );

    ztimeout!(qbl4.undeclare()).unwrap();

    // NOTE: undeclaring qbl4 should not change the state of the BestMatching querier
    assert_eq!(get_matching_listener_status(&matching_listener1), None);
    assert_eq!(
        get_matching_listener_status(&matching_listener2),
        locality_compatible.then_some(false)
    );
    assert_eq!(
        ztimeout!(querier1.matching_status()).unwrap().matching(),
        locality_compatible
    );
    assert!(!ztimeout!(querier2.matching_status()).unwrap().matching());

    ztimeout!(qbl2.undeclare()).unwrap();
    assert_eq!(get_matching_listener_status(&matching_listener1), None);
    assert_eq!(get_matching_listener_status(&matching_listener2), None);
    assert_eq!(
        ztimeout!(querier1.matching_status()).unwrap().matching(),
        locality_compatible
    );
    assert!(!ztimeout!(querier2.matching_status()).unwrap().matching());

    ztimeout!(qbl1.undeclare()).unwrap();
    assert_eq!(
        get_matching_listener_status(&matching_listener1),
        locality_compatible.then_some(false)
    );
    assert_eq!(get_matching_listener_status(&matching_listener2), None);
    assert!(!ztimeout!(querier1.matching_status()).unwrap().matching());
    assert!(!ztimeout!(querier2.matching_status()).unwrap().matching());
}

async fn zenoh_publisher_matching_status_inner(publisher_locality: Locality, same_session: bool) {
    println!("Publisher origin: {publisher_locality:?}, same session: {same_session}");
    let key_expr = match publisher_locality {
        Locality::SessionLocal => "zenoh_publisher_matching_status_local_test",
        Locality::Remote => "zenoh_publisher_matching_status_remote_test",
        Locality::Any => "zenoh_publisher_matching_status_any_test",
    };

    let (session1, session2) = match same_session {
        false => create_direct_session_pair((WhatAmI::Peer, WhatAmI::Client)).await,
        true => {
            let mut config = zenoh_config::Config::default();
            config.scouting.multicast.set_enabled(Some(false)).unwrap();
            let s1 = ztimeout!(zenoh::open(config)).unwrap();
            let s2 = s1.clone();
            (s1, s2)
        }
    };
    let locality_compatible = is_locality_compatible(publisher_locality, same_session);

    let publisher = ztimeout!(session1
        .declare_publisher(format!("{key_expr}/*"))
        .allowed_destination(publisher_locality))
    .unwrap();

    let matching_listener = ztimeout!(publisher.matching_listener()).unwrap();

    assert_eq!(get_matching_listener_status(&matching_listener), None);

    let sub = ztimeout!(session2.declare_subscriber(format!("{key_expr}/value"))).unwrap();

    assert_eq!(
        get_matching_listener_status(&matching_listener),
        locality_compatible.then_some(true)
    );
    assert_eq!(
        ztimeout!(publisher.matching_status()).unwrap().matching(),
        locality_compatible
    );

    ztimeout!(sub.undeclare()).unwrap();

    assert_eq!(
        get_matching_listener_status(&matching_listener),
        locality_compatible.then_some(false)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_publisher_matching_status() -> ZResult<()> {
    zenoh_util::init_log_from_env_or("error");
    zenoh_publisher_matching_status_inner(Locality::Any, true).await;
    zenoh_publisher_matching_status_inner(Locality::Any, false).await;
    zenoh_publisher_matching_status_inner(Locality::Remote, true).await;
    zenoh_publisher_matching_status_inner(Locality::Remote, false).await;
    zenoh_publisher_matching_status_inner(Locality::SessionLocal, true).await;
    zenoh_publisher_matching_status_inner(Locality::SessionLocal, false).await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_matching_listener_drop_deadlock() {
    zenoh_util::init_log_from_env_or("error");

    let mut config = zenoh_config::Config::default();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    let session = ztimeout!(zenoh::open(config)).unwrap();

    let querier = std::sync::Arc::new(
        session
            .declare_querier("zenoh_matching_listener_drop_deadlock")
            .await
            .unwrap(),
    );
    let matching_listener = querier
        .matching_listener()
        .callback({
            let querier = querier.clone();
            move |_| println!("{}", querier.key_expr())
        })
        .await
        .unwrap();

    drop(querier);
    drop(matching_listener); // Should not deadlock
}

async fn zenoh_querier_matching_status_session_drop_inner(test_type: TestType) {
    println!("Querier session drop test type: {test_type:?}");
    let key_expr = "zenoh_querier_matching_status_session_drop_test";

    let mut router = None;
    let (session1, session2) = match test_type {
        TestType::ClientPeer => create_direct_session_pair((WhatAmI::Client, WhatAmI::Peer)).await,
        TestType::PeerClient => create_direct_session_pair((WhatAmI::Peer, WhatAmI::Client)).await,
        TestType::PeerPeer => create_direct_session_pair((WhatAmI::Peer, WhatAmI::Peer)).await,
        TestType::ClientRouterClient => {
            let (s1, s2, routed_router) = create_routed_client_pair().await;
            router = Some(routed_router);
            (s1, s2)
        }
        _ => unreachable!(),
    };

    let querier1 = ztimeout!(session1
        .declare_querier(format!("{key_expr}/value"))
        .target(zenoh::query::QueryTarget::BestMatching))
    .unwrap();

    let matching_listener1 = ztimeout!(querier1.matching_listener()).unwrap();

    assert!(!ztimeout!(querier1.matching_status()).unwrap().matching());

    assert_eq!(get_matching_listener_status(&matching_listener1), None);

    let _qbl1 = ztimeout!(session2
        .declare_queryable(format!("{key_expr}/value"))
        .complete(false))
    .unwrap();

    assert_eq!(
        get_matching_listener_status(&matching_listener1),
        Some(true)
    );
    assert!(ztimeout!(querier1.matching_status()).unwrap().matching());

    if let Some(r) = router.take() {
        ztimeout!(r.close()).unwrap();
    } else {
        ztimeout!(session2.close()).unwrap();
    }
    assert_eq!(
        get_matching_listener_status(&matching_listener1),
        Some(false)
    );
    assert!(!ztimeout!(querier1.matching_status()).unwrap().matching());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_querier_matching_status_session_drop() -> ZResult<()> {
    zenoh_util::init_log_from_env_or("error");
    let test_types = [
        TestType::ClientPeer,
        TestType::PeerClient,
        TestType::PeerPeer,
        TestType::ClientRouterClient,
    ];
    for tt in test_types {
        zenoh_querier_matching_status_session_drop_inner(tt).await;
    }
    Ok(())
}
