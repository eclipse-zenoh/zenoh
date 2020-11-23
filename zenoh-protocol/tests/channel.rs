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
use async_std::prelude::*;
use async_std::sync::Arc;
use async_std::task;
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use zenoh_protocol::core::{whatami, CongestionControl, PeerId, Reliability, ResKey};
use zenoh_protocol::io::RBuf;
use zenoh_protocol::link::{Link, Locator};
use zenoh_protocol::proto::ZenohMessage;
use zenoh_protocol::session::{
    Session, SessionEventHandler, SessionHandler, SessionManager, SessionManagerConfig,
};
use zenoh_util::core::ZResult;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

const MSG_COUNT: usize = 1_000;
const MSG_SIZE: usize = 1_024;

// Session Handler for the router
struct SHRouter {
    count: Arc<AtomicUsize>,
}

impl SHRouter {
    fn new() -> Self {
        Self {
            count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn get_count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl SessionHandler for SHRouter {
    async fn new_session(
        &self,
        _session: Session,
    ) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
        let arc = Arc::new(SCRouter::new(self.count.clone()));
        Ok(arc)
    }
}

// Session Callback for the router
pub struct SCRouter {
    count: Arc<AtomicUsize>,
}

impl SCRouter {
    pub fn new(count: Arc<AtomicUsize>) -> Self {
        Self { count }
    }
}

#[async_trait]
impl SessionEventHandler for SCRouter {
    async fn handle_message(&self, _message: ZenohMessage) -> ZResult<()> {
        self.count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn new_link(&self, _link: Link) {}

    async fn del_link(&self, _link: Link) {}

    async fn closing(&self) {}

    async fn closed(&self) {}
}

// Session Handler for the client
struct SHClient {}

impl SHClient {
    fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl SessionHandler for SHClient {
    async fn new_session(
        &self,
        _session: Session,
    ) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
        Ok(Arc::new(SCClient::new()))
    }
}

// Session Callback for the client
pub struct SCClient;

impl SCClient {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SessionEventHandler for SCClient {
    async fn handle_message(&self, _message: ZenohMessage) -> ZResult<()> {
        Ok(())
    }

    async fn new_link(&self, _link: Link) {}

    async fn del_link(&self, _link: Link) {}

    async fn closing(&self) {}

    async fn closed(&self) {}
}

async fn open_session(locators: Vec<Locator>) -> (SessionManager, Arc<SHRouter>, Session) {
    // Define client and router IDs
    let client_id = PeerId::new(1, [0u8; PeerId::MAX_SIZE]);
    let router_id = PeerId::new(1, [1u8; PeerId::MAX_SIZE]);

    // Create the router session manager
    let router_handler = Arc::new(SHRouter::new());
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::ROUTER,
        id: router_id.clone(),
        handler: router_handler.clone(),
    };
    let router_manager = SessionManager::new(config, None);

    // Create the client session manager
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::CLIENT,
        id: client_id,
        handler: Arc::new(SHClient::new()),
    };
    let client_manager = SessionManager::new(config, None);

    // Create the listener on the router
    for l in locators.iter() {
        println!("Add locator: {}", l);
        let res = router_manager.add_listener(l).await;
        assert!(res.is_ok());
    }

    // Create an empty session with the client
    // Open session -> This should be accepted
    let attachment = None;
    for l in locators.iter() {
        println!("Opening session with {}", l);
        let res = client_manager.open_session(l, &attachment).await;
        assert_eq!(res.is_ok(), true);
    }
    let client_session = client_manager.get_session(&router_id).await.unwrap();

    // Return the handlers
    (router_manager, router_handler, client_session)
}

async fn close_session(
    router_manager: SessionManager,
    client_session: Session,
    locators: Vec<Locator>,
) {
    // Close the client session
    let mut ll = "".to_string();
    for l in locators.iter() {
        ll.push_str(&format!("{} ", l));
    }
    println!("Closing session with {}", ll);
    let res = client_session.close().await;
    assert!(res.is_ok());

    // Wait a little bit
    task::sleep(SLEEP).await;

    // Stop the locators on the manager
    for l in locators.iter() {
        println!("Del locator: {}", l);
        let res = router_manager.del_listener(l).await;
        assert!(res.is_ok());
    }
}

async fn run(
    router_handler: Arc<SHRouter>,
    client_session: Session,
    reliability: Reliability,
    congestion_control: CongestionControl,
) {
    // Create the message to send
    let key = ResKey::RName("/test".to_string());
    let payload = RBuf::from(vec![0u8; MSG_SIZE]);
    let data_info = None;
    let reply_context = None;
    let attachment = None;
    let message = ZenohMessage::make_data(
        key,
        payload,
        reliability,
        congestion_control,
        data_info,
        reply_context,
        attachment,
    );

    println!(
        "Sending {} messages... {:?} {:?}",
        MSG_COUNT, reliability, congestion_control
    );
    for _ in 0..MSG_COUNT {
        client_session.schedule(message.clone()).await.unwrap();
    }

    macro_rules! some {
        () => {
            // Wait to receive something
            let count = async {
                while router_handler.get_count() == 0 {
                    task::yield_now().await;
                }
            };
            let res = count.timeout(TIMEOUT).await;
            assert!(res.is_ok());
        };
    }

    macro_rules! all {
        () => {
            // Wait for the messages to arrive to the other side
            let count = async {
                while router_handler.get_count() != MSG_COUNT {
                    task::yield_now().await;
                }
            };
            let res = count.timeout(TIMEOUT).await;
            assert!(res.is_ok());
        };
    }

    match reliability {
        Reliability::Reliable => match congestion_control {
            CongestionControl::Block => {
                all!();
            }
            CongestionControl::Drop => {
                some!();
            }
        },
        Reliability::BestEffort => {
            some!();
        }
    }
}

#[test]
fn channel_tcp() {
    // Define the locators
    let locators: Vec<Locator> = vec!["tcp/127.0.0.1:7447".parse().unwrap()];
    // Define the reliability and congestion control
    let reliability = [Reliability::Reliable, Reliability::BestEffort];
    let congestion_control = [CongestionControl::Block, CongestionControl::Drop];
    // Run
    task::block_on(async {
        let (router_manager, router_handler, client_session) = open_session(locators.clone()).await;
        for rl in reliability.iter() {
            for cc in congestion_control.iter() {
                run(router_handler.clone(), client_session.clone(), *rl, *cc).await;
            }
        }
        close_session(router_manager, client_session, locators).await;
    });
}

#[test]
fn channel_udp() {
    // Define the locator
    let locators: Vec<Locator> = vec!["udp/127.0.0.1:7447".parse().unwrap()];
    // Define the reliability and congestion control
    let reliability = [Reliability::BestEffort];
    let congestion_control = [CongestionControl::Block, CongestionControl::Drop];
    // Run
    task::block_on(async {
        let (router_manager, router_handler, client_session) = open_session(locators.clone()).await;
        for rl in reliability.iter() {
            for cc in congestion_control.iter() {
                run(router_handler.clone(), client_session.clone(), *rl, *cc).await;
            }
        }
        close_session(router_manager, client_session, locators).await;
    });
}

#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
#[test]
fn channel_unix() {
    let _ = std::fs::remove_file("zenoh-test-unix-socket-5.sock");
    // Define the locator
    let locators: Vec<Locator> = vec!["unixsock-stream/zenoh-test-unix-socket-5.sock"
        .parse()
        .unwrap()];
    // Define the reliability and congestion control
    let reliability = [Reliability::BestEffort];
    let congestion_control = [CongestionControl::Block, CongestionControl::Drop];
    // Run
    task::block_on(async {
        let (router_manager, router_handler, client_session) = open_session(locators.clone()).await;
        for rl in reliability.iter() {
            for cc in congestion_control.iter() {
                run(router_handler.clone(), client_session.clone(), *rl, *cc).await;
            }
        }
        close_session(router_manager, client_session, locators).await;
    });
    let _ = std::fs::remove_file("zenoh-test-unix-socket-5.sock");
}

#[test]
fn channel_tcp_udp() {
    // Define the locator
    let locators: Vec<Locator> = vec![
        "tcp/127.0.0.1:7448".parse().unwrap(),
        "udp/127.0.0.1:7448".parse().unwrap(),
    ];
    // Define the reliability and congestion control
    let reliability = [Reliability::BestEffort];
    let congestion_control = [CongestionControl::Block, CongestionControl::Drop];
    // Run
    task::block_on(async {
        let (router_manager, router_handler, client_session) = open_session(locators.clone()).await;
        for rl in reliability.iter() {
            for cc in congestion_control.iter() {
                run(router_handler.clone(), client_session.clone(), *rl, *cc).await;
            }
        }
        close_session(router_manager, client_session, locators).await;
    });
}

#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
#[test]
fn channel_tcp_unix() {
    let _ = std::fs::remove_file("zenoh-test-unix-socket-6.sock");
    // Define the locator
    let locators: Vec<Locator> = vec![
        "tcp/127.0.0.1:7449".parse().unwrap(),
        "unixsock-stream/zenoh-test-unix-socket-6.sock"
            .parse()
            .unwrap(),
    ];
    // Define the reliability and congestion control
    let reliability = [Reliability::BestEffort];
    let congestion_control = [CongestionControl::Block, CongestionControl::Drop];
    // Run
    task::block_on(async {
        let (router_manager, router_handler, client_session) = open_session(locators.clone()).await;
        for rl in reliability.iter() {
            for cc in congestion_control.iter() {
                run(router_handler.clone(), client_session.clone(), *rl, *cc).await;
            }
        }
        close_session(router_manager, client_session, locators).await;
    });
    let _ = std::fs::remove_file("zenoh-test-unix-socket-6.sock");
}

#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
#[test]
fn channel_udp_unix() {
    let _ = std::fs::remove_file("zenoh-test-unix-socket-7.sock");
    // Define the locator
    let locators: Vec<Locator> = vec![
        "udp/127.0.0.1:7449".parse().unwrap(),
        "unixsock-stream/zenoh-test-unix-socket-7.sock"
            .parse()
            .unwrap(),
    ];
    // Define the reliability and congestion control
    let reliability = [Reliability::BestEffort];
    let congestion_control = [CongestionControl::Block, CongestionControl::Drop];
    // Run
    task::block_on(async {
        let (router_manager, router_handler, client_session) = open_session(locators.clone()).await;
        for rl in reliability.iter() {
            for cc in congestion_control.iter() {
                run(router_handler.clone(), client_session.clone(), *rl, *cc).await;
            }
        }
        close_session(router_manager, client_session, locators).await;
    });
    let _ = std::fs::remove_file("zenoh-test-unix-socket-7.sock");
}

#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
#[test]
fn channel_tcp_udp_unix() {
    let _ = std::fs::remove_file("zenoh-test-unix-socket-8.sock");
    // Define the locator
    let locators: Vec<Locator> = vec![
        "tcp/127.0.0.1:7450".parse().unwrap(),
        "udp/127.0.0.1:7450".parse().unwrap(),
        "unixsock-stream/zenoh-test-unix-socket-8.sock"
            .parse()
            .unwrap(),
    ];
    // Define the reliability and congestion control
    let reliability = [Reliability::BestEffort];
    let congestion_control = [CongestionControl::Block, CongestionControl::Drop];
    // Run
    task::block_on(async {
        let (router_manager, router_handler, client_session) = open_session(locators.clone()).await;
        for rl in reliability.iter() {
            for cc in congestion_control.iter() {
                run(router_handler.clone(), client_session.clone(), *rl, *cc).await;
            }
        }
        close_session(router_manager, client_session, locators).await;
    });
    let _ = std::fs::remove_file("zenoh-test-unix-socket-8.sock");
}
