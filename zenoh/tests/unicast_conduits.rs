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
use zenoh::net::link::{LinkUnicast, Locator};
use zenoh::net::protocol::core::{
    whatami, Channel, CongestionControl, PeerId, Priority, Reliability, ResKey,
};
use zenoh::net::protocol::io::ZBuf;
use zenoh::net::protocol::proto::ZenohMessage;
use zenoh::net::transport::{
    TransportEventHandler, TransportManager, TransportManagerConfig, TransportMulticast,
    TransportMulticastEventHandler, TransportUnicast, TransportUnicastEventHandler,
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

// Transport Handler for the router
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

impl TransportEventHandler for SHRouter {
    fn new_unicast(
        &self,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportUnicastEventHandler>> {
        let arc = Arc::new(SCRouter::new(self.count.clone(), self.priority));
        Ok(arc)
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        panic!();
    }
}

// Transport Callback for the router
pub struct SCRouter {
    priority: Priority,
    count: Arc<AtomicUsize>,
}

impl SCRouter {
    pub fn new(count: Arc<AtomicUsize>, priority: Priority) -> Self {
        Self { priority, count }
    }
}

impl TransportUnicastEventHandler for SCRouter {
    fn handle_message(&self, message: ZenohMessage) -> ZResult<()> {
        assert_eq!(self.priority, message.channel.priority);
        self.count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn new_link(&self, _link: LinkUnicast) {}
    fn del_link(&self, _link: LinkUnicast) {}
    fn closing(&self) {}
    fn closed(&self) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}

// Transport Handler for the client
struct SHClient;

impl Default for SHClient {
    fn default() -> Self {
        Self
    }
}

impl TransportEventHandler for SHClient {
    fn new_unicast(
        &self,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportUnicastEventHandler>> {
        Ok(Arc::new(SCClient::default()))
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        panic!();
    }
}

// Transport Callback for the client
pub struct SCClient;

impl Default for SCClient {
    fn default() -> Self {
        Self
    }
}

impl TransportUnicastEventHandler for SCClient {
    fn handle_message(&self, _message: ZenohMessage) -> ZResult<()> {
        Ok(())
    }

    fn new_link(&self, _link: LinkUnicast) {}
    fn del_link(&self, _link: LinkUnicast) {}
    fn closing(&self) {}
    fn closed(&self) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}

async fn open_transport(
    locators: &[Locator],
    priority: Priority,
) -> (TransportManager, Arc<SHRouter>, TransportUnicast) {
    // Define client and router IDs
    let client_id = PeerId::new(1, [0u8; PeerId::MAX_SIZE]);
    let router_id = PeerId::new(1, [1u8; PeerId::MAX_SIZE]);

    // Create the router transport manager
    let router_handler = Arc::new(SHRouter::new(priority));
    let config = TransportManagerConfig::builder()
        .whatami(whatami::ROUTER)
        .pid(router_id)
        .build(router_handler.clone());
    let router_manager = TransportManager::new(config);

    // Create the client transport manager
    let config = TransportManagerConfig::builder()
        .whatami(whatami::CLIENT)
        .pid(client_id)
        .build(Arc::new(SHClient::default()));
    let client_manager = TransportManager::new(config);

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

    // Create an empty transport with the client
    // Open transport -> This should be accepted
    for l in locators.iter() {
        println!("Opening transport with {}", l);
        let _ = client_manager
            .open_transport(l)
            .timeout(TIMEOUT)
            .await
            .unwrap()
            .unwrap();
    }

    let client_transport = client_manager.get_transport(&router_id).unwrap();

    // Return the handlers
    (router_manager, router_handler, client_transport)
}

async fn close_transport(
    router_manager: TransportManager,
    client_transport: TransportUnicast,
    locators: &[Locator],
) {
    // Close the client transport
    let mut ll = "".to_string();
    for l in locators.iter() {
        ll.push_str(&format!("{} ", l));
    }
    println!("Closing transport with {}", ll);
    let _ = client_transport
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
    client_transport: TransportUnicast,
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
        client_transport.schedule(message.clone()).unwrap();
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
            let (router_manager, router_handler, client_transport) =
                open_transport(locators, ch.priority).await;
            single_run(router_handler.clone(), client_transport.clone(), *ch, *ms).await;
            close_transport(router_manager, client_transport, locators).await;
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
