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
#[cfg(feature = "zero-copy")]
mod tests {
    use async_std::prelude::*;
    use async_std::task;
    use std::any::Any;
    use std::collections::HashSet;
    use std::iter::FromIterator;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use zenoh::net::link::{EndPoint, Link};
    use zenoh::net::protocol::core::{whatami, Channel, PeerId, Priority, Reliability, ResKey};
    use zenoh::net::protocol::io::{SharedMemoryManager, ZBuf};
    use zenoh::net::protocol::proto::{Data, ZenohBody, ZenohMessage};
    use zenoh::net::transport::unicast::establishment::authenticator::SharedMemoryAuthenticator;
    use zenoh::net::transport::{
        TransportEventHandler, TransportManager, TransportManagerConfig,
        TransportManagerConfigUnicast, TransportMulticast, TransportMulticastEventHandler,
        TransportPeer, TransportPeerEventHandler, TransportUnicast,
    };
    use zenoh::net::CongestionControl;
    use zenoh_util::core::ZResult;
    use zenoh_util::zasync_executor_init;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const USLEEP: Duration = Duration::from_micros(100);

    const MSG_COUNT: usize = 1_000;
    const MSG_SIZE: usize = 1_024;

    // Transport Handler for the router
    struct SHPeer {
        count: Arc<AtomicUsize>,
        is_shm: bool,
    }

    impl SHPeer {
        fn new(is_shm: bool) -> Self {
            Self {
                count: Arc::new(AtomicUsize::new(0)),
                is_shm,
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
            let arc = Arc::new(SCPeer::new(self.count.clone(), self.is_shm));
            Ok(arc)
        }

        fn new_multicast(
            &self,
            _transport: TransportMulticast,
        ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
            panic!();
        }
    }

    // Transport Callback for the peer
    pub struct SCPeer {
        count: Arc<AtomicUsize>,
        is_shm: bool,
    }

    impl SCPeer {
        pub fn new(count: Arc<AtomicUsize>, is_shm: bool) -> Self {
            Self { count, is_shm }
        }
    }

    impl TransportPeerEventHandler for SCPeer {
        fn handle_message(&self, message: ZenohMessage) -> ZResult<()> {
            if self.is_shm {
                print!("s");
            } else {
                print!("n");
            }
            let payload = match message.body {
                ZenohBody::Data(Data { payload, .. }) => payload.contiguous(),
                _ => panic!("Unsolicited message"),
            };
            assert_eq!(payload.len(), MSG_SIZE);

            let mut count_bytes = [0u8; 8];
            count_bytes.copy_from_slice(&payload[0..8]);
            let msg_count = u64::from_le_bytes(count_bytes) as usize;
            let sex_count = self.count.fetch_add(1, Ordering::SeqCst);
            assert_eq!(msg_count, sex_count);
            print!("{} ", msg_count);

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

    async fn run(endpoint: &EndPoint) {
        // Define client and router IDs
        let peer_shm01 = PeerId::new(1, [0u8; PeerId::MAX_SIZE]);
        let peer_shm02 = PeerId::new(1, [1u8; PeerId::MAX_SIZE]);
        let peer_net01 = PeerId::new(1, [2u8; PeerId::MAX_SIZE]);

        // Create the SharedMemoryManager
        let mut shm01 = SharedMemoryManager::new("peer_shm01".to_string(), 2 * MSG_SIZE).unwrap();

        // Create a peer manager with zero-copy authenticator enabled
        let peer_shm01_handler = Arc::new(SHPeer::new(false));
        let config = TransportManagerConfig::builder()
            .whatami(whatami::PEER)
            .pid(peer_shm01)
            .unicast(
                TransportManagerConfigUnicast::builder()
                    .peer_authenticator(HashSet::from_iter(vec![
                        SharedMemoryAuthenticator::new().into()
                    ]))
                    .build(),
            )
            .build(peer_shm01_handler.clone());
        let peer_shm01_manager = TransportManager::new(config);

        // Create a peer manager with zero-copy authenticator enabled
        let peer_shm02_handler = Arc::new(SHPeer::new(true));
        let config = TransportManagerConfig::builder()
            .whatami(whatami::PEER)
            .pid(peer_shm02)
            .unicast(
                TransportManagerConfigUnicast::builder()
                    .peer_authenticator(HashSet::from_iter(vec![
                        SharedMemoryAuthenticator::new().into()
                    ]))
                    .build(),
            )
            .build(peer_shm02_handler.clone());
        let peer_shm02_manager = TransportManager::new(config);

        // Create a peer manager with zero-copy authenticator disabled
        let peer_net01_handler = Arc::new(SHPeer::new(false));
        let config = TransportManagerConfig::builder()
            .whatami(whatami::PEER)
            .pid(peer_net01)
            .build(peer_net01_handler.clone());
        let peer_net01_manager = TransportManager::new(config);

        // Create the listener on the peer
        println!("\nTransport SHM [1a]");
        let _ = peer_shm01_manager
            .add_listener(endpoint.clone())
            .timeout(TIMEOUT)
            .await
            .unwrap()
            .unwrap();

        // Create a transport with the peer
        println!("Transport SHM [1b]");
        let _ = peer_shm02_manager
            .open_transport(endpoint.clone())
            .timeout(TIMEOUT)
            .await
            .unwrap()
            .unwrap();

        // Create a transport with the peer
        println!("Transport SHM [1c]");
        let _ = peer_net01_manager
            .open_transport(endpoint.clone())
            .timeout(TIMEOUT)
            .await
            .unwrap()
            .unwrap();

        // Retrieve the transports
        println!("Transport SHM [2a]");
        let peer_shm02_transport = peer_shm01_manager.get_transport(&peer_shm02).unwrap();

        println!("Transport SHM [2b]");
        let peer_net01_transport = peer_shm01_manager.get_transport(&peer_net01).unwrap();

        // Send the message
        println!("Transport SHM [3a]");
        // The msg count
        for (msg_count, _) in (0..MSG_COUNT).enumerate() {
            // Create the message to send
            let mut sbuf = async {
                loop {
                    match shm01.alloc(MSG_SIZE) {
                        Some(sbuf) => break sbuf,
                        None => task::sleep(USLEEP).await,
                    }
                }
            }
            .timeout(TIMEOUT)
            .await
            .unwrap();

            let bs = unsafe { sbuf.as_mut_slice() };
            bs[0..8].copy_from_slice(&msg_count.to_le_bytes());

            let key = ResKey::RName("/test".to_string());
            let payload: ZBuf = sbuf.into();
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

            peer_shm02_transport.schedule(message.clone()).unwrap();
        }

        // Wait a little bit
        task::sleep(SLEEP).await;

        // Wait for the messages to arrive to the other side
        println!("\nTransport SHM [3b]");
        let count = async {
            while peer_shm02_handler.get_count() != MSG_COUNT {
                task::sleep(SLEEP).await;
            }
        };
        let _ = count.timeout(TIMEOUT).await.unwrap();

        // Send the message
        println!("Transport SHM [4a]");
        // The msg count
        for (msg_count, _) in (0..MSG_COUNT).enumerate() {
            // Create the message to send
            let mut sbuf = async {
                loop {
                    match shm01.alloc(MSG_SIZE) {
                        Some(sbuf) => break sbuf,
                        None => task::sleep(USLEEP).await,
                    }
                }
            }
            .timeout(TIMEOUT)
            .await
            .unwrap();
            let bs = unsafe { sbuf.as_mut_slice() };
            bs[0..8].copy_from_slice(&msg_count.to_le_bytes());

            let key = ResKey::RName("/test".to_string());
            let payload: ZBuf = sbuf.into();
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

            peer_net01_transport.schedule(message.clone()).unwrap();
        }

        // Wait a little bit
        task::sleep(SLEEP).await;

        // Wait for the messages to arrive to the other side
        println!("\nTransport SHM [4b]");
        let count = async {
            while peer_net01_handler.get_count() != MSG_COUNT {
                task::sleep(SLEEP).await;
            }
        };
        let _ = count.timeout(TIMEOUT).await.unwrap();

        // Wait a little bit
        task::sleep(SLEEP).await;

        // Close the transports
        println!("Transport SHM [5a]");
        let _ = peer_shm02_transport
            .close()
            .timeout(TIMEOUT)
            .await
            .unwrap()
            .unwrap();

        println!("Transport SHM [5b]");
        let _ = peer_net01_transport
            .close()
            .timeout(TIMEOUT)
            .await
            .unwrap()
            .unwrap();

        // Wait a little bit
        task::sleep(SLEEP).await;

        // Delete the listener
        println!("Transport SHM [6a]");
        let _ = peer_shm01_manager
            .del_listener(endpoint)
            .timeout(TIMEOUT)
            .await
            .unwrap()
            .unwrap();

        // Wait a little bit
        task::sleep(SLEEP).await;
    }

    #[cfg(all(feature = "transport_tcp", feature = "zero-copy"))]
    #[test]
    fn transport_tcp_shm() {
        env_logger::init();
        task::block_on(async {
            zasync_executor_init!();
        });

        let endpoint: EndPoint = "tcp/127.0.0.1:12447".parse().unwrap();
        task::block_on(run(&endpoint));
    }
}
