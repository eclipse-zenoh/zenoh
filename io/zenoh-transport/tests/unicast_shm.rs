//
// Copyright (c) 2022 ZettaScale Technology
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
#[cfg(feature = "shared-memory")]
mod tests {
    use async_std::{prelude::FutureExt, task};
    use std::{
        any::Any,
        collections::HashSet,
        convert::TryFrom,
        iter::FromIterator,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        time::Duration,
    };
    use zenoh_buffers::{SplitBuffer, ZBuf};
    use zenoh_core::zasync_executor_init;
    use zenoh_link::Link;
    use zenoh_protocol::{
        core::{Channel, CongestionControl, EndPoint, Priority, Reliability, WhatAmI, ZenohId},
        zenoh::{Data, ZenohBody, ZenohMessage},
    };
    use zenoh_result::ZResult;
    use zenoh_shm::SharedMemoryManager;
    use zenoh_transport::{
        unicast::establishment::authenticator::SharedMemoryAuthenticator, TransportEventHandler,
        TransportManager, TransportMulticast, TransportMulticastEventHandler, TransportPeer,
        TransportPeerEventHandler, TransportUnicast,
    };

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const USLEEP: Duration = Duration::from_micros(100);

    const MSG_COUNT: usize = 1_000;
    const MSG_SIZE: usize = 1_024;

    macro_rules! ztimeout {
        ($f:expr) => {
            $f.timeout(TIMEOUT).await.unwrap()
        };
    }

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
                ZenohBody::Data(Data { payload, .. }) => payload.contiguous().into_owned(),
                _ => panic!("Unsolicited message"),
            };
            assert_eq!(payload.len(), MSG_SIZE);

            let mut count_bytes = [0_u8; 8];
            count_bytes.copy_from_slice(&payload[0..8]);
            let msg_count = u64::from_le_bytes(count_bytes) as usize;
            let sex_count = self.count.fetch_add(1, Ordering::SeqCst);
            assert_eq!(msg_count, sex_count);
            print!("{msg_count} ");

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
        println!("Transport SHM [0a]: {endpoint:?}");

        // Define client and router IDs
        let peer_shm01 = ZenohId::try_from([1]).unwrap();
        let peer_shm02 = ZenohId::try_from([2]).unwrap();
        let peer_net01 = ZenohId::try_from([3]).unwrap();

        // Create the SharedMemoryManager
        let mut shm01 =
            SharedMemoryManager::make(format!("peer_shm01_{}", endpoint.protocol()), 2 * MSG_SIZE)
                .unwrap();

        // Create a peer manager with shared-memory authenticator enabled
        let peer_shm01_handler = Arc::new(SHPeer::new(false));
        let unicast =
            TransportManager::config_unicast().peer_authenticator(HashSet::from_iter(vec![
                SharedMemoryAuthenticator::make().unwrap().into(),
            ]));
        let peer_shm01_manager = TransportManager::builder()
            .whatami(WhatAmI::Peer)
            .zid(peer_shm01)
            .unicast(unicast)
            .build(peer_shm01_handler.clone())
            .unwrap();

        // Create a peer manager with shared-memory authenticator enabled
        let peer_shm02_handler = Arc::new(SHPeer::new(true));
        let unicast =
            TransportManager::config_unicast().peer_authenticator(HashSet::from_iter(vec![
                SharedMemoryAuthenticator::make().unwrap().into(),
            ]));
        let peer_shm02_manager = TransportManager::builder()
            .whatami(WhatAmI::Peer)
            .zid(peer_shm02)
            .unicast(unicast)
            .build(peer_shm02_handler.clone())
            .unwrap();

        // Create a peer manager with shared-memory authenticator disabled
        let peer_net01_handler = Arc::new(SHPeer::new(false));
        let peer_net01_manager = TransportManager::builder()
            .whatami(WhatAmI::Peer)
            .zid(peer_net01)
            .build(peer_net01_handler.clone())
            .unwrap();

        // Create the listener on the peer
        println!("Transport SHM [1a]");
        let _ = ztimeout!(peer_shm01_manager
            .add_listener(endpoint.clone())
            .timeout(TIMEOUT))
        .unwrap();

        // Create a transport with the peer
        println!("Transport SHM [1b]");
        let _ = ztimeout!(peer_shm02_manager.open_transport(endpoint.clone())).unwrap();

        // Create a transport with the peer
        println!("Transport SHM [1c]");
        let _ = ztimeout!(peer_net01_manager.open_transport(endpoint.clone())).unwrap();

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
            let mut sbuf = ztimeout!(async {
                loop {
                    match shm01.alloc(MSG_SIZE) {
                        Ok(sbuf) => break sbuf,
                        Err(_) => task::sleep(USLEEP).await,
                    }
                }
            });

            let bs = unsafe { sbuf.as_mut_slice() };
            bs[0..8].copy_from_slice(&msg_count.to_le_bytes());

            let key = "test".into();
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
        println!("Transport SHM [3b]");
        ztimeout!(async {
            while peer_shm02_handler.get_count() != MSG_COUNT {
                task::sleep(SLEEP).await;
            }
        });

        // Send the message
        println!("Transport SHM [4a]");
        // The msg count
        for (msg_count, _) in (0..MSG_COUNT).enumerate() {
            // Create the message to send
            let mut sbuf = ztimeout!(async {
                loop {
                    match shm01.alloc(MSG_SIZE) {
                        Ok(sbuf) => break sbuf,
                        Err(_) => task::sleep(USLEEP).await,
                    }
                }
            });
            let bs = unsafe { sbuf.as_mut_slice() };
            bs[0..8].copy_from_slice(&msg_count.to_le_bytes());

            let key = "test".into();
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
        println!("Transport SHM [4b]");
        ztimeout!(async {
            while peer_net01_handler.get_count() != MSG_COUNT {
                task::sleep(SLEEP).await;
            }
        });

        // Wait a little bit
        task::sleep(SLEEP).await;

        // Close the transports
        println!("Transport SHM [5a]");
        ztimeout!(peer_shm02_transport.close()).unwrap();

        println!("Transport SHM [5b]");
        ztimeout!(peer_net01_transport.close()).unwrap();

        ztimeout!(async {
            while !peer_shm01_manager.get_transports().is_empty() {
                task::sleep(SLEEP).await;
            }
        });

        // Delete the listener
        println!("Transport SHM [6a]");
        ztimeout!(peer_shm01_manager.del_listener(endpoint)).unwrap();

        // Wait a little bit
        ztimeout!(async {
            while !peer_shm01_manager.get_listeners().is_empty() {
                task::sleep(SLEEP).await;
            }
        });
        task::sleep(SLEEP).await;

        ztimeout!(peer_net01_manager.close());
        ztimeout!(peer_shm01_manager.close());
        ztimeout!(peer_shm02_manager.close());

        // Wait a little bit
        task::sleep(SLEEP).await;
    }

    #[cfg(all(feature = "transport_tcp", feature = "shared-memory"))]
    #[test]
    fn transport_tcp_shm() {
        let _ = env_logger::try_init();
        task::block_on(async {
            zasync_executor_init!();
        });

        let endpoint: EndPoint = format!("tcp/127.0.0.1:{}", 14000).parse().unwrap();
        task::block_on(run(&endpoint));
    }

    #[cfg(all(feature = "transport_ws", feature = "shared-memory"))]
    #[test]
    fn transport_ws_shm() {
        let _ = env_logger::try_init();
        task::block_on(async {
            zasync_executor_init!();
        });

        let endpoint: EndPoint = format!("ws/127.0.0.1:{}", 14010).parse().unwrap();
        task::block_on(run(&endpoint));
    }
}
