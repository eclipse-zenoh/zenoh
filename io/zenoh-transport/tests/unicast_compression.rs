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
#[cfg(feature = "transport_compression")]
mod tests {
    use std::{
        any::Any,
        convert::TryFrom,
        fmt::Write as _,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        time::Duration,
    };

    use zenoh_core::ztimeout;
    use zenoh_link::Link;
    use zenoh_protocol::{
        core::{
            Channel, CongestionControl, EndPoint, Priority, Reliability, WhatAmI, ZenohIdProto,
        },
        network::{push::ext::QoSType, NetworkMessage, NetworkMessageMut, Push},
    };
    use zenoh_result::ZResult;
    use zenoh_transport::{
        multicast::TransportMulticast,
        unicast::{test_helpers::make_transport_manager_builder, TransportUnicast},
        TransportEventHandler, TransportManager, TransportMulticastEventHandler, TransportPeer,
        TransportPeerEventHandler,
    };

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const SLEEP_COUNT: Duration = Duration::from_millis(10);

    const MSG_COUNT: usize = 1_000;
    const MSG_SIZE_ALL: [usize; 2] = [1_024, 131_072];
    const MSG_SIZE_NOFRAG: [usize; 1] = [1_024];
    const MSG_SIZE_LOWLATENCY: [usize; 1] = MSG_SIZE_NOFRAG;

    // Transport Handler for the router
    struct SHRouter {
        count: Arc<AtomicUsize>,
    }

    impl Default for SHRouter {
        fn default() -> Self {
            Self {
                count: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    impl SHRouter {
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
            let arc = Arc::new(SCRouter::new(self.count.clone()));
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
        count: Arc<AtomicUsize>,
    }

    impl SCRouter {
        pub fn new(count: Arc<AtomicUsize>) -> Self {
            Self { count }
        }
    }

    impl TransportPeerEventHandler for SCRouter {
        fn handle_message(&self, _message: NetworkMessageMut) -> ZResult<()> {
            self.count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn new_link(&self, _link: Link) {}
        fn del_link(&self, _link: Link) {}
        fn closed(&self) {}

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    // Transport Handler for the client
    #[derive(Default)]
    struct SHClient;

    impl TransportEventHandler for SHClient {
        fn new_unicast(
            &self,
            _peer: TransportPeer,
            _transport: TransportUnicast,
        ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
            Ok(Arc::new(SCClient))
        }

        fn new_multicast(
            &self,
            _transport: TransportMulticast,
        ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
            panic!();
        }
    }

    // Transport Callback for the client
    #[derive(Default)]
    pub struct SCClient;

    impl TransportPeerEventHandler for SCClient {
        fn handle_message(&self, _message: NetworkMessageMut) -> ZResult<()> {
            Ok(())
        }

        fn new_link(&self, _link: Link) {}
        fn del_link(&self, _link: Link) {}
        fn closed(&self) {}

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    async fn open_transport_unicast(
        client_endpoints: &[EndPoint],
        server_endpoints: &[EndPoint],
        lowlatency_transport: bool,
    ) -> (
        TransportManager,
        Arc<SHRouter>,
        TransportManager,
        TransportUnicast,
    ) {
        // Define client and router IDs
        let client_id = ZenohIdProto::try_from([1]).unwrap();
        let router_id = ZenohIdProto::try_from([2]).unwrap();

        // Create the router transport manager
        let router_handler = Arc::new(SHRouter::default());
        let unicast = make_transport_manager_builder(
            #[cfg(feature = "transport_multilink")]
            server_endpoints.len(),
            lowlatency_transport,
        )
        .compression(true);
        let router_manager = TransportManager::builder()
            .zid(router_id)
            .whatami(WhatAmI::Router)
            .unicast(unicast)
            .build(router_handler.clone())
            .unwrap();

        // Create the listener on the router
        for e in server_endpoints.iter() {
            println!("Add endpoint: {e}");
            let _ = ztimeout!(router_manager.add_listener(e.clone())).unwrap();
        }

        // Create the client transport manager
        let unicast = make_transport_manager_builder(
            #[cfg(feature = "transport_multilink")]
            client_endpoints.len(),
            lowlatency_transport,
        )
        .compression(true);
        let client_manager = TransportManager::builder()
            .whatami(WhatAmI::Client)
            .zid(client_id)
            .unicast(unicast)
            .build(Arc::new(SHClient))
            .unwrap();

        // Create an empty transport with the client
        // Open transport -> This should be accepted
        for e in client_endpoints.iter() {
            println!("Opening transport with {e}");
            let _ = ztimeout!(client_manager.open_transport_unicast(e.clone())).unwrap();
        }

        let client_transport = ztimeout!(client_manager.get_transport_unicast(&router_id)).unwrap();

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
        let mut ee = String::new();
        for e in endpoints.iter() {
            let _ = write!(ee, "{e} ");
        }
        println!("Closing transport with {ee}");
        ztimeout!(client_transport.close()).unwrap();

        ztimeout!(async {
            while !router_manager.get_transports_unicast().await.is_empty() {
                tokio::time::sleep(SLEEP).await;
            }
        });

        // Stop the locators on the manager
        for e in endpoints.iter() {
            println!("Del locator: {e}");
            ztimeout!(router_manager.del_listener(e)).unwrap();
        }

        ztimeout!(async {
            while !router_manager.get_listeners().await.is_empty() {
                tokio::time::sleep(SLEEP).await;
            }
        });

        // Wait a little bit
        tokio::time::sleep(SLEEP).await;

        ztimeout!(router_manager.close());
        ztimeout!(client_manager.close());

        // Wait a little bit
        tokio::time::sleep(SLEEP).await;
    }

    async fn test_transport(
        router_handler: Arc<SHRouter>,
        client_transport: TransportUnicast,
        channel: Channel,
        msg_size: usize,
    ) {
        println!("Sending {MSG_COUNT} messages... {channel:?} {msg_size}");
        let cctrl = match channel.reliability {
            Reliability::Reliable => CongestionControl::Block,
            Reliability::BestEffort => CongestionControl::Drop,
        };
        // Create the message to send
        let message = NetworkMessage::from(Push {
            wire_expr: "test".into(),
            ext_qos: QoSType::new(channel.priority, cctrl, false),
            ..Push::from(vec![0u8; msg_size])
        });
        for _ in 0..MSG_COUNT {
            let _ = client_transport.schedule(message.clone().as_mut());
        }

        match channel.reliability {
            Reliability::Reliable => {
                ztimeout!(async {
                    while router_handler.get_count() != MSG_COUNT {
                        tokio::time::sleep(SLEEP_COUNT).await;
                    }
                });
            }
            Reliability::BestEffort => {
                ztimeout!(async {
                    while router_handler.get_count() == 0 {
                        tokio::time::sleep(SLEEP_COUNT).await;
                    }
                });
            }
        };

        // Wait a little bit
        tokio::time::sleep(SLEEP).await;
    }

    async fn run_single(
        client_endpoints: &[EndPoint],
        server_endpoints: &[EndPoint],
        channel: Channel,
        msg_size: usize,
        lowlatency_transport: bool,
    ) {
        println!(
            "\n>>> Running test for:  {client_endpoints:?}, {server_endpoints:?}, {channel:?}, {msg_size}"
        );

        #[allow(unused_variables)] // Used when stats feature is enabled
        let (router_manager, router_handler, client_manager, client_transport) =
            open_transport_unicast(client_endpoints, server_endpoints, lowlatency_transport).await;

        test_transport(
            router_handler.clone(),
            client_transport.clone(),
            channel,
            msg_size,
        )
        .await;

        #[cfg(feature = "stats")]
        {
            let c_stats = client_transport.get_stats().unwrap().report();
            println!("\tClient: {c_stats:?}");
            let r_stats =
                ztimeout!(router_manager.get_transport_unicast(&client_manager.config.zid))
                    .unwrap()
                    .get_stats()
                    .map(|s| s.report())
                    .unwrap();
            println!("\tRouter: {r_stats:?}");
        }

        close_transport(
            router_manager,
            client_manager,
            client_transport,
            client_endpoints,
        )
        .await;
    }

    async fn run_internal(
        client_endpoints: &[EndPoint],
        server_endpoints: &[EndPoint],
        channel: &[Channel],
        msg_size: &[usize],
        lowlatency_transport: bool,
    ) {
        for ch in channel.iter() {
            for ms in msg_size.iter() {
                run_single(
                    client_endpoints,
                    server_endpoints,
                    *ch,
                    *ms,
                    lowlatency_transport,
                )
                .await;
            }
        }
    }

    async fn run_with_universal_transport(
        client_endpoints: &[EndPoint],
        server_endpoints: &[EndPoint],
        channel: &[Channel],
        msg_size: &[usize],
    ) {
        run_internal(client_endpoints, server_endpoints, channel, msg_size, false).await;
    }

    async fn run_with_lowlatency_transport(
        client_endpoints: &[EndPoint],
        server_endpoints: &[EndPoint],
        channel: &[Channel],
        msg_size: &[usize],
    ) {
        if client_endpoints.len() > 1 || server_endpoints.len() > 1 {
            println!("LowLatency transport doesn't support more than one link, so this test would produce MAX_LINKS error!");
            panic!();
        }
        run_internal(client_endpoints, server_endpoints, channel, msg_size, true).await;
    }

    #[cfg(feature = "transport_tcp")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn transport_unicast_compression_tcp_only() {
        zenoh_util::init_log_from_env_or("error");

        // Define the locators
        let endpoints: Vec<EndPoint> = vec![
            format!("tcp/127.0.0.1:{}", 19000).parse().unwrap(),
            format!("tcp/[::1]:{}", 19001).parse().unwrap(),
        ];
        // Define the reliability and congestion control
        let channel = [
            Channel {
                priority: Priority::DEFAULT,
                reliability: Reliability::Reliable,
            },
            Channel {
                priority: Priority::RealTime,
                reliability: Reliability::Reliable,
            },
        ];
        // Run
        run_with_universal_transport(&endpoints, &endpoints, &channel, &MSG_SIZE_ALL).await;
    }

    #[cfg(feature = "transport_tcp")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn transport_unicast_compression_tcp_only_with_lowlatency_transport() {
        zenoh_util::init_log_from_env_or("error");

        // Define the locators
        let endpoints: Vec<EndPoint> = vec![format!("tcp/127.0.0.1:{}", 19100).parse().unwrap()];
        // Define the reliability and congestion control
        let channel = [
            Channel {
                priority: Priority::DEFAULT,
                reliability: Reliability::Reliable,
            },
            Channel {
                priority: Priority::RealTime,
                reliability: Reliability::Reliable,
            },
        ];
        // Run
        run_with_lowlatency_transport(&endpoints, &endpoints, &channel, &MSG_SIZE_LOWLATENCY).await;
    }

    #[cfg(feature = "transport_udp")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn transport_unicast_compression_udp_only() {
        zenoh_util::init_log_from_env_or("error");

        // Define the locator
        let endpoints: Vec<EndPoint> = vec![
            format!("udp/127.0.0.1:{}", 19010).parse().unwrap(),
            format!("udp/[::1]:{}", 19011).parse().unwrap(),
        ];
        // Define the reliability and congestion control
        let channel = [
            Channel {
                priority: Priority::DEFAULT,
                reliability: Reliability::BestEffort,
            },
            Channel {
                priority: Priority::RealTime,
                reliability: Reliability::BestEffort,
            },
        ];
        // Run
        run_with_universal_transport(&endpoints, &endpoints, &channel, &MSG_SIZE_NOFRAG).await;
    }

    #[cfg(feature = "transport_udp")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn transport_unicast_compression_udp_only_with_lowlatency_transport() {
        zenoh_util::init_log_from_env_or("error");

        // Define the locator
        let endpoints: Vec<EndPoint> = vec![format!("udp/127.0.0.1:{}", 19110).parse().unwrap()];
        // Define the reliability and congestion control
        let channel = [
            Channel {
                priority: Priority::DEFAULT,
                reliability: Reliability::BestEffort,
            },
            Channel {
                priority: Priority::RealTime,
                reliability: Reliability::BestEffort,
            },
        ];
        // Run
        run_with_lowlatency_transport(&endpoints, &endpoints, &channel, &MSG_SIZE_NOFRAG).await;
    }
}
