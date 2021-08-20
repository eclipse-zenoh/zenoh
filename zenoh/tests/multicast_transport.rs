// //
// // Copyright (c) 2017, 2020 ADLINK Technology Inc.
// //
// // This program and the accompanying materials are made available under the
// // terms of the Eclipse Public License 2.0 which is available at
// // http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// // which is available at https://www.apache.org/licenses/LICENSE-2.0.
// //
// // SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
// //
// // Contributors:
// //   ADLINK zenoh team, <zenoh@adlink-labs.tech>
// //
// use async_std::prelude::*;
// use async_std::sync::Arc;
// use async_std::task;
// use std::any::Any;
// use std::sync::atomic::{AtomicUsize, Ordering};
// use std::time::Duration;
// use zenoh::net::link::{Locator, LocatorProperty};
// use zenoh::net::protocol::core::{
//     whatami, Channel, CongestionControl, PeerId, Priority, Reliability, ResKey,
// };
// use zenoh::net::protocol::io::ZBuf;
// use zenoh::net::protocol::proto::ZenohMessage;
// use zenoh::net::transport::{
//     TransportEventHandler, TransportManager, TransportManagerConfig, TransportMulticast,
//     TransportMulticastEventHandler, TransportUnicast, TransportUnicastEventHandler,
// };
// use zenoh_util::core::ZResult;
// use zenoh_util::zasync_executor_init;

// const TIMEOUT: Duration = Duration::from_secs(60);
// const SLEEP: Duration = Duration::from_secs(1);
// const SLEEP_COUNT: Duration = Duration::from_millis(10);

// const MSG_COUNT: usize = 1_000;
// const MSG_SIZE_NOFRAG: [usize; 1] = [1_024];

// // Transport Handler for the peer02
// struct SHPeer {
//     count: Arc<AtomicUsize>,
// }

// impl Default for SHPeer {
//     fn default() -> Self {
//         Self {
//             count: Arc::new(AtomicUsize::new(0)),
//         }
//     }
// }

// impl SHPeer {
//     fn get_count(&self) -> usize {
//         self.count.load(Ordering::SeqCst)
//     }
// }

// impl TransportEventHandler for SHPeer {
//     fn new_unicast(
//         &self,
//         _transport: TransportUnicast,
//     ) -> ZResult<Arc<dyn TransportUnicastEventHandler>> {
//         panic!();
//     }

//     fn new_multicast(
//         &self,
//         _transport: TransportMulticast,
//     ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
//         let arc = Arc::new(SCPeer::new(self.count.clone()));
//         Ok(arc)
//     }
// }

// // Transport Callback for the peer02
// pub struct SCPeer {
//     count: Arc<AtomicUsize>,
// }

// impl SCPeer {
//     pub fn new(count: Arc<AtomicUsize>) -> Self {
//         Self { count }
//     }
// }

// impl TransportMulticastEventHandler for SCPeer {
//     fn handle_message(&self, _msg: ZenohMessage, _peer: &PeerId) -> ZResult<()> {
//         self.count.fetch_add(1, Ordering::SeqCst);
//         Ok(())
//     }

//     fn new_peer(&self, _peer: PeerId) {}
//     fn del_peer(&self, _peer: PeerId) {}
//     fn closing(&self) {}
//     fn closed(&self) {}

//     fn as_any(&self) -> &dyn Any {
//         self
//     }
// }

// struct TransportPeer {
//     _manager: TransportManager,
//     handler: Arc<SHPeer>,
//     transport: TransportMulticast,
// }

// async fn open_transport(
//     locator: &Locator,
//     locator_property: Option<LocatorProperty>,
// ) -> (TransportPeer, TransportPeer) {
//     // Define peer01 and peer02 IDs
//     let peer01_id = PeerId::new(1, [0u8; PeerId::MAX_SIZE]);
//     let peer02_id = PeerId::new(1, [1u8; PeerId::MAX_SIZE]);

//     // Create the peer01 transport manager
//     let peer01_handler = Arc::new(SHPeer::default());
//     let config = TransportManagerConfig::builder()
//         .pid(peer01_id.clone())
//         .whatami(whatami::PEER)
//         .locator_property(
//             locator_property
//                 .clone()
//                 .map_or_else(Vec::new, |lp| vec![lp]),
//         )
//         .build(peer01_handler.clone());
//     let peer01_manager = TransportManager::new(config);

//     // Create the peer02 transport manager
//     let peer02_handler = Arc::new(SHPeer::default());
//     let config = TransportManagerConfig::builder()
//         .whatami(whatami::PEER)
//         .pid(peer02_id)
//         .locator_property(
//             locator_property
//                 .clone()
//                 .map_or_else(Vec::new, |lp| vec![lp]),
//         )
//         .build(peer02_handler.clone());
//     let peer02_manager = TransportManager::new(config);

//     // Create an empty transport with the peer01
//     // Open transport -> This should be accepted
//     println!("Opening transport with {}", locator);
//     let _ = peer01_manager
//         .open_transport_multicast(locator)
//         .timeout(TIMEOUT)
//         .await
//         .unwrap()
//         .unwrap();

//     println!("Opening transport with {}", locator);
//     let _ = peer02_manager
//         .open_transport_multicast(locator)
//         .timeout(TIMEOUT)
//         .await
//         .unwrap()
//         .unwrap();

//     let peer01_transport = peer01_manager.get_transport_multicast(locator).unwrap();
//     let peer02_transport = peer02_manager.get_transport_multicast(locator).unwrap();

//     (
//         TransportPeer {
//             _manager: peer01_manager,
//             handler: peer01_handler,
//             transport: peer01_transport,
//         },
//         TransportPeer {
//             _manager: peer02_manager,
//             handler: peer02_handler,
//             transport: peer02_transport,
//         },
//     )
// }

// async fn close_transport(peer01: TransportPeer, peer02: TransportPeer, locator: &Locator) {
//     // Close the peer01 transport
//     println!("Closing transport with {}", locator);
//     let _ = peer01
//         .transport
//         .close()
//         .timeout(TIMEOUT)
//         .await
//         .unwrap()
//         .unwrap();

//     println!("Closing transport with {}", locator);
//     let _ = peer02
//         .transport
//         .close()
//         .timeout(TIMEOUT)
//         .await
//         .unwrap()
//         .unwrap();

//     // Wait a little bit
//     task::sleep(SLEEP).await;
// }

// async fn single_run(
//     peer01: &TransportPeer,
//     peer02: &TransportPeer,
//     channel: Channel,
//     msg_size: usize,
// ) {
//     // Create the message to send
//     let key = ResKey::RName("/test".to_string());
//     let payload = ZBuf::from(vec![0u8; msg_size]);
//     let data_info = None;
//     let routing_context = None;
//     let reply_context = None;
//     let attachment = None;
//     let message = ZenohMessage::make_data(
//         key,
//         payload,
//         channel,
//         CongestionControl::Block,
//         data_info,
//         routing_context,
//         reply_context,
//         attachment,
//     );

//     println!(
//         "Sending {} messages... {:?} {}",
//         MSG_COUNT, channel, msg_size
//     );
//     for _ in 0..MSG_COUNT {
//         peer01.transport.schedule(message.clone()).unwrap();
//     }

//     match channel.reliability {
//         Reliability::Reliable => {
//             let count = async {
//                 while peer02.handler.get_count() != MSG_COUNT {
//                     task::sleep(SLEEP_COUNT).await;
//                 }
//             };
//             let _ = count.timeout(TIMEOUT).await.unwrap();
//         }
//         Reliability::BestEffort => {
//             let count = async {
//                 while peer02.handler.get_count() == 0 {
//                     task::sleep(SLEEP_COUNT).await;
//                 }
//             };
//             let _ = count.timeout(TIMEOUT).await.unwrap();
//         }
//     };

//     // Wait a little bit
//     task::sleep(SLEEP).await;
// }

// async fn run(
//     locator: &Locator,
//     properties: Option<LocatorProperty>,
//     channel: &[Channel],
//     msg_size: &[usize],
// ) {
//     for ch in channel.iter() {
//         for ms in msg_size.iter() {
//             let (peer01, peer02) = open_transport(locator, properties.clone()).await;
//             single_run(&peer01, &peer02, *ch, *ms).await;
//             close_transport(peer01, peer02, locator).await;
//         }
//     }
// }

// #[cfg(feature = "transport_udp")]
// #[test]
// fn transport_multicast_udp_only() {
//     env_logger::init();

//     task::block_on(async {
//         zasync_executor_init!();
//     });

//     // Define the locator
//     let locator: Locator = "udp/224.0.0.224:7447".parse().unwrap();
//     let properties = None;
//     // Define the reliability and congestion control
//     let channel = [
//         Channel {
//             priority: Priority::default(),
//             reliability: Reliability::BestEffort,
//         },
//         Channel {
//             priority: Priority::RealTime,
//             reliability: Reliability::BestEffort,
//         },
//     ];
//     // Run
//     task::block_on(run(&locator, properties, &channel, &MSG_SIZE_NOFRAG));
// }
