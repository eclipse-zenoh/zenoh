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
#![cfg(feature = "internal_config")]

use std::time::Duration;

use zenoh::{
    handlers::FifoChannelHandler,
    matching::{MatchingListener, MatchingStatus},
    sample::Locality,
    Result as ZResult, Session,
};
use zenoh_config::{ModeDependentValue, WhatAmI};
use zenoh_core::ztimeout;

const TIMEOUT: Duration = Duration::from_secs(60);
const RECV_TIMEOUT: Duration = Duration::from_secs(1);

async fn create_session_pair(locator: &str) -> (Session, Session) {
    let config1 = {
        let mut config = zenoh::Config::default();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config
            .listen
            .endpoints
            .set(vec![locator.parse().unwrap()])
            .unwrap();
        config
    };
    let mut config2 = zenoh::Config::default();
    config2.set_mode(Some(WhatAmI::Client)).unwrap();
    config2
        .connect
        .set_endpoints(ModeDependentValue::Unique(vec![locator.parse().unwrap()]))
        .unwrap();

    let session1 = ztimeout!(zenoh::open(config1)).unwrap();
    let session2 = ztimeout!(zenoh::open(config2)).unwrap();
    (session1, session2)
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

async fn zenoh_querier_matching_status_inner(querier_locality: Locality, same_session: bool) {
    println!("Querier origin :{querier_locality:?}, same session: {same_session}");
    zenoh_util::init_log_from_env_or("error");
    let key_expr = match querier_locality {
        Locality::SessionLocal => "zenoh_querier_matching_status_local_test",
        Locality::Remote => "zenoh_querier_matching_status_remote_test",
        Locality::Any => "zenoh_querier_matching_status_any_test",
    };

    let (session1, session2) = match same_session {
        false => create_session_pair("tcp/127.0.0.1:18002").await,
        true => {
            let s1 = ztimeout!(zenoh::open(zenoh::Config::default())).unwrap();
            let s2 = s1.clone();
            (s1, s2)
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
    // There is a bug that hats do not distinguish whether they register/unregister complete or incomplete queryable,
    // if there are more than one with the same keyexpr on the same face
    //let qbl2 = ztimeout!(session2
    //    .declare_queryable(format!("{key_expr}/*"))
    //    .complete(false))
    //.unwrap();

    let _qbl3 = ztimeout!(session2
        .declare_queryable(format!("{key_expr}/value3"))
        .complete(true))
    .unwrap();

    assert_eq!(
        get_matching_listener_status(&matching_listener1),
        locality_compatible.then_some(true)
    );
    assert_eq!(get_matching_listener_status(&matching_listener2), None);

    assert_eq!(
        ztimeout!(querier1.matching_status()).unwrap().matching(),
        locality_compatible
    );
    assert!(!ztimeout!(querier2.matching_status()).unwrap().matching());

    let qbl4 = ztimeout!(session2
        .declare_queryable(format!("{key_expr}/*"))
        .complete(true))
    .unwrap();
    assert_eq!(
        get_matching_listener_status(&matching_listener2),
        locality_compatible.then_some(true)
    );
    assert_eq!(
        ztimeout!(querier2.matching_status()).unwrap().matching(),
        locality_compatible
    );

    ztimeout!(qbl4.undeclare()).unwrap();

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

    ztimeout!(qbl1.undeclare()).unwrap();
    // ztimeout!(qbl2.undeclare()).unwrap();
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
    zenoh_util::init_log_from_env_or("error");
    let key_expr = match publisher_locality {
        Locality::SessionLocal => "zenoh_publisher_matching_status_local_test",
        Locality::Remote => "zenoh_publisher_matching_status_remote_test",
        Locality::Any => "zenoh_publisher_matching_status_any_test",
    };

    let (session1, session2) = match same_session {
        false => create_session_pair("tcp/127.0.0.1:18001").await,
        true => {
            let s1 = ztimeout!(zenoh::open(zenoh::Config::default())).unwrap();
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
async fn zenoh_querier_matching_status() -> ZResult<()> {
    zenoh_util::init_log_from_env_or("error");
    zenoh_querier_matching_status_inner(Locality::Any, true).await;
    zenoh_querier_matching_status_inner(Locality::Any, false).await;
    zenoh_querier_matching_status_inner(Locality::Remote, true).await;
    zenoh_querier_matching_status_inner(Locality::Remote, false).await;
    zenoh_querier_matching_status_inner(Locality::SessionLocal, true).await;
    zenoh_querier_matching_status_inner(Locality::SessionLocal, false).await;
    Ok(())
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

    let mut config = zenoh::Config::default();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    let session = ztimeout!(zenoh::open(config)).unwrap();

    let querier = std::sync::Arc::new(session
        .declare_querier("zenoh_matching_listener_drop_deadlock")
        .await
        .unwrap());
    let matching_listener = querier
        .matching_listener()
        .callback({
            let querier = querier.clone();
            move |_| println!("{:?}", querier.id())
        })
        .await
        .unwrap();

    drop(querier);
    drop(matching_listener); // Should not deadlock
}