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
use std::{
    any::Any,
    convert::TryFrom,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use tokio::sync::Barrier;
use zenoh_core::ztimeout;
use zenoh_link::Link;
use zenoh_protocol::{
    core::{CongestionControl, EndPoint, Priority, WhatAmI, ZenohIdProto},
    network::{
        push::{ext::QoSType, Push},
        NetworkMessage, NetworkMessageMut,
    },
};
use zenoh_result::ZResult;
use zenoh_transport::{
    multicast::TransportMulticast, unicast::TransportUnicast, TransportEventHandler,
    TransportManager, TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler,
};

const MSG_COUNT: usize = 1_000;
const MSG_SIZE: usize = 1_024;
const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_millis(100);

// Transport Handler for the router
struct SHPeer {
    count: Arc<AtomicUsize>,
}

impl SHPeer {
    fn new() -> Self {
        Self {
            count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn get_count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}

impl TransportEventHandler for SHPeer {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        let mh = Arc::new(MHPeer::new(self.count.clone()));
        Ok(mh)
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        panic!();
    }
}

struct MHPeer {
    count: Arc<AtomicUsize>,
}

impl MHPeer {
    fn new(count: Arc<AtomicUsize>) -> Self {
        Self { count }
    }
}

impl TransportPeerEventHandler for MHPeer {
    fn handle_message(&self, _msg: NetworkMessageMut) -> ZResult<()> {
        self.count.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    fn new_link(&self, _link: Link) {}
    fn del_link(&self, _link: Link) {}
    fn closed(&self) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}

async fn transport_concurrent(endpoint01: Vec<EndPoint>, endpoint02: Vec<EndPoint>) {
    /* [Peers] */
    let peer_id01 = ZenohIdProto::try_from([2]).unwrap();
    let peer_id02 = ZenohIdProto::try_from([3]).unwrap();

    // Create the peer01 transport manager
    let peer_sh01 = Arc::new(SHPeer::new());
    let unicast01 = TransportManager::config_unicast().max_links(endpoint02.len());
    let peer01_manager = TransportManager::builder()
        .whatami(WhatAmI::Peer)
        .zid(peer_id01)
        .unicast(unicast01)
        .build(peer_sh01.clone())
        .unwrap();

    // Create the peer01 transport manager
    let peer_sh02 = Arc::new(SHPeer::new());
    let unicast02 = TransportManager::config_unicast().max_links(endpoint01.len());
    let peer02_manager = TransportManager::builder()
        .whatami(WhatAmI::Peer)
        .zid(peer_id02)
        .unicast(unicast02)
        .build(peer_sh02.clone())
        .unwrap();

    // Barrier to synchronize the two tasks
    let barrier_peer = Arc::new(Barrier::new(2));
    let barrier_open_wait = Arc::new(Barrier::new(endpoint01.len() + endpoint02.len()));
    let barrier_open_done = Arc::new(Barrier::new(endpoint01.len() + endpoint02.len() + 2));

    // Peer01
    let c_barp = barrier_peer.clone();
    let c_barow = barrier_open_wait.clone();
    let c_barod = barrier_open_done.clone();
    let c_zid02 = peer_id02;
    let c_end01 = endpoint01.clone();
    let c_end02 = endpoint02.clone();
    let peer01_task = tokio::task::spawn(async move {
        // Add the endpoints on the first peer
        for e in c_end01.iter() {
            let res = ztimeout!(peer01_manager.add_listener(e.clone()));
            println!("[Transport Peer 01a] => Adding endpoint {e:?}: {res:?}");
            assert!(res.is_ok());
        }
        let locs = ztimeout!(peer01_manager.get_listeners());
        println!("[Transport Peer 01b] => Getting endpoints: {c_end01:?} {locs:?}");
        assert_eq!(c_end01.len(), locs.len());

        // Open the transport with the second peer
        for e in c_end02.iter() {
            let cc_barow = c_barow.clone();
            let cc_barod = c_barod.clone();
            let c_p01m = peer01_manager.clone();
            let c_end = e.clone();
            tokio::task::spawn(async move {
                println!("[Transport Peer 01c] => Waiting for opening transport");
                // Syncrhonize before opening the transports
                ztimeout!(cc_barow.wait());
                let res = ztimeout!(c_p01m.open_transport_unicast(c_end.clone()));
                println!("[Transport Peer 01d] => Opening transport with {c_end:?}: {res:?}");
                assert!(res.is_ok());

                // Syncrhonize after opening the transports
                ztimeout!(cc_barod.wait());
            });
        }

        // Syncrhonize after opening the transports
        ztimeout!(c_barod.wait());
        println!("[Transport Peer 01e] => Waiting... OK");

        // Verify that the transport has been correctly open
        assert_eq!(ztimeout!(peer01_manager.get_transports_unicast()).len(), 1);
        let s02 = ztimeout!(peer01_manager.get_transport_unicast(&c_zid02)).unwrap();
        assert_eq!(
            s02.get_links().unwrap().len(),
            c_end01.len() + c_end02.len()
        );

        // Create the message to send
        let message = NetworkMessage::from(Push {
            wire_expr: "test".into(),
            ext_qos: QoSType::new(Priority::DEFAULT, CongestionControl::Block, false),
            ..Push::from(vec![0u8; MSG_SIZE])
        });

        // Synchronize wit the peer
        ztimeout!(c_barp.wait());
        println!("[Transport Peer 01f] => Waiting... OK");

        for i in 0..MSG_COUNT {
            println!("[Transport Peer 01g] Scheduling message {i}");
            s02.schedule(message.clone().as_mut()).unwrap();
        }
        println!("[Transport Peer 01g] => Scheduling OK");

        // Wait for the messages to arrive to the other side
        ztimeout!(async {
            while peer_sh02.get_count() != MSG_COUNT {
                tokio::time::sleep(SLEEP).await;
            }
        });

        // Synchronize wit the peer
        ztimeout!(c_barp.wait());

        println!("[Transport Peer 01h] => Closing {s02:?}...");
        let res = ztimeout!(s02.close());
        println!("[Transport Peer 01l] => Closing {s02:?}: {res:?}");
        assert!(res.is_ok());

        // Close the transport
        ztimeout!(peer01_manager.close());
    });

    // Peer02
    let c_barp = barrier_peer;
    let c_barow = barrier_open_wait.clone();
    let c_barod = barrier_open_done.clone();
    let c_zid01 = peer_id01;
    let c_end01 = endpoint01.clone();
    let c_end02 = endpoint02.clone();
    let peer02_task = tokio::task::spawn(async move {
        // Add the endpoints on the first peer
        for e in c_end02.iter() {
            let res = ztimeout!(peer02_manager.add_listener(e.clone()));
            println!("[Transport Peer 02a] => Adding endpoint {e:?}: {res:?}");
            assert!(res.is_ok());
        }
        let locs = ztimeout!(peer02_manager.get_listeners());
        println!("[Transport Peer 02b] => Getting endpoints: {c_end02:?} {locs:?}");
        assert_eq!(c_end02.len(), locs.len());

        // Open the transport with the first peer
        for e in c_end01.iter() {
            let cc_barow = c_barow.clone();
            let cc_barod = c_barod.clone();
            let c_p02m = peer02_manager.clone();
            let c_end = e.clone();
            tokio::task::spawn(async move {
                println!("[Transport Peer 02c] => Waiting for opening transport");
                // Syncrhonize before opening the transports
                ztimeout!(cc_barow.wait());

                let res = ztimeout!(c_p02m.open_transport_unicast(c_end.clone()));
                println!("[Transport Peer 02d] => Opening transport with {c_end:?}: {res:?}");
                assert!(res.is_ok());

                // Syncrhonize after opening the transports
                ztimeout!(cc_barod.wait());
            });
        }

        // Syncrhonize after opening the transports
        ztimeout!(c_barod.wait());

        // Verify that the transport has been correctly open
        println!(
            "[Transport Peer 02e] => Transports: {:?}",
            ztimeout!(peer02_manager.get_transports_unicast())
        );
        assert_eq!(ztimeout!(peer02_manager.get_transports_unicast()).len(), 1);
        let s01 = ztimeout!(peer02_manager.get_transport_unicast(&c_zid01)).unwrap();
        assert_eq!(
            s01.get_links().unwrap().len(),
            c_end01.len() + c_end02.len()
        );

        // Create the message to send
        let message = NetworkMessage::from(Push {
            wire_expr: "test".into(),
            ext_qos: QoSType::new(Priority::DEFAULT, CongestionControl::Block, false),
            ..Push::from(vec![0u8; MSG_SIZE])
        });

        // Synchronize wit the peer
        ztimeout!(c_barp.wait());
        println!("[Transport Peer 02f] => Waiting... OK");

        for i in 0..MSG_COUNT {
            println!("[Transport Peer 02g] Scheduling message {i}");
            s01.schedule(message.clone().as_mut()).unwrap();
        }
        println!("[Transport Peer 02g] => Scheduling OK");

        // Wait for the messages to arrive to the other side
        ztimeout!(async {
            while peer_sh01.get_count() != MSG_COUNT {
                tokio::time::sleep(SLEEP).await;
            }
        });

        // Synchronize wit the peer
        ztimeout!(c_barp.wait());

        println!("[Transport Peer 02h] => Closing {s01:?}...");
        let res = ztimeout!(s01.close());
        println!("[Transport Peer 02l] => Closing {s01:?}: {res:?}");
        assert!(res.is_ok());

        // Close the transport
        ztimeout!(peer02_manager.close());
    });

    println!("[Transport Current 01] => Starting...");
    let _ = tokio::join!(peer01_task, peer02_task);
    println!("[Transport Current 02] => ...Stopped");

    // Wait a little bit
    tokio::time::sleep(SLEEP).await;
}

#[cfg(feature = "transport_tcp")]
#[tokio::test]
async fn transport_tcp_concurrent() {
    zenoh_util::init_log_from_env_or("error");

    let endpoint01: Vec<EndPoint> = vec![
        format!("tcp/127.0.0.1:{}", 9000).parse().unwrap(),
        format!("tcp/127.0.0.1:{}", 9001).parse().unwrap(),
        format!("tcp/127.0.0.1:{}", 9002).parse().unwrap(),
        format!("tcp/127.0.0.1:{}", 9003).parse().unwrap(),
        format!("tcp/127.0.0.1:{}", 9004).parse().unwrap(),
        format!("tcp/127.0.0.1:{}", 9005).parse().unwrap(),
        format!("tcp/127.0.0.1:{}", 9006).parse().unwrap(),
        format!("tcp/127.0.0.1:{}", 9007).parse().unwrap(),
    ];
    let endpoint02: Vec<EndPoint> = vec![
        format!("tcp/127.0.0.1:{}", 9010).parse().unwrap(),
        format!("tcp/127.0.0.1:{}", 9011).parse().unwrap(),
        format!("tcp/127.0.0.1:{}", 9012).parse().unwrap(),
        format!("tcp/127.0.0.1:{}", 9013).parse().unwrap(),
        format!("tcp/127.0.0.1:{}", 9014).parse().unwrap(),
        format!("tcp/127.0.0.1:{}", 9015).parse().unwrap(),
        format!("tcp/127.0.0.1:{}", 9016).parse().unwrap(),
        format!("tcp/127.0.0.1:{}", 9017).parse().unwrap(),
    ];

    transport_concurrent(endpoint01, endpoint02).await;
}

#[cfg(feature = "transport_ws")]
#[tokio::test]
#[ignore]
async fn transport_ws_concurrent() {
    zenoh_util::init_log_from_env_or("error");

    let endpoint01: Vec<EndPoint> = vec![
        format!("ws/127.0.0.1:{}", 9020).parse().unwrap(),
        format!("ws/127.0.0.1:{}", 9021).parse().unwrap(),
        format!("ws/127.0.0.1:{}", 9022).parse().unwrap(),
        format!("ws/127.0.0.1:{}", 9023).parse().unwrap(),
        format!("ws/127.0.0.1:{}", 9024).parse().unwrap(),
        format!("ws/127.0.0.1:{}", 9025).parse().unwrap(),
        format!("ws/127.0.0.1:{}", 9026).parse().unwrap(),
        format!("ws/127.0.0.1:{}", 9027).parse().unwrap(),
    ];
    let endpoint02: Vec<EndPoint> = vec![
        format!("ws/127.0.0.1:{}", 9030).parse().unwrap(),
        format!("ws/127.0.0.1:{}", 9031).parse().unwrap(),
        format!("ws/127.0.0.1:{}", 9032).parse().unwrap(),
        format!("ws/127.0.0.1:{}", 9033).parse().unwrap(),
        format!("ws/127.0.0.1:{}", 9034).parse().unwrap(),
        format!("ws/127.0.0.1:{}", 9035).parse().unwrap(),
        format!("ws/127.0.0.1:{}", 9036).parse().unwrap(),
        format!("ws/127.0.0.1:{}", 9037).parse().unwrap(),
    ];

    transport_concurrent(endpoint01, endpoint02).await;
}

#[cfg(feature = "transport_unixpipe")]
#[tokio::test]
#[ignore]
async fn transport_unixpipe_concurrent() {
    zenoh_util::init_log_from_env_or("error");

    let endpoint01: Vec<EndPoint> = vec![
        "unixpipe/transport_unixpipe_concurrent".parse().unwrap(),
        "unixpipe/transport_unixpipe_concurrent2".parse().unwrap(),
        "unixpipe/transport_unixpipe_concurrent3".parse().unwrap(),
        "unixpipe/transport_unixpipe_concurrent4".parse().unwrap(),
        "unixpipe/transport_unixpipe_concurrent5".parse().unwrap(),
        "unixpipe/transport_unixpipe_concurrent6".parse().unwrap(),
        "unixpipe/transport_unixpipe_concurrent7".parse().unwrap(),
        "unixpipe/transport_unixpipe_concurrent8".parse().unwrap(),
    ];
    let endpoint02: Vec<EndPoint> = vec![
        "unixpipe/transport_unixpipe_concurrent9".parse().unwrap(),
        "unixpipe/transport_unixpipe_concurrent10".parse().unwrap(),
        "unixpipe/transport_unixpipe_concurrent11".parse().unwrap(),
        "unixpipe/transport_unixpipe_concurrent12".parse().unwrap(),
        "unixpipe/transport_unixpipe_concurrent13".parse().unwrap(),
        "unixpipe/transport_unixpipe_concurrent14".parse().unwrap(),
        "unixpipe/transport_unixpipe_concurrent15".parse().unwrap(),
        "unixpipe/transport_unixpipe_concurrent16".parse().unwrap(),
    ];

    transport_concurrent(endpoint01, endpoint02).await;
}
