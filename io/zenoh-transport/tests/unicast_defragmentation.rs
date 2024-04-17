//
// Copyright (c) 2023 ZettaScale Technology
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
use std::{convert::TryFrom, sync::Arc, time::Duration};
use zenoh_core::ztimeout;
use zenoh_protocol::{
    core::{
        Channel, CongestionControl, Encoding, EndPoint, Priority, Reliability, WhatAmI, ZenohId,
    },
    network::{
        push::{
            ext::{NodeIdType, QoSType},
            Push,
        },
        NetworkMessage,
    },
    zenoh::Put,
};
use zenoh_transport::{DummyTransportEventHandler, TransportManager};

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

const MSG_SIZE: usize = 131_072;
const MSG_DEFRAG_BUF: usize = 128_000;

async fn run(endpoint: &EndPoint, channel: Channel, msg_size: usize) {
    // Define client and router IDs
    let client_id = ZenohId::try_from([1]).unwrap();
    let router_id = ZenohId::try_from([2]).unwrap();

    // Create the router transport manager
    let router_manager = TransportManager::builder()
        .zid(router_id)
        .whatami(WhatAmI::Router)
        .defrag_buff_size(MSG_DEFRAG_BUF)
        .build(Arc::new(DummyTransportEventHandler))
        .unwrap();

    // Create the client transport manager
    let client_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client_id)
        .defrag_buff_size(MSG_DEFRAG_BUF)
        .build(Arc::new(DummyTransportEventHandler))
        .unwrap();

    // Create the listener on the router
    println!("Add locator: {endpoint}");
    let _ = ztimeout!(router_manager.add_listener(endpoint.clone())).unwrap();

    // Create an empty transport with the client
    // Open transport -> This should be accepted
    println!("Opening transport with {endpoint}");
    let _ = ztimeout!(client_manager.open_transport_unicast(endpoint.clone())).unwrap();

    let client_transport = client_manager
        .get_transport_unicast(&router_id)
        .await
        .unwrap();

    // Create the message to send
    let message: NetworkMessage = Push {
        wire_expr: "test".into(),
        ext_qos: QoSType::new(channel.priority, CongestionControl::Block, false),
        ext_tstamp: None,
        ext_nodeid: NodeIdType::DEFAULT,
        payload: Put {
            payload: vec![0u8; msg_size].into(),
            timestamp: None,
            encoding: Encoding::empty(),
            ext_sinfo: None,
            #[cfg(feature = "shared-memory")]
            ext_shm: None,
            ext_attachment: None,
            ext_unknown: vec![],
        }
        .into(),
    }
    .into();

    println!(
        "Sending message of {msg_size} bytes while defragmentation buffer size is {MSG_DEFRAG_BUF} bytes"
    );
    client_transport.schedule(message.clone()).unwrap();

    // Wait that the client transport has been closed
    ztimeout!(async {
        while client_transport.get_zid().is_ok() {
            tokio::time::sleep(SLEEP).await;
        }
    });

    // Wait on the router manager that the transport has been closed
    ztimeout!(async {
        while !router_manager.get_transports_unicast().await.is_empty() {
            tokio::time::sleep(SLEEP).await;
        }
    });

    // Stop the locators on the manager
    println!("Del locator: {endpoint}");
    ztimeout!(router_manager.del_listener(endpoint)).unwrap();

    // Wait a little bit
    ztimeout!(async {
        while !router_manager.get_listeners().await.is_empty() {
            tokio::time::sleep(SLEEP).await;
        }
    });

    tokio::time::sleep(SLEEP).await;

    ztimeout!(router_manager.close());
    ztimeout!(client_manager.close());

    // Wait a little bit
    tokio::time::sleep(SLEEP).await;
}

#[cfg(feature = "transport_tcp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn transport_unicast_defragmentation_tcp_only() {
    zenoh_util::init_log_from_env();

    // Define the locators
    let endpoint: EndPoint = format!("tcp/127.0.0.1:{}", 11000).parse().unwrap();
    // Define the reliability and congestion control
    let channel = [
        Channel {
            priority: Priority::DEFAULT,
            reliability: Reliability::Reliable,
        },
        Channel {
            priority: Priority::DEFAULT,
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
    for ch in channel.iter() {
        run(&endpoint, *ch, MSG_SIZE).await;
    }
}

#[cfg(feature = "transport_ws")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn transport_unicast_defragmentation_ws_only() {
    zenoh_util::init_log_from_env();

    // Define the locators
    let endpoint: EndPoint = format!("ws/127.0.0.1:{}", 11010).parse().unwrap();
    // Define the reliability and congestion control
    let channel = [
        Channel {
            priority: Priority::DEFAULT,
            reliability: Reliability::Reliable,
        },
        Channel {
            priority: Priority::DEFAULT,
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
    for ch in channel.iter() {
        run(&endpoint, *ch, MSG_SIZE).await;
    }
}

#[cfg(feature = "transport_unixpipe")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn transport_unicast_defragmentation_unixpipe_only() {
    zenoh_util::init_log_from_env();

    // Define the locators
    let endpoint: EndPoint = "unixpipe/transport_unicast_defragmentation_unixpipe_only"
        .parse()
        .unwrap();
    // Define the reliability and congestion control
    let channel = [
        Channel {
            priority: Priority::DEFAULT,
            reliability: Reliability::Reliable,
        },
        Channel {
            priority: Priority::DEFAULT,
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
    for ch in channel.iter() {
        run(&endpoint, *ch, MSG_SIZE).await;
    }
}
