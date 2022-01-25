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

// Restricting to macos by default because of no IPv6 support
// on GitHub CI actions on Linux and Windows.
#[cfg(target_os = "macos")]
mod tests {
    use async_std::prelude::*;
    use async_std::sync::Arc;
    use async_std::task;
    use std::any::Any;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use zenoh::net::link::EndPoint;
    use zenoh::net::link::Link;
    use zenoh::net::protocol::core::{
        Channel, CongestionControl, Priority, Reliability, WhatAmI, ZenohId,
    };
    use zenoh::net::protocol::io::ZBuf;
    use zenoh::net::protocol::message::ZenohMessage;
    use zenoh::net::transport::{
        TransportEventHandler, TransportManager, TransportMulticast,
        TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler, TransportUnicast,
    };
    use zenoh_util::core::Result as ZResult;
    use zenoh_util::properties::config::*;
    use zenoh_util::zasync_executor_init;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const SLEEP_COUNT: Duration = Duration::from_millis(10);

    const MSG_COUNT: usize = 1_000;
    const MSG_SIZE_NOFRAG: [usize; 1] = [1_024];

    macro_rules! ztimeout {
        ($f:expr) => {
            $f.timeout(TIMEOUT).await.unwrap()
        };
    }

    // Transport Handler for the peer02
    struct SHPeer {
        count: Arc<AtomicUsize>,
    }

    impl Default for SHPeer {
        fn default() -> Self {
            Self {
                count: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    impl SHPeer {
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
            panic!();
        }

        fn new_multicast(
            &self,
            _transport: TransportMulticast,
        ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
            let arc = Arc::new(SCPeer::new(self.count.clone()));
            Ok(arc)
        }
    }

    // Transport Callback for the peer02
    pub struct SCPeer {
        count: Arc<AtomicUsize>,
    }

    impl SCPeer {
        pub fn new(count: Arc<AtomicUsize>) -> Self {
            Self { count }
        }
    }

    impl TransportMulticastEventHandler for SCPeer {
        fn new_peer(&self, _peer: TransportPeer) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
            Ok(Arc::new(SCPeer {
                count: self.count.clone(),
            }))
        }
        fn closing(&self) {}
        fn closed(&self) {}

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    impl TransportPeerEventHandler for SCPeer {
        fn handle_message(&self, _msg: ZenohMessage) -> ZResult<()> {
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

    struct TransportMulticastPeer {
        manager: TransportManager,
        handler: Arc<SHPeer>,
        transport: TransportMulticast,
    }

    async fn open_transport(
        endpoint: &EndPoint,
    ) -> (TransportMulticastPeer, TransportMulticastPeer) {
        // Define peer01 and peer02 IDs
        let peer01_id = ZenohId::new(1, [0_u8; ZenohId::MAX_SIZE]);
        let peer02_id = ZenohId::new(1, [1_u8; ZenohId::MAX_SIZE]);

        // Create the peer01 transport manager
        let peer01_handler = Arc::new(SHPeer::default());
        let peer01_manager = TransportManager::builder()
            .zid(peer01_id)
            .whatami(WhatAmI::Peer)
            .build(peer01_handler.clone())
            .unwrap();

        // Create the peer02 transport manager
        let peer02_handler = Arc::new(SHPeer::default());
        let peer02_manager = TransportManager::builder()
            .whatami(WhatAmI::Peer)
            .zid(peer02_id)
            .build(peer02_handler.clone())
            .unwrap();

        // Create an empty transport with the peer01
        // Open transport -> This should be accepted
        println!("Opening transport with {}", endpoint);
        let _ = ztimeout!(peer01_manager.open_transport_multicast(endpoint.clone())).unwrap();
        assert!(peer01_manager
            .get_transport_multicast(&endpoint.locator)
            .is_some());
        println!("\t{:?}", peer01_manager.get_transports_multicast());

        println!("Opening transport with {}", endpoint);
        let _ = ztimeout!(peer02_manager.open_transport_multicast(endpoint.clone())).unwrap();
        assert!(peer02_manager
            .get_transport_multicast(&endpoint.locator)
            .is_some());
        println!("\t{:?}", peer02_manager.get_transports_multicast());

        // Wait to for peer 01 and 02 to join each other
        let peer01_transport = peer01_manager
            .get_transport_multicast(&endpoint.locator)
            .unwrap();
        ztimeout!(async {
            while peer01_transport.get_peers().unwrap().is_empty() {
                task::sleep(SLEEP_COUNT).await;
            }
        });

        let peer02_transport = peer02_manager
            .get_transport_multicast(&endpoint.locator)
            .unwrap();
        ztimeout!(async {
            while peer02_transport.get_peers().unwrap().is_empty() {
                task::sleep(SLEEP_COUNT).await;
            }
        });

        (
            TransportMulticastPeer {
                manager: peer01_manager,
                handler: peer01_handler,
                transport: peer01_transport,
            },
            TransportMulticastPeer {
                manager: peer02_manager,
                handler: peer02_handler,
                transport: peer02_transport,
            },
        )
    }

    async fn close_transport(
        peer01: TransportMulticastPeer,
        peer02: TransportMulticastPeer,
        endpoint: &EndPoint,
    ) {
        // Close the peer01 transport
        println!("Closing transport with {}", endpoint);
        let _ = ztimeout!(peer01.transport.close()).unwrap();
        assert!(peer01.manager.get_transports_multicast().is_empty());
        assert!(peer02.transport.get_peers().unwrap().is_empty());

        // Close the peer02 transport
        println!("Closing transport with {}", endpoint);
        let _ = ztimeout!(peer02.transport.close()).unwrap();
        assert!(peer02.manager.get_transports_multicast().is_empty());

        // Wait a little bit
        task::sleep(SLEEP).await;
    }

    async fn test_transport(
        peer01: &TransportMulticastPeer,
        peer02: &TransportMulticastPeer,
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
            peer01.transport.schedule(message.clone()).unwrap();
        }

        match channel.reliability {
            Reliability::Reliable => {
                ztimeout!(async {
                    while peer02.handler.get_count() != MSG_COUNT {
                        task::sleep(SLEEP_COUNT).await;
                    }
                });
            }
            Reliability::BestEffort => {
                ztimeout!(async {
                    while peer02.handler.get_count() == 0 {
                        task::sleep(SLEEP_COUNT).await;
                    }
                });
            }
        };

        // Wait a little bit
        task::sleep(SLEEP).await;
    }

    async fn run_single(endpoint: &EndPoint, channel: Channel, msg_size: usize) {
        let (peer01, peer02) = open_transport(endpoint).await;
        test_transport(&peer01, &peer02, channel, msg_size).await;

        #[cfg(feature = "stats")]
        {
            let stats = peer01.transport.get_stats().unwrap();
            println!("\tPeer 01: {:?}", stats);
            let stats = peer02.transport.get_stats().unwrap();
            println!("\tPeer 02: {:?}", stats);
        }

        close_transport(peer01, peer02, endpoint).await;
    }

    async fn run(endpoints: &[EndPoint], channel: &[Channel], msg_size: &[usize]) {
        for e in endpoints.iter() {
            for ch in channel.iter() {
                for ms in msg_size.iter() {
                    run_single(e, *ch, *ms).await;
                }
            }
        }
    }

    #[cfg(feature = "transport_udp")]
    #[test]
    fn transport_multicast_udp_only() {
        env_logger::init();

        task::block_on(async {
            zasync_executor_init!();
        });

        // Define the locator
        let endpoints: Vec<EndPoint> = vec![
            format!("udp/{}", ZN_MULTICAST_IPV4_ADDRESS_DEFAULT)
                .parse()
                .unwrap(),
            // Disabling by default because of no IPv6 support
            // on GitHub CI actions.
            // format!("udp/{}", ZN_MULTICAST_IPV6_ADDRESS_DEFAULT)
            //     .parse()
            //     .unwrap(),
        ];
        // Define the reliability and congestion control
        let channel = [
            Channel {
                priority: Priority::default(),
                reliability: Reliability::BestEffort,
            },
            Channel {
                priority: Priority::RealTime,
                reliability: Reliability::BestEffort,
            },
        ];
        // Run
        task::block_on(run(&endpoints, &channel, &MSG_SIZE_NOFRAG));
    }
}
