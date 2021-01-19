//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use async_std::sync::Arc;
use async_std::task;
use async_trait::async_trait;
use std::time::Duration;
use zenoh_protocol::core::{whatami, PeerId};
use zenoh_protocol::link::{Link, Locator};
use zenoh_protocol::proto::ZenohMessage;
use zenoh_protocol::session::{
    Session, SessionEventHandler, SessionHandler, SessionManager, SessionManagerConfig,
};
use zenoh_util::core::ZResult;

const SLEEP: Duration = Duration::from_millis(100);
const RUNS: usize = 10;

// Session Handler
struct SH;

impl SH {
    fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SessionHandler for SH {
    async fn new_session(
        &self,
        _session: Session,
    ) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
        let arc = Arc::new(SC::new());
        Ok(arc)
    }
}

// Session Callback for the router
pub struct SC;

impl SC {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SessionEventHandler for SC {
    async fn handle_message(&self, _message: ZenohMessage) -> ZResult<()> {
        Ok(())
    }

    async fn new_link(&self, _link: Link) {}

    async fn del_link(&self, _link: Link) {}

    async fn closing(&self) {}

    async fn closed(&self) {}
}

async fn run(locators: Vec<Locator>) {
    // Create the session manager
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::PEER,
        id: PeerId::new(1, [0u8; PeerId::MAX_SIZE]),
        handler: Arc::new(SH::new()),
    };
    let sm = SessionManager::new(config, None);

    for _ in 0..RUNS {
        // Create the listeners
        for l in locators.iter() {
            println!("Add {}", l);
            let res = sm.add_listener(l).await;
            println!("Res: {:?}", res);
            assert!(res.is_ok());
        }

        task::sleep(SLEEP).await;

        // Delete the listeners
        for l in locators.iter() {
            println!("Del {}", l);
            let res = sm.del_listener(l).await;
            println!("Res: {:?}", res);
            assert!(res.is_ok());
        }

        task::sleep(SLEEP).await;
    }
}

#[cfg(feature = "transport_tcp")]
#[test]
fn locator_tcp() {
    // Define the locators
    let locators: Vec<Locator> = vec![
        "tcp/127.0.0.1:7447".parse().unwrap(),
        "tcp/localhost:7448".parse().unwrap(),
    ];
    task::block_on(run(locators));
}

#[cfg(feature = "transport_udp")]
#[test]
fn locator_udp() {
    // Define the locators
    let locators: Vec<Locator> = vec![
        "udp/127.0.0.1:7447".parse().unwrap(),
        "udp/localhost:7448".parse().unwrap(),
    ];
    task::block_on(run(locators));
}

#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
#[test]
fn locator_unix() {
    // Remove the files if they still exists
    let _ = std::fs::remove_file("zenoh-test-unix-socket-0.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-1.sock");
    // Define the locators
    let locators: Vec<Locator> = vec![
        "unixsock-stream/zenoh-test-unix-socket-0.sock"
            .parse()
            .unwrap(),
        "unixsock-stream/zenoh-test-unix-socket-1.sock"
            .parse()
            .unwrap(),
    ];
    task::block_on(run(locators));
    let _ = std::fs::remove_file("zenoh-test-unix-socket-0.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-1.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-0.sock.lock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-1.sock.lock");
}

#[cfg(all(feature = "transport_tcp", feature = "transport_udp"))]
#[test]
fn locator_tcp_udp() {
    // Define the locators
    let locators: Vec<Locator> = vec![
        "tcp/127.0.0.1:7449".parse().unwrap(),
        "udp/127.0.0.1:7449".parse().unwrap(),
    ];
    task::block_on(run(locators));
}

#[cfg(all(
    feature = "transport_tcp",
    feature = "transport_udp",
    feature = "transport_unixsock-stream",
    target_family = "unix"
))]
#[test]
fn locator_tcp_udp_unix() {
    // Remove the file if it still exists
    let _ = std::fs::remove_file("zenoh-test-unix-socket-2.sock");
    // Define the locators
    let locators: Vec<Locator> = vec![
        "tcp/127.0.0.1:7450".parse().unwrap(),
        "udp/127.0.0.1:7450".parse().unwrap(),
        "unixsock-stream/zenoh-test-unix-socket-2.sock"
            .parse()
            .unwrap(),
    ];
    task::block_on(run(locators));
    let _ = std::fs::remove_file("zenoh-test-unix-socket-2.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-2.sock.lock");
}

#[cfg(all(
    feature = "transport_tcp",
    feature = "transport_unixsock-stream",
    target_family = "unix"
))]
#[test]
fn locator_tcp_unix() {
    // Remove the file if it still exists
    let _ = std::fs::remove_file("zenoh-test-unix-socket-3.sock");
    // Define the locators
    let locators: Vec<Locator> = vec![
        "tcp/127.0.0.1:7451".parse().unwrap(),
        "unixsock-stream/zenoh-test-unix-socket-3.sock"
            .parse()
            .unwrap(),
    ];
    task::block_on(run(locators));
    let _ = std::fs::remove_file("zenoh-test-unix-socket-3.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-3.sock.lock");
}

#[cfg(all(
    feature = "transport_udp",
    feature = "transport_unixsock-stream",
    target_family = "unix"
))]
#[test]
fn locator_udp_unix() {
    // Remove the file if it still exists
    let _ = std::fs::remove_file("zenoh-test-unix-socket-4.sock");
    // Define the locators
    let locators: Vec<Locator> = vec![
        "udp/127.0.0.1:7451".parse().unwrap(),
        "unixsock-stream/zenoh-test-unix-socket-4.sock"
            .parse()
            .unwrap(),
    ];
    task::block_on(run(locators));
    let _ = std::fs::remove_file("zenoh-test-unix-socket-4.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-4.sock.lock");
}
