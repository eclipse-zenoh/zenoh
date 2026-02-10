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

#![cfg(feature = "unstable")]
#[allow(dead_code)]
#[path = "common/mod.rs"]
mod common;
use core::time::Duration;
use std::sync::{atomic::AtomicBool, Arc};

use zenoh::{
    matching::MatchingStatus,
    query::{Query, Reply},
    sample::Sample,
    session::{LinkEvent, TransportEvent},
    Session, Wait,
};
use zenoh_core::ztimeout;

use crate::common::{open_session_connect, open_session_listen};

const TIMEOUT: Duration = Duration::from_secs(60);

async fn create_peer_pair(locator: &str) -> (Session, Session) {
    (
        open_session_listen(&[locator]).await,
        open_session_connect(&[locator]).await,
    )
}

async fn create_peer() -> Session {
    let mut config = zenoh::Config::default();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    zenoh::open(config).await.unwrap()
}

struct TestCallback {
    n: Arc<AtomicBool>,
}

impl Drop for TestCallback {
    fn drop(&mut self) {
        std::thread::sleep(Duration::from_secs(3));
        self.n.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

impl TestCallback {
    fn call<T>(&self, _: T) {
        std::thread::sleep(Duration::from_secs(3));
    }
}

fn create_callback<T>() -> (impl Fn(T) + Send + Sync + 'static, Arc<AtomicBool>) {
    let n = Arc::new(AtomicBool::new(false));
    let tb = TestCallback { n: n.clone() };
    let f = move |t| tb.call::<T>(t);
    (f, n)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_callback_drop_on_undeclare_subscriber() {
    zenoh::init_log_from_env_or("error");
    let ke = "test/undeclare/subscriber_callback_drop";
    let (session1, session2) = ztimeout!(create_peer_pair("tcp/127.0.0.1:20001"));
    let (cb, n) = create_callback::<Sample>();
    let subscriber = ztimeout!(session1.declare_subscriber(ke).callback(cb)).unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;
    let publisher = ztimeout!(session2.declare_publisher(ke)).unwrap();
    ztimeout!(publisher.put("payload")).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;

    ztimeout!(subscriber.undeclare().wait_callbacks()).unwrap();
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));

    // check background with session.close()
    let ke = "test/undeclare_background/subscriber_callback_drop";
    let (cb, n) = create_callback::<Sample>();
    ztimeout!(session1.declare_subscriber(ke).callback(cb).background()).unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;
    let publisher = ztimeout!(session2.declare_publisher(ke)).unwrap();
    ztimeout!(publisher.put("payload")).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;

    ztimeout!(session1.close().wait_callbacks()).unwrap();
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_callback_drop_on_undeclare_subscriber_local() {
    fn put_from_another_thread(s: &Session, ke: String) {
        std::thread::spawn({
            let s = s.clone();
            move || {
                s.put(ke, "payload").wait().unwrap();
            }
        });
    }

    zenoh::init_log_from_env_or("error");
    let ke = "test/undeclare/subscriber_callback_drop_local";
    let session = ztimeout!(create_peer());
    let (cb, n) = create_callback::<Sample>();
    let subscriber = ztimeout!(session.declare_subscriber(ke).callback(cb)).unwrap();
    put_from_another_thread(&session, ke.to_string());
    tokio::time::sleep(Duration::from_secs(1)).await;
    ztimeout!(subscriber.undeclare().wait_callbacks()).unwrap();
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));

    // check background with session.close()
    let ke = "test/undeclare_background/subscriber_callback_drop_local";
    let (cb, n) = create_callback::<Sample>();
    ztimeout!(session.declare_subscriber(ke).callback(cb).background()).unwrap();

    put_from_another_thread(&session, ke.to_string());
    tokio::time::sleep(Duration::from_secs(1)).await;

    ztimeout!(session.close().wait_callbacks()).unwrap();
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_callback_drop_on_undeclare_queryable() {
    zenoh::init_log_from_env_or("error");
    let ke = "test/undeclare/queryable_callback_drop";
    let (session1, session2) = ztimeout!(create_peer_pair("tcp/127.0.0.1:20002"));
    let (cb, n) = create_callback::<Query>();
    let queryable = ztimeout!(session1.declare_queryable(ke).callback(cb)).unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;
    let _replies = ztimeout!(session2.get(ke)).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;

    ztimeout!(queryable.undeclare().wait_callbacks()).unwrap();
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));

    // check background with session.close()
    let ke = "test/undeclare_background/queryable_callback_drop";
    let (cb, n) = create_callback::<Query>();
    ztimeout!(session1.declare_queryable(ke).callback(cb).background()).unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;
    let _replies = ztimeout!(session2.get(ke)).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;

    ztimeout!(session1.close().wait_callbacks()).unwrap();
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_callback_drop_on_undeclare_queryable_local() {
    fn get_from_another_thread(s: &Session, ke: String) {
        std::thread::spawn({
            let s = s.clone();
            move || {
                s.get(ke).wait().unwrap();
            }
        });
    }

    zenoh::init_log_from_env_or("error");
    let ke = "test/undeclare/queryable_callback_drop_local";
    let session = ztimeout!(create_peer());
    let (cb, n) = create_callback::<Query>();
    let queryable = ztimeout!(session.declare_queryable(ke).callback(cb)).unwrap();
    get_from_another_thread(&session, ke.to_string());
    tokio::time::sleep(Duration::from_secs(1)).await;
    ztimeout!(queryable.undeclare().wait_callbacks()).unwrap();
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));

    // check background with session.close()
    let ke = "test/undeclare_background/queryable_callback_drop_local";
    let (cb, n) = create_callback::<Query>();
    ztimeout!(session.declare_queryable(ke).callback(cb).background()).unwrap();

    get_from_another_thread(&session, ke.to_string());
    tokio::time::sleep(Duration::from_secs(1)).await;

    ztimeout!(session.close().wait_callbacks()).unwrap();
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_callback_drop_on_undeclare_liveliness_subscriber() {
    zenoh::init_log_from_env_or("error");
    let ke = "test/undeclare/liveliness_subscriber_callback_drop";
    let (session1, session2) = ztimeout!(create_peer_pair("tcp/127.0.0.1:20003"));
    let (cb, n) = create_callback::<Sample>();
    let subscriber = ztimeout!(session1.liveliness().declare_subscriber(ke).callback(cb)).unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;
    let token = ztimeout!(session2.liveliness().declare_token(ke)).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;

    ztimeout!(subscriber.undeclare().wait_callbacks()).unwrap();
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));
    drop(token);
    tokio::time::sleep(Duration::from_secs(1)).await;

    // check background with session.close()
    let ke = "test/undeclare_background/liveliness_subscriber_callback_drop";
    let (cb, n) = create_callback::<Sample>();
    ztimeout!(session1
        .liveliness()
        .declare_subscriber(ke)
        .callback(cb)
        .background())
    .unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;
    let _token = ztimeout!(session2.liveliness().declare_token(ke)).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;

    ztimeout!(session1.close().wait_callbacks()).unwrap();
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_callback_drop_on_undeclare_querier() {
    zenoh::init_log_from_env_or("error");
    let ke = "test/undeclare/querier_callback_drop";
    let (session1, session2) = ztimeout!(create_peer_pair("tcp/127.0.0.1:20004"));

    let querier = ztimeout!(session1.declare_querier(ke)).unwrap();
    let queryable = ztimeout!(session2.declare_queryable(ke)).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    let (cb, n) = create_callback::<Reply>();
    ztimeout!(querier.get().callback(cb)).unwrap();
    let query = ztimeout!(queryable.recv_async()).unwrap();
    ztimeout!(query.reply(ke, "payload")).unwrap();
    drop(query);
    tokio::time::sleep(Duration::from_secs(1)).await;
    ztimeout!(querier.undeclare().wait_callbacks()).unwrap();
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));
    drop(queryable);

    tokio::time::sleep(Duration::from_secs(1)).await;

    // check matching listener
    let querier = ztimeout!(session1.declare_querier(ke)).unwrap();
    let (cb, n) = create_callback::<MatchingStatus>();
    let listener = ztimeout!(querier.matching_listener().callback(cb)).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    let _queryable = ztimeout!(session2.declare_queryable(ke)).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    ztimeout!(listener.undeclare().wait_callbacks()).unwrap();
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));
    drop(_queryable);

    tokio::time::sleep(Duration::from_secs(1)).await;

    // check background matching listener
    let (cb, n) = create_callback::<MatchingStatus>();
    ztimeout!(querier.matching_listener().callback(cb).background()).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    let _queryable = ztimeout!(session2.declare_queryable(ke)).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    ztimeout!(querier.undeclare().wait_callbacks()).unwrap();

    assert!(n.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_callback_drop_on_undeclare_querier_local() {
    fn reply_from_another_thread(q: Query, ke: String) {
        std::thread::spawn({
            move || {
                q.reply(ke, "payload").wait().unwrap();
            }
        });
    }

    fn declare_queryable_in_another_thread(s: &Session, ke: String) {
        std::thread::spawn({
            let s = s.clone();
            move || {
                let _queryable = s.declare_queryable(ke).wait().unwrap();
                std::thread::sleep(Duration::from_secs(1));
            }
        });
    }

    zenoh::init_log_from_env_or("error");
    let ke = "test/undeclare/querier_callback_drop_local";
    let session = ztimeout!(create_peer());

    let querier = ztimeout!(session.declare_querier(ke)).unwrap();
    let queryable = ztimeout!(session.declare_queryable(ke)).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    let (cb, n) = create_callback::<Reply>();
    ztimeout!(querier.get().callback(cb)).unwrap();
    let query = ztimeout!(queryable.recv_async()).unwrap();
    reply_from_another_thread(query, ke.to_string());
    tokio::time::sleep(Duration::from_secs(1)).await;
    ztimeout!(querier.undeclare().wait_callbacks()).unwrap();
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));
    drop(queryable);

    tokio::time::sleep(Duration::from_secs(1)).await;

    // check matching listener
    let querier = ztimeout!(session.declare_querier(ke)).unwrap();
    let (cb, n) = create_callback::<MatchingStatus>();
    let listener = ztimeout!(querier.matching_listener().callback(cb)).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    declare_queryable_in_another_thread(&session, ke.to_string());
    tokio::time::sleep(Duration::from_secs(1)).await;
    ztimeout!(listener.undeclare().wait_callbacks()).unwrap();
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));

    tokio::time::sleep(Duration::from_secs(1)).await;

    // check background matching listener
    let (cb, n) = create_callback::<MatchingStatus>();
    ztimeout!(querier.matching_listener().callback(cb).background()).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    declare_queryable_in_another_thread(&session, ke.to_string());
    tokio::time::sleep(Duration::from_secs(1)).await;
    ztimeout!(querier.undeclare().wait_callbacks()).unwrap();

    assert!(n.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_callback_drop_on_undeclare_publisher() {
    zenoh::init_log_from_env_or("error");
    let ke = "test/undeclare/publisher_callback_drop";
    let (session1, session2) = ztimeout!(create_peer_pair("tcp/127.0.0.1:20005"));

    // check matching listener
    let publisher = ztimeout!(session1.declare_publisher(ke)).unwrap();
    let (cb, n) = create_callback::<MatchingStatus>();
    let listener = ztimeout!(publisher.matching_listener().callback(cb)).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    let _subscriber = ztimeout!(session2.declare_subscriber(ke)).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    ztimeout!(listener.undeclare().wait_callbacks()).unwrap();
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));
    drop(_subscriber);

    tokio::time::sleep(Duration::from_secs(1)).await;

    // check background matching listener
    let (cb, n) = create_callback::<MatchingStatus>();
    ztimeout!(publisher.matching_listener().callback(cb).background()).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    let _subscriber = ztimeout!(session2.declare_subscriber(ke)).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    ztimeout!(publisher.undeclare().wait_callbacks()).unwrap();

    assert!(n.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_callback_drop_on_undeclare_publisher_local() {
    fn declare_subscriber_in_another_thread(s: &Session, ke: String) {
        std::thread::spawn({
            let s = s.clone();
            move || {
                let _subscriber = s.declare_subscriber(ke).wait().unwrap();
                std::thread::sleep(Duration::from_secs(1));
            }
        });
    }

    zenoh::init_log_from_env_or("error");
    let ke = "test/undeclare/publisher_callback_drop_local";
    let session = ztimeout!(create_peer());

    // check matching listener
    let publisher = ztimeout!(session.declare_publisher(ke)).unwrap();
    let (cb, n) = create_callback::<MatchingStatus>();
    let listener = ztimeout!(publisher.matching_listener().callback(cb)).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    declare_subscriber_in_another_thread(&session, ke.to_string());
    tokio::time::sleep(Duration::from_secs(1)).await;
    ztimeout!(listener.undeclare().wait_callbacks()).unwrap();
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));

    tokio::time::sleep(Duration::from_secs(1)).await;

    // check background matching listener
    let (cb, n) = create_callback::<MatchingStatus>();
    ztimeout!(publisher.matching_listener().callback(cb).background()).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    declare_subscriber_in_another_thread(&session, ke.to_string());
    tokio::time::sleep(Duration::from_secs(1)).await;
    ztimeout!(publisher.undeclare().wait_callbacks()).unwrap();

    assert!(n.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_callback_drop_on_undeclare_get() {
    zenoh::init_log_from_env_or("error");
    let ke = "test/undeclare/get_callback_drop";
    let (session1, session2) = ztimeout!(create_peer_pair("tcp/127.0.0.1:20006"));

    let queryable = ztimeout!(session2.declare_queryable(ke)).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    let (cb, n) = create_callback::<Reply>();
    ztimeout!(session1.get(ke).callback(cb)).unwrap();
    let query = ztimeout!(queryable.recv_async()).unwrap();
    ztimeout!(query.reply(ke, "payload")).unwrap();
    drop(query);
    tokio::time::sleep(Duration::from_secs(1)).await;
    ztimeout!(session1.close().wait_callbacks()).unwrap();
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_callback_drop_on_undeclare_get_local() {
    fn reply_from_another_thread(q: Query, ke: String) {
        std::thread::spawn({
            move || {
                q.reply(ke, "payload").wait().unwrap();
            }
        });
    }

    zenoh::init_log_from_env_or("error");
    let ke = "test/undeclare/get_callback_drop_local";
    let session = ztimeout!(create_peer());

    let queryable = ztimeout!(session.declare_queryable(ke)).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    let (cb, n) = create_callback::<Reply>();
    ztimeout!(session.get(ke).callback(cb)).unwrap();
    let query = ztimeout!(queryable.recv_async()).unwrap();
    reply_from_another_thread(query, ke.to_string());
    tokio::time::sleep(Duration::from_secs(1)).await;
    ztimeout!(session.close().wait_callbacks()).unwrap();
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_callback_drop_on_undeclare_liveliness_get() {
    zenoh::init_log_from_env_or("error");
    let ke = "test/undeclare/liveliness_get_callback_drop";
    let (session1, session2) = ztimeout!(create_peer_pair("tcp/127.0.0.1:20007"));

    let _token = ztimeout!(session2.liveliness().declare_token(ke)).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    let (cb, n) = create_callback::<Reply>();
    ztimeout!(session1.liveliness().get(ke).callback(cb)).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    ztimeout!(session1.close().wait_callbacks()).unwrap();
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_callback_drop_on_undeclare_transport_events_listener() {
    zenoh::init_log_from_env_or("error");
    let locator = ["tcp/127.0.0.1:20008"];
    let session1 = open_session_listen(&locator).await;
    let (cb, n) = create_callback::<TransportEvent>();
    let listener = ztimeout!(session1.info().transport_events_listener().callback(cb)).unwrap();

    let session2 = open_session_connect(&locator).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    ztimeout!(listener.undeclare().wait_callbacks()).unwrap();
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));

    drop(session2);

    // check background events listener
    let (cb, n) = create_callback::<TransportEvent>();
    ztimeout!(session1
        .info()
        .transport_events_listener()
        .callback(cb)
        .background())
    .unwrap();
    let _session2 = open_session_connect(&locator).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    ztimeout!(session1.close().wait_callbacks()).unwrap();
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_callback_drop_on_undeclare_link_events_listener() {
    zenoh::init_log_from_env_or("error");
    let locator = ["tcp/127.0.0.1:20009"];
    let session1 = open_session_listen(&locator).await;
    let (cb, n) = create_callback::<LinkEvent>();
    let listener = ztimeout!(session1.info().link_events_listener().callback(cb)).unwrap();

    let session2 = open_session_connect(&locator).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    ztimeout!(listener.undeclare().wait_callbacks()).unwrap();
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));

    drop(session2);

    // check background events listener
    let (cb, n) = create_callback::<LinkEvent>();
    ztimeout!(session1
        .info()
        .link_events_listener()
        .callback(cb)
        .background())
    .unwrap();
    let _session2 = open_session_connect(&locator).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    ztimeout!(session1.close().wait_callbacks()).unwrap();
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));
}
