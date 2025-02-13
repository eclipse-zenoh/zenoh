//
// Copyright (c) 2025 ZettaScale Technology
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
#![cfg(feature = "internal_config")]

use std::time::Duration;

use zenoh::{sample::Locality, Result as ZResult, Session};
use zenoh_config::{ModeDependentValue, WhatAmI};
use zenoh_core::ztimeout;
use zenoh_keyexpr::{keyexpr, OwnedKeyExpr};
use zenoh_macros::ke;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

async fn create_peer_client_pair(
    locator: &str,
    namespaces: &[Option<OwnedKeyExpr>; 2],
) -> (Session, Session) {
    let config1 = {
        let mut config = zenoh::Config::default();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config
            .listen
            .endpoints
            .set(vec![locator.parse().unwrap()])
            .unwrap();
        config.namespace = namespaces[0].clone();
        config
    };
    let mut config2 = zenoh::Config::default();
    config2.set_mode(Some(WhatAmI::Client)).unwrap();
    config2
        .connect
        .set_endpoints(ModeDependentValue::Unique(vec![locator.parse().unwrap()]))
        .unwrap();
    config2.namespace = namespaces[1].clone();

    let session1 = zenoh::open(config1).await.unwrap();
    let session2 = zenoh::open(config2).await.unwrap();
    (session1, session2)
}

async fn create_routed_clients_pair(
    locator: &str,
    namespaces: &[Option<OwnedKeyExpr>; 2],
) -> (Session, Session, Session) {
    let config_router = {
        let mut config = zenoh::Config::default();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config.set_mode(Some(WhatAmI::Router)).unwrap();
        config
            .listen
            .endpoints
            .set(vec![locator.parse().unwrap()])
            .unwrap();
        config.namespace = namespaces[0].clone();
        config
    };

    let mut config1 = zenoh::Config::default();
    config1.set_mode(Some(WhatAmI::Client)).unwrap();
    config1
        .connect
        .set_endpoints(ModeDependentValue::Unique(vec![locator.parse().unwrap()]))
        .unwrap();
    config1.namespace = namespaces[0].clone();

    let mut config2 = zenoh::Config::default();
    config2.set_mode(Some(WhatAmI::Client)).unwrap();
    config2
        .connect
        .set_endpoints(ModeDependentValue::Unique(vec![locator.parse().unwrap()]))
        .unwrap();
    config2.namespace = namespaces[1].clone();

    let router = zenoh::open(config_router).await.unwrap();
    let session1 = zenoh::open(config1).await.unwrap();
    let session2 = zenoh::open(config2).await.unwrap();
    (session1, session2, router)
}

async fn create_local_session(namespace: Option<OwnedKeyExpr>) -> (Session, Session) {
    let config1 = {
        let mut config = zenoh::Config::default();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config.namespace = namespace;
        config
    };

    let session1 = zenoh::open(config1).await.unwrap();
    let session2 = session1.clone();
    (session1, session2)
}

async fn zenoh_namespace_pub_sub_inner(
    session1: Session,
    session2: Session,
    ke1: &keyexpr,
    ke2: &keyexpr,
    locality: Locality,
) {
    println!(
        "zenoh_namespace_pub_sub: {ke1} -> {ke2}, locality: {:?}",
        locality
    );
    let publisher = session1
        .declare_publisher(ke1)
        .allowed_destination(locality)
        .await
        .unwrap();
    let listener = publisher.matching_listener().await.unwrap();
    let subscriber = session2
        .declare_subscriber(ke2)
        .allowed_origin(locality)
        .await
        .unwrap();

    tokio::time::sleep(SLEEP).await;

    assert_eq!(publisher.key_expr().as_keyexpr(), ke1);
    assert_eq!(subscriber.key_expr().as_keyexpr(), ke2);
    publisher.put("test_pub_put").await.unwrap();
    let res = subscriber.recv_async().await.unwrap();
    assert_eq!(res.key_expr().as_keyexpr(), ke2);

    ztimeout!(session1.put(ke1, "test_put")).unwrap();
    let res = subscriber.recv_async().await.unwrap();
    assert_eq!(res.key_expr().as_keyexpr(), ke2);

    assert!(ztimeout!(publisher.matching_status()).unwrap().matching());
    let res = listener.recv_async().await.unwrap();
    assert!(res.matching());

    subscriber.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(!ztimeout!(publisher.matching_status()).unwrap().matching());
    let res = listener.recv_async().await.unwrap();
    assert!(!res.matching());
}

async fn zenoh_namespace_queryable_get_inner(
    session1: Session,
    session2: Session,
    ke1: &keyexpr,
    ke2: &keyexpr,
    locality: Locality,
) {
    println!(
        "zenoh_namespace_queryable_get: {ke1} -> {ke2}, locality: {:?}",
        locality
    );

    let querier = session2
        .declare_querier(ke2)
        .allowed_destination(locality)
        .await
        .unwrap();
    let listener = querier.matching_listener().await.unwrap();
    let queryable = session1
        .declare_queryable(ke1)
        .allowed_origin(locality)
        .await
        .unwrap();

    tokio::time::sleep(SLEEP).await;

    assert_eq!(querier.key_expr().as_keyexpr(), ke2);

    let reply = session2.get(ke2).await.unwrap();

    let query = queryable.recv_async().await.unwrap();
    assert_eq!(query.key_expr().as_keyexpr(), ke1);
    query.reply(query.key_expr(), "reply").await.unwrap();
    drop(query);

    let res = reply.recv_async().await.unwrap();
    assert_eq!(res.result().unwrap().key_expr().as_keyexpr(), ke2);

    let reply = querier.get().await.unwrap();

    let query = queryable.recv_async().await.unwrap();
    assert_eq!(query.key_expr().as_keyexpr(), ke1);
    query.reply(query.key_expr(), "reply").await.unwrap();
    drop(query);

    let res = reply.recv_async().await.unwrap();
    assert_eq!(res.result().unwrap().key_expr().as_keyexpr(), ke2);

    assert!(ztimeout!(querier.matching_status()).unwrap().matching());
    let res = listener.recv_async().await.unwrap();
    assert!(res.matching());

    queryable.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(!ztimeout!(querier.matching_status()).unwrap().matching());
    let res = listener.recv_async().await.unwrap();
    assert!(!res.matching());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_namespace_pub_sub() -> ZResult<()> {
    zenoh_util::init_log_from_env_or("error");
    let (s1, s2) = ztimeout!(create_peer_client_pair(
        "tcp/127.0.0.1:19001",
        &[Some(ke!("ns1").to_owned()), None]
    ));
    ztimeout!(zenoh_namespace_pub_sub_inner(
        s1,
        s2,
        ke!("zenoh/pub"),
        ke!("ns1/zenoh/pub"),
        Locality::Any
    ));

    let (s1, s2) = ztimeout!(create_peer_client_pair(
        "tcp/127.0.0.1:19001",
        &[Some(ke!("ns2").to_owned()), Some(ke!("ns2").to_owned())]
    ));
    ztimeout!(zenoh_namespace_pub_sub_inner(
        s1,
        s2,
        ke!("zenoh/pub"),
        ke!("zenoh/pub"),
        Locality::Any
    ));

    let (s1, s2) = ztimeout!(create_peer_client_pair(
        "tcp/127.0.0.1:19001",
        &[None, Some(ke!("ns3").to_owned())]
    ));
    ztimeout!(zenoh_namespace_pub_sub_inner(
        s1,
        s2,
        ke!("ns3/zenoh/pub"),
        ke!("zenoh/pub"),
        Locality::Any
    ));

    let (s1, s2) = ztimeout!(create_local_session(Some(ke!("ns4").to_owned())));
    ztimeout!(zenoh_namespace_pub_sub_inner(
        s1,
        s2,
        ke!("zenoh/pub"),
        ke!("zenoh/pub"),
        Locality::Any
    ));

    let (s1, s2) = ztimeout!(create_local_session(Some(ke!("ns4").to_owned())));
    ztimeout!(zenoh_namespace_pub_sub_inner(
        s1,
        s2,
        ke!("zenoh/pub"),
        ke!("zenoh/pub"),
        Locality::SessionLocal
    ));

    let (s1, s2, _router) = ztimeout!(create_routed_clients_pair(
        "tcp/127.0.0.1:19003",
        &[Some(ke!("ns4").to_owned()), None]
    ));
    ztimeout!(zenoh_namespace_pub_sub_inner(
        s1,
        s2,
        ke!("zenoh/pub"),
        ke!("ns4/zenoh/pub"),
        Locality::Any
    ));

    let (s1, s2, _router) = ztimeout!(create_routed_clients_pair(
        "tcp/127.0.0.1:19005",
        &[Some(ke!("ns5").to_owned()), Some(ke!("ns5").to_owned())]
    ));
    ztimeout!(zenoh_namespace_pub_sub_inner(
        s1,
        s2,
        ke!("zenoh/pub"),
        ke!("zenoh/pub"),
        Locality::Any
    ));

    let (s1, s2, _router) = ztimeout!(create_routed_clients_pair(
        "tcp/127.0.0.1:19007",
        &[None, Some(ke!("ns6").to_owned())]
    ));
    ztimeout!(zenoh_namespace_pub_sub_inner(
        s1,
        s2,
        ke!("ns6/zenoh/pub"),
        ke!("zenoh/pub"),
        Locality::Any
    ));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_namespace_queryable_get() -> ZResult<()> {
    zenoh_util::init_log_from_env_or("error");
    let (s1, s2) = ztimeout!(create_peer_client_pair(
        "tcp/127.0.0.1:19002",
        &[Some(ke!("ns1").to_owned()), None]
    ));
    ztimeout!(zenoh_namespace_queryable_get_inner(
        s1,
        s2,
        ke!("zenoh/query"),
        ke!("ns1/zenoh/query"),
        Locality::Any
    ));

    let (s1, s2) = ztimeout!(create_peer_client_pair(
        "tcp/127.0.0.1:19002",
        &[Some(ke!("ns2").to_owned()), Some(ke!("ns2").to_owned())]
    ));
    ztimeout!(zenoh_namespace_queryable_get_inner(
        s1,
        s2,
        ke!("zenoh/query"),
        ke!("zenoh/query"),
        Locality::Any
    ));

    let (s1, s2) = ztimeout!(create_peer_client_pair(
        "tcp/127.0.0.1:19002",
        &[None, Some(ke!("ns3").to_owned())]
    ));
    ztimeout!(zenoh_namespace_queryable_get_inner(
        s1,
        s2,
        ke!("ns3/zenoh/query"),
        ke!("zenoh/query"),
        Locality::Any
    ));

    let (s1, s2) = ztimeout!(create_local_session(Some(ke!("ns4").to_owned())));
    ztimeout!(zenoh_namespace_queryable_get_inner(
        s1,
        s2,
        ke!("zenoh/query"),
        ke!("zenoh/query"),
        Locality::Any
    ));

    let (s1, s2) = ztimeout!(create_local_session(Some(ke!("ns4").to_owned())));
    ztimeout!(zenoh_namespace_queryable_get_inner(
        s1,
        s2,
        ke!("zenoh/query"),
        ke!("zenoh/query"),
        Locality::SessionLocal
    ));

    let (s1, s2, _router) = ztimeout!(create_routed_clients_pair(
        "tcp/127.0.0.1:19004",
        &[Some(ke!("ns4").to_owned()), None]
    ));
    ztimeout!(zenoh_namespace_queryable_get_inner(
        s1,
        s2,
        ke!("zenoh/query"),
        ke!("ns4/zenoh/query"),
        Locality::Any
    ));

    let (s1, s2, _router) = ztimeout!(create_routed_clients_pair(
        "tcp/127.0.0.1:19006",
        &[Some(ke!("ns5").to_owned()), Some(ke!("ns5").to_owned())]
    ));
    ztimeout!(zenoh_namespace_queryable_get_inner(
        s1,
        s2,
        ke!("zenoh/query"),
        ke!("zenoh/query"),
        Locality::Any
    ));

    let (s1, s2, _router) = ztimeout!(create_routed_clients_pair(
        "tcp/127.0.0.1:19008",
        &[None, Some(ke!("ns6").to_owned())]
    ));
    ztimeout!(zenoh_namespace_queryable_get_inner(
        s1,
        s2,
        ke!("ns6/zenoh/query"),
        ke!("zenoh/query"),
        Locality::Any
    ));
    Ok(())
}
