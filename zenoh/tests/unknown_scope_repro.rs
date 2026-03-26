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

use std::time::{Duration, Instant};

use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use zenoh::{
    bytes::ZBytes,
    config::WhatAmI,
    qos::{CongestionControl, Priority},
    Session,
};
use zenoh_config::Config;
use zenoh_core::ztimeout;
use zenoh_link::EndPoint;

const LISTEN_ENDPOINT: &str = "tcp/127.0.0.1:31003";
const CONNECT_ENDPOINT: &str = "tcp/127.0.0.1:31003?rel=1;prio=1-7";
const KEY_PREFIX: &str = "test/unknown_scope/repro";

const TIMEOUT: Duration = Duration::from_secs(60);
const PROPAGATION_DELAY: Duration = Duration::from_millis(500);
const RECEIVE_TIMEOUT: Duration = Duration::from_secs(15);
const ITERATIONS: usize = 8;
const MSGS_PER_ITERATION: usize = 32;
const PAYLOAD_SIZE: usize = 256 * 1024;

fn router_config() -> Config {
    let mut config = Config::default();
    config.set_mode(Some(WhatAmI::Router)).unwrap();
    config
        .listen
        .endpoints
        .set(vec![LISTEN_ENDPOINT.parse::<EndPoint>().unwrap()])
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    config.transport.unicast.qos.set_enabled(true).unwrap();
    config
}

fn client_config() -> Config {
    let mut config = Config::default();
    config.set_mode(Some(WhatAmI::Client)).unwrap();
    config.listen.endpoints.set(vec![]).unwrap();
    config
        .connect
        .endpoints
        .set(vec![CONNECT_ENDPOINT.parse::<EndPoint>().unwrap()])
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    config.transport.unicast.qos.set_enabled(true).unwrap();
    config
}

async fn receive_key(rx: &mut UnboundedReceiver<String>, key_expr: &str, expected: usize) -> usize {
    let deadline = Instant::now() + RECEIVE_TIMEOUT;
    let mut received = 0;

    while received < expected {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            break;
        };

        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Some(sample_key)) if sample_key == key_expr => {
                received += 1;
            }
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => break,
        }
    }

    received
}

async fn open(config: Config) -> Session {
    ztimeout!(zenoh::open(config)).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn publisher_undeclare_does_not_drop_inflight_data() {
    zenoh::init_log_from_env_or("error");

    let router = open(router_config()).await;
    let subscriber_session = open(client_config()).await;
    let publisher_session = open(client_config()).await;

    let (tx, mut rx) = unbounded_channel::<String>();
    let _subscriber = ztimeout!(subscriber_session
        .declare_subscriber(format!("{KEY_PREFIX}/**"))
        .callback(move |sample| {
            tx.send(sample.key_expr().to_string()).unwrap();
        }))
    .unwrap();

    tokio::time::sleep(PROPAGATION_DELAY).await;

    let payload = ZBytes::from(vec![0_u8; PAYLOAD_SIZE]);
    for iteration in 0..ITERATIONS {
        let key_expr = format!("{KEY_PREFIX}/{iteration}");
        let publisher = ztimeout!(publisher_session
            .declare_publisher(key_expr.clone())
            .priority(Priority::Background)
            .congestion_control(CongestionControl::Block))
        .unwrap();

        for _ in 0..MSGS_PER_ITERATION {
            ztimeout!(publisher.put(payload.clone())).unwrap();
        }

        drop(publisher);

        let received = receive_key(&mut rx, &key_expr, MSGS_PER_ITERATION).await;
        assert_eq!(
            received, MSGS_PER_ITERATION,
            "late Push messages were lost after publisher undeclare for {key_expr}"
        );
    }

    publisher_session.close().await.unwrap();
    subscriber_session.close().await.unwrap();
    router.close().await.unwrap();
}
