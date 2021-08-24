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
use zenoh::net::link::{Locator, LocatorProperty};
use zenoh::net::protocol::core::{
    whatami, Channel, CongestionControl, PeerId, Priority, Reliability, ResKey,
};
use zenoh::net::protocol::io::ZBuf;
use zenoh::net::protocol::proto::ZenohMessage;
use zenoh::net::transport::{DummyTransportEventHandler, TransportManager, TransportManagerConfig};
use zenoh_util::zasync_executor_init;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

const MSG_SIZE: usize = 131_072;
const MSG_DEFRAG_BUF: usize = 128_000;

async fn run(
    locator: &Locator,
    locator_property: Option<Vec<LocatorProperty>>,
    channel: Channel,
    msg_size: usize,
) {
    // Define client and router IDs
    let client_id = PeerId::new(1, [0u8; PeerId::MAX_SIZE]);
    let router_id = PeerId::new(1, [1u8; PeerId::MAX_SIZE]);

    // Create the router transport manager
    let config = TransportManagerConfig::builder()
        .pid(router_id)
        .whatami(whatami::ROUTER)
        .defrag_buff_size(MSG_DEFRAG_BUF)
        .locator_property(locator_property.clone().unwrap_or_else(Vec::new))
        .build(Arc::new(DummyTransportEventHandler::default()));
    let router_manager = TransportManager::new(config);

    // Create the client transport manager
    let config = TransportManagerConfig::builder()
        .whatami(whatami::CLIENT)
        .pid(client_id)
        .defrag_buff_size(MSG_DEFRAG_BUF)
        .locator_property(locator_property.unwrap_or_else(Vec::new))
        .build(Arc::new(DummyTransportEventHandler::default()));
    let client_manager = TransportManager::new(config);

    // Create the listener on the router
    println!("Add locator: {}", locator);
    let _ = router_manager
        .add_listener(locator)
        .timeout(TIMEOUT)
        .await
        .unwrap()
        .unwrap();

    // Create an empty transport with the client
    // Open transport -> This should be accepted
    println!("Opening transport with {}", locator);
    let _ = client_manager
        .open_transport(locator)
        .timeout(TIMEOUT)
        .await
        .unwrap()
        .unwrap();

    let client_transport = client_manager.get_transport(&router_id).unwrap();

    // Create the message to send, this would trigger the transport closure
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
        "Sending message of {} bytes while defragmentation buffer size is {} bytes",
        msg_size, MSG_DEFRAG_BUF
    );
    client_transport.schedule(message.clone()).unwrap();

    // Wait that the client transport has been closed
    let closed = async {
        while client_transport.get_pid().is_ok() {
            task::sleep(SLEEP).await;
        }
    };
    let _ = closed.timeout(TIMEOUT).await.unwrap();

    // Wait on the router manager that the transport has been closed
    let closed = async {
        while !router_manager.get_transports_unicast().is_empty() {
            task::sleep(SLEEP).await;
        }
    };
    let _ = closed.timeout(TIMEOUT).await.unwrap();

    // Stop the locators on the manager
    println!("Del locator: {}", locator);
    let _ = router_manager
        .del_listener(locator)
        .timeout(TIMEOUT)
        .await
        .unwrap()
        .unwrap();

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
    let locator: Locator = "tcp/127.0.0.1:14447".parse().unwrap();
    let properties = None;
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
            run(&locator, properties.clone(), *ch, MSG_SIZE).await;
        }
    });
}
