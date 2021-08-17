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
use std::any::Any;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use zenoh::net::protocol::core::{
    whatami, Channel, CongestionControl, PeerId, Priority, Reliability, ResKey,
};
use zenoh::net::protocol::io::ZBuf;
use zenoh::net::protocol::link::{Link, Locator};
use zenoh::net::protocol::proto::ZenohMessage;
use zenoh::net::protocol::session::{
    Session, SessionEventHandler, SessionHandler, SessionManager, SessionManagerConfig,
};
use zenoh_util::core::ZResult;
use zenoh_util::zasync_executor_init;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);
const SLEEP_COUNT: Duration = Duration::from_millis(10);

const MSG_COUNT: usize = 100;
const MSG_SIZE_ALL: [usize; 2] = [1_024, 131_072];

const PRIORITY_ALL: [Priority; 8] = [
    Priority::Control,
    Priority::RealTime,
    Priority::InteractiveHigh,
    Priority::InteractiveLow,
    Priority::DataHigh,
    Priority::Data,
    Priority::DataLow,
    Priority::Background,
];

// Session Handler for the router
struct SHRouter {
    priority: Priority,
    count: Arc<AtomicUsize>,
}

impl SHRouter {
    fn new(priority: Priority) -> Self {
        Self {
            priority,
            count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn get_count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}

impl SessionHandler for SHRouter {
    fn new_session(&self, _session: Session) -> ZResult<Arc<dyn SessionEventHandler>> {
        let arc = Arc::new(SCRouter::new(self.count.clone(), self.priority));
        Ok(arc)
    }
}

// Session Callback for the router
pub struct SCRouter {
    priority: Priority,
    count: Arc<AtomicUsize>,
}

impl SCRouter {
    pub fn new(count: Arc<AtomicUsize>, priority: Priority) -> Self {
        Self { priority, count }
    }
}

impl SessionEventHandler for SCRouter {
    fn handle_message(&self, message: ZenohMessage) -> ZResult<()> {
        assert_eq!(self.priority, message.channel.priority);
        self.count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn new_link(&self, _link: Link) {}
    fn del_link(&self, _link: Link) {}
    fn closing(&self) {}
    fn closed(&self) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}

// Session Handler for the client
struct SHClient;

impl Default for SHClient {
    fn default() -> Self {
        Self
    }
}

impl SessionHandler for SHClient {
    fn new_session(&self, _session: Session) -> ZResult<Arc<dyn SessionEventHandler>> {
        Ok(Arc::new(SCClient::default()))
    }
}

// Session Callback for the client
pub struct SCClient;

impl Default for SCClient {
    fn default() -> Self {
        Self
    }
}

impl SessionEventHandler for SCClient {
    fn handle_message(&self, _message: ZenohMessage) -> ZResult<()> {
        Ok(())
    }

    fn new_link(&self, _link: Link) {}
    fn del_link(&self, _link: Link) {}
    fn closing(&self) {}
    fn closed(&self) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}

async fn open_session(
    locators: &[Locator],
    priority: Priority,
) -> (SessionManager, Arc<SHRouter>, Session) {
    // Define client and router IDs
    let client_id = PeerId::new(1, [0u8; PeerId::MAX_SIZE]);
    let router_id = PeerId::new(1, [1u8; PeerId::MAX_SIZE]);

    // Create the router session manager
    let router_handler = Arc::new(SHRouter::new(priority));
    let config = SessionManagerConfig::builder()
        .whatami(whatami::ROUTER)
        .pid(router_id.clone())
        .build(router_handler.clone());
    let router_manager = SessionManager::new(config);

    // Create the client session manager
    let config = SessionManagerConfig::builder()
        .whatami(whatami::CLIENT)
        .pid(client_id)
        .build(Arc::new(SHClient::default()));
    let client_manager = SessionManager::new(config);

    // Create the listener on the router
    for l in locators.iter() {
        println!("Add locator: {}", l);
        let _ = router_manager
            .add_listener(l)
            .timeout(TIMEOUT)
            .await
            .unwrap()
            .unwrap();
    }

    // Create an empty session with the client
    // Open session -> This should be accepted
    for l in locators.iter() {
        println!("Opening session with {}", l);
        let _ = client_manager
            .open_session(l)
            .timeout(TIMEOUT)
            .await
            .unwrap()
            .unwrap();
    }

    let client_session = client_manager.get_session(&router_id).unwrap();

    // Return the handlers
    (router_manager, router_handler, client_session)
}

async fn close_session(
    router_manager: SessionManager,
    client_session: Session,
    locators: &[Locator],
) {
    // Close the client session
    let mut ll = "".to_string();
    for l in locators.iter() {
        ll.push_str(&format!("{} ", l));
    }
    println!("Closing session with {}", ll);
    let _ = client_session
        .close()
        .timeout(TIMEOUT)
        .await
        .unwrap()
        .unwrap();

    // Wait a little bit
    task::sleep(SLEEP).await;

    // Stop the locators on the manager
    for l in locators.iter() {
        println!("Del locator: {}", l);
        let _ = router_manager
            .del_listener(l)
            .timeout(TIMEOUT)
            .await
            .unwrap()
            .unwrap();
    }

    // Wait a little bit
    task::sleep(SLEEP).await;
}

async fn single_run(
    router_handler: Arc<SHRouter>,
    client_session: Session,
    channel: Channel,
    msg_size: usize,
) {
    // Create the message to send
    let key = ResKey::RName("/test".to_string());
    let payload = ZBuf::from(vec![0u8; msg_size]);
    let data_info = None;
    let routing_context = None;
    let reply_context = None;
    let attachment = None;
    let message = ZenohMessage::make_data(
        key,
        payload,
        channel,
        CongestionControl::Block,
        data_info,
        routing_context,
        reply_context,
        attachment,
    );

    println!(
        "Sending {} messages... {:?} {}",
        MSG_COUNT, channel, msg_size
    );
    for _ in 0..MSG_COUNT {
        client_session.schedule(message.clone()).unwrap();
    }

    // Wait for the messages to arrive to the other side
    let count = async {
        while router_handler.get_count() != MSG_COUNT {
            task::sleep(SLEEP_COUNT).await;
        }
    };
    let _ = count.timeout(TIMEOUT).await.unwrap();

    // Wait a little bit
    task::sleep(SLEEP).await;
}

async fn run(locators: &[Locator], channel: &[Channel], msg_size: &[usize]) {
    for ch in channel.iter() {
        for ms in msg_size.iter() {
            let (router_manager, router_handler, client_session) =
                open_session(locators, ch.priority).await;
            single_run(router_handler.clone(), client_session.clone(), *ch, *ms).await;
            close_session(router_manager, client_session, locators).await;
        }
    }
}

#[cfg(feature = "transport_tcp")]
#[test]
fn conduits_tcp_only() {
    task::block_on(async {
        zasync_executor_init!();
    });
    let mut channel = vec![];
    for p in PRIORITY_ALL.iter() {
        channel.push(Channel {
            priority: *p,
            reliability: Reliability::Reliable,
        });
    }
    // Define the locators
    let locators: Vec<Locator> = vec!["tcp/127.0.0.1:13447".parse().unwrap()];
    // Run
    task::block_on(run(&locators, &channel, &MSG_SIZE_ALL));
}
