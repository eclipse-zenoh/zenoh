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
use async_std::sync::{Arc, Barrier};
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

const MSG_COUNT: usize = 1_000;
const MSG_SIZE: usize = 1_024;
const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_millis(100);

macro_rules! ztimeout {
    ($f:expr) => {
        $f.timeout(TIMEOUT).await.unwrap()
    };
}

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
    fn handle_message(&self, _msg: ZenohMessage) -> ZResult<()> {
        self.count.fetch_add(1, Ordering::AcqRel);
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

async fn transport_concurrent(endpoint01: Vec<EndPoint>, endpoint02: Vec<EndPoint>) {
    /* [Peers] */
    let peer_id01 = ZenohId::new(1, [1_u8; ZenohId::MAX_SIZE]);
    let peer_id02 = ZenohId::new(1, [2_u8; ZenohId::MAX_SIZE]);

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
    let c_pid02 = peer_id02;
    let c_end01 = endpoint01.clone();
    let c_end02 = endpoint02.clone();
    let peer01_task = task::spawn(async move {
        // Add the endpoints on the first peer
        for e in c_end01.iter() {
            let res = ztimeout!(peer01_manager.add_listener(e.clone()));
            println!("[Transport Peer 01a] => Adding endpoint {:?}: {:?}", e, res);
            assert!(res.is_ok());
        }
        let locs = peer01_manager.get_listeners();
        println!(
            "[Transport Peer 01b] => Getting endpoints: {:?} {:?}",
            c_end01, locs
        );
        assert_eq!(c_end01.len(), locs.len());

        // Open the transport with the second peer
        for e in c_end02.iter() {
            let cc_barow = c_barow.clone();
            let cc_barod = c_barod.clone();
            let c_p01m = peer01_manager.clone();
            let c_end = e.clone();
            task::spawn(async move {
                println!("[Transport Peer 01c] => Waiting for opening transport");
                // Syncrhonize before opening the transports
                ztimeout!(cc_barow.wait());
                let res = ztimeout!(c_p01m.open_transport(c_end.clone()));
                println!(
                    "[Transport Peer 01d] => Opening transport with {:?}: {:?}",
                    c_end, res
                );
                assert!(res.is_ok());

                // Syncrhonize after opening the transports
                ztimeout!(cc_barod.wait());
            });
        }

        // Syncrhonize after opening the transports
        ztimeout!(c_barod.wait());
        println!("[Transport Peer 01e] => Waiting... OK");

        // Verify that the transport has been correctly open
        assert_eq!(peer01_manager.get_transports().len(), 1);
        let s02 = peer01_manager.get_transport(&c_pid02).unwrap();
        assert_eq!(
            s02.get_links().unwrap().len(),
            c_end01.len() + c_end02.len()
        );

        // Create the message to send
        let key = "/test02".into();
        let payload = ZBuf::from(vec![0_u8; MSG_SIZE]);
        let channel = Channel {
            priority: Priority::default(),
            reliability: Reliability::Reliable,
        };
        let congestion_control = CongestionControl::Block;
        let data_info = None;
        let routing_context = None;
        let reply_context = None;
        let attachment = None;

        let message = ZenohMessage::make_data(
            key,
            payload,
            channel,
            congestion_control,
            data_info,
            routing_context,
            reply_context,
            attachment,
        );

        // Synchronize wit the peer
        ztimeout!(c_barp.wait());
        println!("[Transport Peer 01f] => Waiting... OK");

        for i in 0..MSG_COUNT {
            println!("[Transport Peer 01g] Scheduling message {}", i);
            s02.schedule(message.clone()).unwrap();
        }
        println!("[Transport Peer 01g] => Scheduling OK");

        // Wait for the messages to arrive to the other side
        ztimeout!(async {
            while peer_sh02.get_count() != MSG_COUNT {
                task::sleep(SLEEP).await;
            }
        });

        // Synchronize wit the peer
        ztimeout!(c_barp.wait());

        println!("[Transport Peer 01h] => Closing {:?}...", s02);
        let res = ztimeout!(s02.close());
        println!("[Transport Peer 01l] => Closing {:?}: {:?}", s02, res);
        assert!(res.is_ok());

        // Close the transport
        ztimeout!(peer01_manager.close());
    });

    // Peer02
    let c_barp = barrier_peer;
    let c_barow = barrier_open_wait.clone();
    let c_barod = barrier_open_done.clone();
    let c_pid01 = peer_id01;
    let c_end01 = endpoint01.clone();
    let c_end02 = endpoint02.clone();
    let peer02_task = task::spawn(async move {
        // Add the endpoints on the first peer
        for e in c_end02.iter() {
            let res = ztimeout!(peer02_manager.add_listener(e.clone()));
            println!("[Transport Peer 02a] => Adding endpoint {:?}: {:?}", e, res);
            assert!(res.is_ok());
        }
        let locs = peer02_manager.get_listeners();
        println!(
            "[Transport Peer 02b] => Getting endpoints: {:?} {:?}",
            c_end02, locs
        );
        assert_eq!(c_end02.len(), locs.len());

        // Open the transport with the first peer
        for e in c_end01.iter() {
            let cc_barow = c_barow.clone();
            let cc_barod = c_barod.clone();
            let c_p02m = peer02_manager.clone();
            let c_end = e.clone();
            task::spawn(async move {
                println!("[Transport Peer 02c] => Waiting for opening transport");
                // Syncrhonize before opening the transports
                ztimeout!(cc_barow.wait());

                let res = ztimeout!(c_p02m.open_transport(c_end.clone()));
                println!(
                    "[Transport Peer 02d] => Opening transport with {:?}: {:?}",
                    c_end, res
                );
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
            peer02_manager.get_transports()
        );
        assert_eq!(peer02_manager.get_transports().len(), 1);
        let s01 = peer02_manager.get_transport(&c_pid01).unwrap();
        assert_eq!(
            s01.get_links().unwrap().len(),
            c_end01.len() + c_end02.len()
        );

        // Create the message to send
        let key = "/test02".into();
        let payload = ZBuf::from(vec![0_u8; MSG_SIZE]);
        let channel = Channel {
            priority: Priority::default(),
            reliability: Reliability::Reliable,
        };
        let congestion_control = CongestionControl::Block;
        let data_info = None;
        let routing_context = None;
        let reply_context = None;
        let attachment = None;

        let message = ZenohMessage::make_data(
            key,
            payload,
            channel,
            congestion_control,
            data_info,
            routing_context,
            reply_context,
            attachment,
        );

        // Synchronize wit the peer
        ztimeout!(c_barp.wait());
        println!("[Transport Peer 02f] => Waiting... OK");

        for i in 0..MSG_COUNT {
            println!("[Transport Peer 02g] Scheduling message {}", i);
            s01.schedule(message.clone()).unwrap();
        }
        println!("[Transport Peer 02g] => Scheduling OK");

        // Wait for the messages to arrive to the other side
        ztimeout!(async {
            while peer_sh01.get_count() != MSG_COUNT {
                task::sleep(SLEEP).await;
            }
        });

        // Synchronize wit the peer
        ztimeout!(c_barp.wait());

        println!("[Transport Peer 02h] => Closing {:?}...", s01);
        let res = ztimeout!(s01.close());
        println!("[Transport Peer 02l] => Closing {:?}: {:?}", s01, res);
        assert!(res.is_ok());

        // Close the transport
        ztimeout!(peer02_manager.close());
    });

    println!("[Transport Current 01] => Starting...");
    peer01_task.join(peer02_task).await;
    println!("[Transport Current 02] => ...Stopped");

    // Wait a little bit
    task::sleep(SLEEP).await;
}

#[cfg(feature = "transport_tcp")]
#[test]
fn transport_tcp_concurrent() {
    task::block_on(async {
        zasync_executor_init!();
    });

    let endpoint01: Vec<EndPoint> = vec![
        "tcp/127.0.0.1:7447".parse().unwrap(),
        "tcp/127.0.0.1:7448".parse().unwrap(),
        "tcp/127.0.0.1:7449".parse().unwrap(),
        "tcp/127.0.0.1:7450".parse().unwrap(),
        "tcp/127.0.0.1:7451".parse().unwrap(),
        "tcp/127.0.0.1:7452".parse().unwrap(),
        "tcp/127.0.0.1:7453".parse().unwrap(),
        "tcp/127.0.0.1:7454".parse().unwrap(),
    ];
    let endpoint02: Vec<EndPoint> = vec![
        "tcp/127.0.0.1:7455".parse().unwrap(),
        "tcp/127.0.0.1:7456".parse().unwrap(),
        "tcp/127.0.0.1:7457".parse().unwrap(),
        "tcp/127.0.0.1:7458".parse().unwrap(),
        "tcp/127.0.0.1:7459".parse().unwrap(),
        "tcp/127.0.0.1:7460".parse().unwrap(),
        "tcp/127.0.0.1:7461".parse().unwrap(),
        "tcp/127.0.0.1:7462".parse().unwrap(),
    ];

    task::block_on(async {
        transport_concurrent(endpoint01, endpoint02).await;
    });
}
