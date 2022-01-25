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
use zenoh::net::link::{EndPoint, Link};
use zenoh::net::protocol::core::{
    Channel, CongestionControl, Priority, Reliability, WhatAmI, ZenohId,
};
use zenoh::net::protocol::io::ZBuf;
use zenoh::net::protocol::message::ZenohMessage;
use zenoh::net::transport::{
    TransportEventHandler, TransportManager, TransportMulticast, TransportMulticastEventHandler,
    TransportPeer, TransportPeerEventHandler, TransportUnicast,
};
use zenoh_util::core::Result as ZResult;
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

macro_rules! ztimeout {
    ($f:expr) => {
        $f.timeout(TIMEOUT).await.unwrap()
    };
}

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
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
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

impl TransportPeerEventHandler for SCRouter {
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
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
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

impl TransportPeerEventHandler for SCClient {
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

async fn open_transport(
    endpoints: &[EndPoint],
    priority: Priority,
) -> (
    TransportManager,
    Arc<SHRouter>,
    TransportManager,
    TransportUnicast,
) {
    // Define client and router IDs
    let client_id = ZenohId::new(1, [0_u8; ZenohId::MAX_SIZE]);
    let router_id = ZenohId::new(1, [1_u8; ZenohId::MAX_SIZE]);

    // Create the router transport manager
    let router_handler = Arc::new(SHRouter::new(priority));
    let router_manager = TransportManager::builder()
        .whatami(WhatAmI::Router)
        .zid(router_id)
        .build(router_handler.clone())
        .unwrap();

    // Create the client transport manager
    let client_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client_id)
        .build(Arc::new(SHClient::default()))
        .unwrap();

    // Create the listener on the router
    for e in endpoints.iter() {
        println!("Add locator: {}", e);
        let _ = ztimeout!(router_manager.add_listener(e.clone())).unwrap();
    }

    // Create an empty transport with the client
    // Open transport -> This should be accepted
    for e in endpoints.iter() {
        println!("Opening transport with {}", e);
        let _ = ztimeout!(client_manager.open_transport(e.clone())).unwrap();
    }

    let client_transport = client_manager.get_transport(&router_id).unwrap();

    // Return the handlers
    (
        router_manager,
        router_handler,
        client_manager,
        client_transport,
    )
}

async fn close_transport(
    router_manager: TransportManager,
    client_manager: TransportManager,
    client_transport: TransportUnicast,
    endpoints: &[EndPoint],
) {
    // Close the client transport
    let mut ee = "".to_string();
    for e in endpoints.iter() {
        ee.push_str(&format!("{} ", e));
    }
    println!("Closing transport with {}", ee);
    let _ = ztimeout!(client_transport.close()).unwrap();

    ztimeout!(async {
        while !router_manager.get_transports().is_empty() {
            task::sleep(SLEEP).await;
        }
    });

    // Stop the locators on the manager
    for e in endpoints.iter() {
        println!("Del locator: {}", e);
        let _ = ztimeout!(router_manager.del_listener(e)).unwrap();
    }

    // Wait a little bit
    task::sleep(SLEEP).await;

    ztimeout!(router_manager.close());
    ztimeout!(client_manager.close());

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
    let key = "/test".into();
    let payload = ZBuf::from(vec![0_u8; msg_size]);
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
    ztimeout!(async {
        while router_handler.get_count() != MSG_COUNT {
            task::sleep(SLEEP_COUNT).await;
        }
    });

    // Wait a little bit
    task::sleep(SLEEP).await;
}

async fn run(endpoints: &[EndPoint], channel: &[Channel], msg_size: &[usize]) {
    for ch in channel.iter() {
        for ms in msg_size.iter() {
            let (router_manager, router_handler, client_manager, client_transport) =
                open_transport(endpoints, ch.priority).await;
            single_run(router_handler.clone(), client_transport.clone(), *ch, *ms).await;
            close_transport(router_manager, client_manager, client_transport, endpoints).await;
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
    let endpoints: Vec<EndPoint> = vec!["tcp/127.0.0.1:13447".parse().unwrap()];
    // Run
    task::block_on(run(&endpoints, &channel, &MSG_SIZE_ALL));
}
