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
use std::time::Duration;
use zenoh::net::link::EndPoint;
use zenoh::net::protocol::core::{
    Channel, CongestionControl, Priority, Reliability, WhatAmI, ZenohId,
};
use zenoh::net::protocol::io::ZBuf;
use zenoh::net::protocol::message::ZenohMessage;
use zenoh::net::transport::{DummyTransportEventHandler, TransportManager};
use zenoh_util::zasync_executor_init;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

const MSG_SIZE: usize = 131_072;
const MSG_DEFRAG_BUF: usize = 128_000;

macro_rules! ztimeout {
    ($f:expr) => {
        $f.timeout(TIMEOUT).await.unwrap()
    };
}

async fn run(endpoint: &EndPoint, channel: Channel, msg_size: usize) {
    // Define client and router IDs
    let client_id = ZenohId::new(1, [0_u8; ZenohId::MAX_SIZE]);
    let router_id = ZenohId::new(1, [1_u8; ZenohId::MAX_SIZE]);

    // Create the router transport manager
    let router_manager = TransportManager::builder()
        .zid(router_id)
        .whatami(WhatAmI::Router)
        .defrag_buff_size(MSG_DEFRAG_BUF)
        .build(Arc::new(DummyTransportEventHandler::default()))
        .unwrap();

    // Create the client transport manager
    let client_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client_id)
        .defrag_buff_size(MSG_DEFRAG_BUF)
        .build(Arc::new(DummyTransportEventHandler::default()))
        .unwrap();

    // Create the listener on the router
    println!("Add locator: {}", endpoint);
    let _ = ztimeout!(router_manager.add_listener(endpoint.clone())).unwrap();

    // Create an empty transport with the client
    // Open transport -> This should be accepted
    println!("Opening transport with {}", endpoint);
    let _ = ztimeout!(client_manager.open_transport(endpoint.clone())).unwrap();

    let client_transport = client_manager.get_transport(&router_id).unwrap();

    // Create the message to send, this would trigger the transport closure
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
        "Sending message of {} bytes while defragmentation buffer size is {} bytes",
        msg_size, MSG_DEFRAG_BUF
    );
    client_transport.schedule(message.clone()).unwrap();

    // Wait that the client transport has been closed
    ztimeout!(async {
        while client_transport.get_zid().is_ok() {
            task::sleep(SLEEP).await;
        }
    });

    // Wait on the router manager that the transport has been closed
    ztimeout!(async {
        while !router_manager.get_transports_unicast().is_empty() {
            task::sleep(SLEEP).await;
        }
    });

    // Stop the locators on the manager
    println!("Del locator: {}", endpoint);
    let _ = ztimeout!(router_manager.del_listener(endpoint)).unwrap();

    // Wait a little bit
    ztimeout!(async {
        while !router_manager.get_listeners().is_empty() {
            task::sleep(SLEEP).await;
        }
    });

    task::sleep(SLEEP).await;

    ztimeout!(router_manager.close());
    ztimeout!(client_manager.close());

    // Wait a little bit
    task::sleep(SLEEP).await;
}

#[cfg(feature = "transport_tcp")]
#[test]
fn transport_unicast_defragmentation_tcp_only() {
    task::block_on(async {
        zasync_executor_init!();
    });

    // Define the locators
    let endpoint: EndPoint = "tcp/127.0.0.1:14447".parse().unwrap();
    // Define the reliability and congestion control
    let channel = [
        Channel {
            priority: Priority::default(),
            reliability: Reliability::Reliable,
        },
        Channel {
            priority: Priority::default(),
            reliability: Reliability::BestEffort,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::Reliable,
        },
        Channel {
            priority: Priority::RealTime,
            reliability: Reliability::BestEffort,
        },
    ];
    // Run
    task::block_on(async {
        for ch in channel.iter() {
            run(&endpoint, *ch, MSG_SIZE).await;
        }
    });
}
