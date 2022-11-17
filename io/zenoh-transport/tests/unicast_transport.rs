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
use async_std::prelude::FutureExt;
use async_std::task;
use std::any::Any;
use std::convert::TryFrom;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use zenoh_core::zasync_executor_init;
use zenoh_core::Result as ZResult;
use zenoh_link::{EndPoint, Link};
use zenoh_protocol::io::ZBuf;
use zenoh_protocol::proto::ZenohMessage;
use zenoh_protocol_core::{Channel, CongestionControl, Priority, Reliability, WhatAmI, ZenohId};
use zenoh_transport::{
    TransportEventHandler, TransportManager, TransportMulticast, TransportMulticastEventHandler,
    TransportPeer, TransportPeerEventHandler, TransportUnicast,
};

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);
const SLEEP_COUNT: Duration = Duration::from_millis(10);

const MSG_COUNT: usize = 1_000;
const MSG_SIZE_ALL: [usize; 2] = [1_024, 131_072];
const MSG_SIZE_NOFRAG: [usize; 1] = [1_024];

macro_rules! ztimeout {
    ($f:expr) => {
        $f.timeout(TIMEOUT).await.unwrap()
    };
}

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
    fn handle_message(&self, _message: ZenohMessage) -> ZResult<()> {
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

// Transport Handler for the client
#[derive(Default)]
struct SHClient;

impl TransportEventHandler for SHClient {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        Ok(Arc::new(SCClient::default()))
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
    fn handle_message(&self, _message: ZenohMessage) -> ZResult<()> {
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

async fn open_transport(
    client_endpoints: &[EndPoint],
    server_endpoint: Option<&EndPoint>,
) -> ZResult<(
    TransportManager,
    Arc<SHRouter>,
    TransportManager,
    TransportUnicast,
)> {
    // Define client and router IDs
    let client_id = ZenohId::try_from([1]).unwrap();
    let router_id = ZenohId::try_from([2]).unwrap();

    // Create the router transport manager
    let router_handler = Arc::new(SHRouter::default());
    let unicast = TransportManager::config_unicast().max_links(client_endpoints.len());

    let router_manager = TransportManager::builder()
        .zid(router_id)
        .whatami(WhatAmI::Router)
        .unicast(unicast)
        .build(router_handler.clone())
        .unwrap();

    if let Some(endpoint) = server_endpoint {
        // Create the listener on the router
        println!("Add endpoint: {}", endpoint);
        let _ = ztimeout!(router_manager.add_listener(endpoint.clone()))?;
    } else {
        // Create the listener on the router
        for e in client_endpoints.iter() {
            println!("Add endpoint: {}\n", e);
            let _ = ztimeout!(router_manager.add_listener(e.clone()))?;
        }
    }

    // Create the client transport manager
    let unicast = TransportManager::config_unicast().max_links(client_endpoints.len());
    let client_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client_id)
        .unicast(unicast)
        .build(Arc::new(SHClient::default()))
        .unwrap();

    // Create an empty transport with the client
    // Open transport -> This should be accepted
    for e in client_endpoints.iter() {
        println!("Opening transport with {}", e);
        let _ = ztimeout!(client_manager.open_transport(e.clone()))?;
    }

    let client_transport = client_manager.get_transport(&router_id).unwrap();

    // Return the handlers
    Ok((
        router_manager,
        router_handler,
        client_manager,
        client_transport,
    ))
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
        let _ = write!(ee, "{} ", e);
    }
    println!("Closing transport with {}", ee);
    ztimeout!(client_transport.close()).unwrap();

    ztimeout!(async {
        while !router_manager.get_transports().is_empty() {
            task::sleep(SLEEP).await;
        }
    });

    // Stop the locators on the manager
    for e in endpoints.iter() {
        println!("Del locator: {}", e);
        ztimeout!(router_manager.del_listener(e)).unwrap();
    }

    ztimeout!(async {
        while !router_manager.get_listeners().is_empty() {
            task::sleep(SLEEP).await;
        }
    });

    // Wait a little bit
    task::sleep(SLEEP).await;

    ztimeout!(router_manager.close());
    ztimeout!(client_manager.close());

    // Wait a little bit
    task::sleep(SLEEP).await;
}

async fn test_transport(
    router_handler: Arc<SHRouter>,
    client_transport: TransportUnicast,
    channel: Channel,
    msg_size: usize,
) {
    // Create the message to send
    let key = "test".into();
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
        client_transport.schedule(message.clone()).unwrap();
    }

    match channel.reliability {
        Reliability::Reliable => {
            ztimeout!(async {
                while router_handler.get_count() != MSG_COUNT {
                    task::sleep(SLEEP_COUNT).await;
                }
            });
        }
        Reliability::BestEffort => {
            ztimeout!(async {
                while router_handler.get_count() == 0 {
                    task::sleep(SLEEP_COUNT).await;
                }
            });
        }
    };

    // Wait a little bit
    task::sleep(SLEEP).await;
}

async fn run_single(
    client_endpoints: &[EndPoint],
    server_endpoint: Option<&EndPoint>,
    channel: Channel,
    msg_size: usize,
) -> ZResult<()> {
    #[allow(unused_variables)] // Used when stats feature is enabled
    let (router_manager, router_handler, client_manager, client_transport) =
        open_transport(client_endpoints, server_endpoint).await?;

    test_transport(
        router_handler.clone(),
        client_transport.clone(),
        channel,
        msg_size,
    )
    .await;

    #[cfg(feature = "stats")]
    {
        let c_stats = client_transport.get_stats().unwrap();
        println!("\tClient: {:?}", c_stats,);
        let r_stats = router_manager
            .get_transport_unicast(&client_manager.config.zid)
            .unwrap()
            .get_stats()
            .unwrap();
        println!("\tRouter: {:?}", r_stats);
    }

    close_transport(
        router_manager,
        client_manager,
        client_transport,
        client_endpoints,
    )
    .await;
    Ok(())
}

async fn run(
    client_endpoints: &[EndPoint],
    server_endpoint: Option<&EndPoint>,
    channel: &[Channel],
    msg_size: &[usize],
) -> ZResult<()> {
    for ch in channel.iter() {
        for ms in msg_size.iter() {
            run_single(client_endpoints, server_endpoint, *ch, *ms).await?;
        }
    }
    Ok(())
}

#[cfg(feature = "transport_tcp")]
#[test]
fn transport_unicast_tcp_only() {
    task::block_on(async {
        zasync_executor_init!();
    });

    // Define the locators
    let endpoints: Vec<EndPoint> = vec![
        "tcp/127.0.0.1:10447".parse().unwrap(),
        "tcp/[::1]:10447".parse().unwrap(),
        "tcp/localhost:10453".parse().unwrap(),
    ];
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
    let result = task::block_on(run(&endpoints, None, &channel, &MSG_SIZE_ALL));
    assert!(result.is_ok())
}

#[cfg(feature = "transport_udp")]
#[test]
fn transport_unicast_udp_only() {
    task::block_on(async {
        zasync_executor_init!();
    });

    // Define the locator
    let endpoints: Vec<EndPoint> = vec![
        "udp/127.0.0.1:10447".parse().unwrap(),
        "udp/[::1]:10447".parse().unwrap(),
        "udp/localhost:10453".parse().unwrap(),
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
    let result = task::block_on(run(&endpoints, None, &channel, &MSG_SIZE_NOFRAG));
    assert!(result.is_ok())
}

#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
#[test]
fn transport_unicast_unix_only() {
    task::block_on(async {
        zasync_executor_init!();
    });

    let _ = std::fs::remove_file("zenoh-test-unix-socket-5.sock");
    // Define the locator
    let endpoints: Vec<EndPoint> = vec!["unixsock-stream/zenoh-test-unix-socket-5.sock"
        .parse()
        .unwrap()];
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
    let result = task::block_on(run(&endpoints, None, &channel, &MSG_SIZE_ALL));
    let _ = std::fs::remove_file("zenoh-test-unix-socket-5.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-5.sock.lock");
    assert!(result.is_ok())
}

#[cfg(feature = "transport_ws")]
#[test]
fn transport_unicast_ws_only() {
    task::block_on(async {
        zasync_executor_init!();
    });

    // Define the locators
    let endpoints: Vec<EndPoint> = vec![
        "ws/127.0.0.1:11447".parse().unwrap(),
        "ws/[::1]:11447".parse().unwrap(),
    ];
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
    task::block_on(run(&endpoints, &channel, &MSG_SIZE_ALL));
}

#[cfg(all(feature = "transport_tcp", feature = "transport_udp"))]
#[test]
fn transport_unicast_tcp_udp() {
    task::block_on(async {
        zasync_executor_init!();
    });

    // Define the locator
    let endpoints: Vec<EndPoint> = vec![
        "tcp/127.0.0.1:10448".parse().unwrap(),
        "udp/127.0.0.1:10448".parse().unwrap(),
        "tcp/[::1]:10448".parse().unwrap(),
        "udp/[::1]:10448".parse().unwrap(),
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
    let result = task::block_on(run(&endpoints, None, &channel, &MSG_SIZE_NOFRAG));
    assert!(result.is_ok())
}

#[cfg(all(
    feature = "transport_tcp",
    feature = "transport_unixsock-stream",
    target_family = "unix"
))]
#[test]
fn transport_unicast_tcp_unix() {
    task::block_on(async {
        zasync_executor_init!();
    });

    let _ = std::fs::remove_file("zenoh-test-unix-socket-6.sock");
    // Define the locator
    let endpoints: Vec<EndPoint> = vec![
        "tcp/127.0.0.1:10449".parse().unwrap(),
        "tcp/[::1]:10449".parse().unwrap(),
        "unixsock-stream/zenoh-test-unix-socket-6.sock"
            .parse()
            .unwrap(),
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
    let result = task::block_on(run(&endpoints, None, &channel, &MSG_SIZE_ALL));
    let _ = std::fs::remove_file("zenoh-test-unix-socket-6.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-6.sock.lock");
    assert!(result.is_ok())
}

#[cfg(all(
    feature = "transport_udp",
    feature = "transport_unixsock-stream",
    target_family = "unix"
))]
#[test]
fn transport_unicast_udp_unix() {
    task::block_on(async {
        zasync_executor_init!();
    });

    let _ = std::fs::remove_file("zenoh-test-unix-socket-7.sock");
    // Define the locator
    let endpoints: Vec<EndPoint> = vec![
        "udp/127.0.0.1:10449".parse().unwrap(),
        "udp/[::1]:10449".parse().unwrap(),
        "unixsock-stream/zenoh-test-unix-socket-7.sock"
            .parse()
            .unwrap(),
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
    let result = task::block_on(run(&endpoints, None, &channel, &MSG_SIZE_NOFRAG));
    let _ = std::fs::remove_file("zenoh-test-unix-socket-7.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-7.sock.lock");
    assert!(result.is_ok())
}

#[cfg(all(
    feature = "transport_tcp",
    feature = "transport_udp",
    feature = "transport_unixsock-stream",
    target_family = "unix"
))]
#[test]
fn transport_unicast_tcp_udp_unix() {
    task::block_on(async {
        zasync_executor_init!();
    });

    let _ = std::fs::remove_file("zenoh-test-unix-socket-8.sock");
    // Define the locator
    let endpoints: Vec<EndPoint> = vec![
        "tcp/127.0.0.1:10450".parse().unwrap(),
        "udp/127.0.0.1:10450".parse().unwrap(),
        "tcp/[::1]:10450".parse().unwrap(),
        "udp/[::1]:10450".parse().unwrap(),
        "unixsock-stream/zenoh-test-unix-socket-8.sock"
            .parse()
            .unwrap(),
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
    let result = task::block_on(run(&endpoints, None, &channel, &MSG_SIZE_NOFRAG));
    let _ = std::fs::remove_file("zenoh-test-unix-socket-8.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-8.sock.lock");
    assert!(result.is_ok())
}

#[cfg(all(feature = "transport_tls", target_family = "unix"))]
#[test]
fn transport_unicast_tls_only() {
    use zenoh_link::tls::config::*;

    task::block_on(async {
        zasync_executor_init!();
    });

    // Define the locator
    let mut endpoint: EndPoint = ("tls/localhost:10451").parse().unwrap();
    endpoint.extend_configuration(
        [
            (TLS_ROOT_CA_CERTIFICATE_RAW, CLIENT_CA),
            (TLS_SERVER_CERTIFICATE_RAW, SERVER_CERT),
            (TLS_SERVER_PRIVATE_KEY_RAW, SERVER_KEY),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
    );
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
    let endpoints = vec![endpoint];
    let result = task::block_on(run(&endpoints, None, &channel, &MSG_SIZE_ALL));
    assert!(result.is_ok())
}

#[cfg(feature = "transport_quic")]
#[test]
fn transport_unicast_quic_only() {
    use zenoh_link::quic::config::*;

    task::block_on(async {
        zasync_executor_init!();
    });

    // Define the locator
    let mut endpoint: EndPoint = ("quic/localhost:10452").parse().unwrap();
    endpoint.extend_configuration(
        [
            (TLS_ROOT_CA_CERTIFICATE_RAW, CLIENT_CA),
            (TLS_SERVER_CERTIFICATE_RAW, SERVER_CERT),
            (TLS_SERVER_PRIVATE_KEY_RAW, SERVER_KEY),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
    );

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
    let endpoints = vec![endpoint];
    let result = task::block_on(run(&endpoints, None, &channel, &MSG_SIZE_ALL));
    assert!(result.is_ok())
}

//*************************************/
//*          Two way auth             */
//*************************************/

// Constants replicating the alert descriptions thrown by the Rustls library.
// These alert descriptions are internal of the library and cannot be reached from these tests
// as to do a proper comparison. For the sake of simplicity we verify these constants are contained
// in the expected error messages from the tests below.
//
// See: https://docs.rs/rustls/latest/src/rustls/msgs/enums.rs.html#128
const RUSTLS_HANDSHAKE_FAILURE_ALERT_DESCRIPTION: &str = "HandshakeFailure";
const RUSTLS_CERTIFICATE_REQUIRED_ALERT_DESCRIPTION: &str = "CertificateRequired";

#[cfg(all(feature = "transport_tls", target_family = "unix"))]
#[test]
fn transport_unicast_tls_two_way_auth_correct_certs_success() {
    use zenoh_link::tls::config::*;

    task::block_on(async {
        zasync_executor_init!();
    });

    let client_auth = "true";

    // Define the locator
    let mut client_endpoint: EndPoint = ("tls/localhost:10461").parse().unwrap();
    client_endpoint.extend_configuration(
        [
            (TLS_ROOT_CA_CERTIFICATE_RAW, CLIENT_CA),
            (TLS_CLIENT_CERTIFICATE_RAW, CLIENT_CERT),
            (TLS_CLIENT_PRIVATE_KEY_RAW, CLIENT_KEY),
            (TLS_CLIENT_AUTH, client_auth),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
    );

    // Define the locator
    let mut server_endpoint: EndPoint = ("tls/localhost:10461").parse().unwrap();
    server_endpoint.extend_configuration(
        [
            (TLS_ROOT_CA_CERTIFICATE_RAW, SERVER_CA),
            (TLS_SERVER_CERTIFICATE_RAW, SERVER_CERT),
            (TLS_SERVER_PRIVATE_KEY_RAW, SERVER_KEY),
            (TLS_CLIENT_AUTH, client_auth),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
    );
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
    let endpoints = vec![client_endpoint];
    let result = task::block_on(run(
        &endpoints,
        Some(&server_endpoint),
        &channel,
        &MSG_SIZE_ALL,
    ));
    assert!(result.is_ok())
}

#[cfg(all(feature = "transport_tls", target_family = "unix"))]
#[test]
fn transport_unicast_tls_two_way_auth_missing_certs_fail() {
    use zenoh_link::tls::config::*;

    task::block_on(async {
        zasync_executor_init!();
    });

    let client_auth = "true";

    // Define the locator
    let mut client_endpoint: EndPoint = ("tls/localhost:10462").parse().unwrap();
    client_endpoint.extend_configuration(
        [(TLS_ROOT_CA_CERTIFICATE_RAW, CLIENT_CA)]
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
    );

    // Define the locator
    let mut server_endpoint: EndPoint = ("tls/localhost:10462").parse().unwrap();
    server_endpoint.extend_configuration(
        [
            (TLS_ROOT_CA_CERTIFICATE_RAW, SERVER_CA),
            (TLS_SERVER_CERTIFICATE_RAW, SERVER_CERT),
            (TLS_SERVER_PRIVATE_KEY_RAW, SERVER_KEY),
            (TLS_CLIENT_AUTH, client_auth),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
    );
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
    let endpoints = vec![client_endpoint];
    let result = task::block_on(run(
        &endpoints,
        Some(&server_endpoint),
        &channel,
        &MSG_SIZE_ALL,
    ));
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err
        .to_string()
        .contains(RUSTLS_CERTIFICATE_REQUIRED_ALERT_DESCRIPTION));
}

#[cfg(all(feature = "transport_tls", target_family = "unix"))]
#[test]
fn transport_unicast_tls_two_way_auth_wrong_certs_fail() {
    use zenoh_link::tls::config::*;

    task::block_on(async {
        zasync_executor_init!();
    });

    let client_auth = "true";

    // Define the locator
    let mut client_endpoint: EndPoint = ("tls/localhost:10463").parse().unwrap();
    client_endpoint.extend_configuration(
        [
            (TLS_ROOT_CA_CERTIFICATE_RAW, CLIENT_CA),
            // Using the SERVER_CERT and SERVER_KEY in the client to simulate the case the client has
            // wrong certificates and keys. The SERVER_CA (cetificate authority) will not recognize
            // these certificates as it is expecting to receive CLIENT_CERT and CLIENT_KEY from the
            // client.
            (TLS_CLIENT_CERTIFICATE_RAW, SERVER_CERT),
            (TLS_CLIENT_PRIVATE_KEY_RAW, SERVER_KEY),
            (TLS_CLIENT_AUTH, client_auth),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
    );

    // Define the locator
    let mut server_endpoint: EndPoint = ("tls/localhost:10463").parse().unwrap();
    server_endpoint.extend_configuration(
        [
            (TLS_ROOT_CA_CERTIFICATE_RAW, SERVER_CA),
            (TLS_SERVER_CERTIFICATE_RAW, SERVER_CERT),
            (TLS_SERVER_PRIVATE_KEY_RAW, SERVER_KEY),
            (TLS_CLIENT_AUTH, client_auth),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
    );
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
    let endpoints = vec![client_endpoint];
    let result = task::block_on(run(
        &endpoints,
        Some(&server_endpoint),
        &channel,
        &MSG_SIZE_ALL,
    ));
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err
        .to_string()
        .contains(RUSTLS_HANDSHAKE_FAILURE_ALERT_DESCRIPTION));
}

//*************************************/
//*          Certificates             */
//*************************************/
//
// These keys and certificates below are purposedly generated to run both one and two way
// authentication tests.
//
// With 2 way authentication, using TLS 1.3, we need two pairs of keys and certificates: one for
// the "server" and another one for the "client".
//
// The keys and certificates below were auto-generated using https://github.com/jsha/minica and
// target the localhost domain, so it has no real mapping to any existing domain.
//
// The keys and certificates generated map as follows to the constants below:
//
//   certificates
//   ├── client
//   │   ├── localhost
//   │   │   ├── cert.pem <------- SERVER_CERT
//   │   │   └── key.pem <-------- SERVER_KEY
//   │   ├── minica-key.pem
//   │   └── minica.pem <--------- CLIENT_CA
//   └── server
//       ├── localhost
//       │   ├── cert.pem <------- CLIENT_CERT
//       │   └── key.pem <-------- CLIENT_KEY
//       ├── minica-key.pem
//       └── minica.pem <--------- SERVER_CA
//
// The way it works is that the client's certificate authority will validate the key and
// certificate brought in by the server. Similarly the server's certificate authority will validate
// the key and certificate brought in by the client.
//
const CLIENT_KEY: &str = "-----BEGIN RSA PRIVATE KEY-----
MIIEpAIBAAKCAQEArNthaKa7u/T7X5LTykqctYnmFmZcx9zUL1R7qXC/uWJWlKk6
3xvQbUs2IDeIxL3yC6djJulbrqw+XvuclCM4nMFmUB8hcidesoTRb2agapompNOH
m7pBwu4H/mKsEN72VC/aTAQfXw7X2NTlJDYG024oDd0oP44blu4MKtOUI/o/z95M
eIVvhpXl7rW4lRzGwzHgZLZxF7DsZoZE0Apdw4/wi4cs5HDlEJG2AbytjbBbDsCl
EiU6T9morYbVPL0yXmGueMY2hqyM26kbhRX+QG94DNurdPa/1HVh/of2mlc8RraX
j93h1/wb2yZpVp7348+hMu1UobP+HaaDQnFYzQIDAQABAoIBAHbedmINJtTeZ28V
/WcDYDjHF98XjX4zsgbKRjADRRUrNvcMWVvMOMywCAyno/oH1UeGWH0NxOqdsFaJ
GOgWQHwr0zwN7GYgBNMm2w+Mt4wXbbOzc3H35/kwz3Z0THddnG/QaIIV46zu/Cg0
X09Dh/Ylro26JE9wXjCwitV4oksKTxfEw8e0CsaLErvAgmITilqaGLVgZjtbnCRu
4CHInsirEAu510Js/Trp4YL4Zck4nwoZE7ORYN4lLisJptBiJbnXXgtbNKikI0MN
EUGCINL9awzOlbxAFFi2kHYQMCtVgbC7rKWotVC6HGm++VyCsOAvt7TFxssPieWh
P7Oby4ECgYEA2SWIUOACEQpHyEWvaTXHfdzVBwPUEeEyDaE9QtalfwxKkFOJUe4s
cE5G/T7Sxm8Uvep+k7wNWNC4z3c8Bm1hSU9AAw1nlTV388+fBQo4UN+O6b1Z+C3F
2c9ZAt01ymM4g/d7ovoaxZpvAZB6oHXaoRd325gkX8998X+ZIiuMVWkCgYEAy8kl
izDzaoNcZnc5Wj29gSsL3+cGKPqL6j6fQRcKe1NyOTaYTBbkx2tXwuPk4Is/QCak
pRa3uN+ujxgelPNAyr/ClPpkg82fUelXnGJabR3QaYj1ljlpYU0Bx/ZZ+eP5OWBD
cf+ipcOKnp7ykPyZo1Rk58ZJptwght+nPWO3x8UCgYBlDpaWLOpJS+OETQoJiMHC
zZdGoH19pLRKq5N7G7IBopLBAF+UBaggzA01ppspRmD80bj+wDHl951K0E7bHuR7
3aoIwaBHTI76pNF44vy6hpBYL4tDeOnvKBRgxNpXyj1vDSo4+vSiqfCnZbnsG20Y
M3fQdsnW3RXb4mo+AM5aoQKBgQCGuJ3HXT8vBVTKsLsLu5FSmWCqTxK1eJ2S6H9k
CpV1Xn8+76bTdrccVwyX3Q1snOHdyS5Drbcb01SVaP6evgnxf8BluPtGX2OaRUcU
LblWNcWYX2DsRVwzZTNuPKDTITGcCtXLwZKHP7SelLoLu9LeNWbYCzCZzSD7yVPI
s+nFeQKBgQCtQZpVRvO0Wyyf0wcP1dBH65No3S3tmELB70nPkt0S1Vt2G7LIRXzQ
Fz66VkcYPMu05ggFIzrsJqPK6LUCb+h8sYnN4464+cJXhIrPvRJ3Pu30NoVBB0yz
AbQCDGFMp3XCC+FLajMdQQhuXfUfSGjbidhQI87hmMF+gZ+WkNc8+A==
-----END RSA PRIVATE KEY-----";

const CLIENT_CERT: &str = "-----BEGIN CERTIFICATE-----
MIIDLDCCAhSgAwIBAgIIdTp0cmbVlKswDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgN2M1NWNjMB4XDTIyMTEwOTE1MzkwNVoXDTI0MTIw
OTE1MzkwNVowFDESMBAGA1UEAxMJbG9jYWxob3N0MIIBIjANBgkqhkiG9w0BAQEF
AAOCAQ8AMIIBCgKCAQEArNthaKa7u/T7X5LTykqctYnmFmZcx9zUL1R7qXC/uWJW
lKk63xvQbUs2IDeIxL3yC6djJulbrqw+XvuclCM4nMFmUB8hcidesoTRb2agapom
pNOHm7pBwu4H/mKsEN72VC/aTAQfXw7X2NTlJDYG024oDd0oP44blu4MKtOUI/o/
z95MeIVvhpXl7rW4lRzGwzHgZLZxF7DsZoZE0Apdw4/wi4cs5HDlEJG2AbytjbBb
DsClEiU6T9morYbVPL0yXmGueMY2hqyM26kbhRX+QG94DNurdPa/1HVh/of2mlc8
RraXj93h1/wb2yZpVp7348+hMu1UobP+HaaDQnFYzQIDAQABo3YwdDAOBgNVHQ8B
Af8EBAMCBaAwHQYDVR0lBBYwFAYIKwYBBQUHAwEGCCsGAQUFBwMCMAwGA1UdEwEB
/wQCMAAwHwYDVR0jBBgwFoAU5NmLIitdZkkwN0eVDK/Nan+Y2u0wFAYDVR0RBA0w
C4IJbG9jYWxob3N0MA0GCSqGSIb3DQEBCwUAA4IBAQBjAKt/2rCmLGg7Ue6W+lz4
GDfgKIxAiTZNeIys/Eq7NuPQsJfKFZ4H2NZGcrJ+eEh/gOzuFkGW5HTO9gt1SQ+g
pRtwFM+qxiVsARBcbamx8+VQ/Y7caZ35RRfllSc3I7NDl4uDjDvxYZcrpftFS8Hg
kSLD01Q1hOYIf2QYznLoePX2dSYrQnDmE+bEkMB/yQ57bdAfKwkpNKsWOhSHsusZ
FnK0IPRdnOl5v3j/62DDBllnJER5aahQcbNx9WszP2ZZb/SNzzQghVJ8yWBrbAJn
SRCU86jw504Zx5q/SbuXJsPXJbiFF7eclvKEumdF3XmJeMRGPg2ysQ/nfco0nBz8
-----END CERTIFICATE-----";

const CLIENT_CA: &str = "-----BEGIN CERTIFICATE-----
MIIDSzCCAjOgAwIBAgIIC3MWFI+HOvowDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgMGI3MzE2MCAXDTIyMTAyNDEzMDIwNloYDzIxMjIx
MDI0MTMwMjA2WjAgMR4wHAYDVQQDExVtaW5pY2Egcm9vdCBjYSAwYjczMTYwggEi
MA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQDUfR2f6aY4VrCt1QSyM/YHnZ6m
FNbZhhWDTo2iiWrm5J7iXnxvrxZR/kyIWm5KNfjSqY7BO2zgvK6iqSzhPZCQ7+Gk
wCQE7CcB/rIp+w4S/+MGdQp3IOU11DDsEPyCVgvsWtS0G9sTzzgmTxoO6iRRAqYb
fQ9X6PIEGCxCKMsqjkLi41lGq4Ta1jVdYfcKSIfYkkF4Newi3YbKZdmxpqReQWt6
6L1vsIAXsN5v4J1wpVLY19krosFhstHccMRIYMMXb5nRx+1VCvomCJaZqvefQisa
lgFMHpvTdnUlkuyEeCz8MNczQEN2T5Ggt+/QccVHl57R890bTGMxKccRLRlvAgMB
AAGjgYYwgYMwDgYDVR0PAQH/BAQDAgKEMB0GA1UdJQQWMBQGCCsGAQUFBwMBBggr
BgEFBQcDAjASBgNVHRMBAf8ECDAGAQH/AgEAMB0GA1UdDgQWBBTXWRqODgE/1z1t
2uAj+1cFIdkbcTAfBgNVHSMEGDAWgBTXWRqODgE/1z1t2uAj+1cFIdkbcTANBgkq
hkiG9w0BAQsFAAOCAQEAy3wljz8FEdfn4M09AKmoMU/cQQsV5CvozTW6kIqb8du7
J6OrVWWQQ/5OLCUcGB9uxBVusuPk8y7Al1bTnNST9HScwFeJ4GCirtOR1PJsQE0z
w5nj1IKEpZUa2kiPlyf0NjONd3bNgT37ULx9nHucBTmvWwS7G0QOXPvFvIxTz2oU
yzLeR60HKUulCVzt0UuGH86eN3ym4XeBDurC98sd/COM/3g30LRzRQfm1NagZ46T
05HDlnkTEqeU9yYl/c7PK20XR0fRg3lBd3d9cjJwq4u/oO6lIrMOSdn61oI9mrVQ
5GkQ3waq7e0CNrbxXnQHgnMrIhD8Te4gSDXswoLgaw==
-----END CERTIFICATE-----";

const SERVER_KEY: &str = "-----BEGIN RSA PRIVATE KEY-----
MIIEpAIBAAKCAQEA0eRJBZe8MzIPBrSZ+2/yLfqFON/QdeJPTu1Oug22fVOlAgG5
c06qCyurwqB5fWyt61m6ZKUJuWI3qZy8y8cnRFv3WKILq6jWBYxN1z3lHiLM199R
GgfHPZdjqUMF9h6q5XilGDa9aydMIIr6iE7PXEcEUSFpGZN2ErVMC1eBeyHLzcMO
LUcG2/GcmjH8+9jGVsk1abcdTKv9ETpjM5ztjE/IDRME3XlJaKQANZQyWaHLH4pa
/HwbKiMfiQ8FQT9SpXsFMERrvMpnQkzaCIW/SRNBlKb2fwQSafqkPWbgp5vf83TP
ML0l1x633rFhSqcnJAU5WIAmapFG11jh72sFJwIDAQABAoIBAQCxYBK1vz00pqE8
MXPPoRMw9/2NytcISHBtau6VHPGTiBRyVbK7V0csmYNVvvfsnuN7eSCj3TUSjYYs
uGB0daEhi/bD2G20a8IyfhdqRsxRY2dpJzgKn3go/L8kU5e+HrydoA8lH12EKHmV
Jt4CQ1fJy9pCFdIT4yJtPPk+vHyX6Lc8fM2fJdlaERb2mLEVUuyf6rB7U4pKVVYU
sW2USZrAWsxI6VTaT9BLBWZHDCEBk3MwxNy6Qrwvd2PXwUVDKS7jplj62SgbgV9w
UfvkRWFp3vkQadbBsAGUorIfp8GnRDY1KcO9Gw3fdwZe57sHLn9sei0iFu+IPUsW
STvUaAOBAoGBANrmXUwXeqUdte//e62fb0VK8etRuYPHhHiT+Su2XN/MWYDA+5uM
4NYQo5qY3ErwRIifKO3tNAyWOQlKvJZuo8tot4xX3nZHP79b2UrqpyL0i1an2pzs
5maQmTfC9Gc/LFHS/PU9w5Y2+MUID0PfwuCV2/8t50kCOtJfq5O5Sqd/AoGBAPV3
E3jQJwNWkmuZH6mPWXvIP337uH+btnkqIUtIfHGLgFVn34K9Lcsd62MmrEFtVlwb
nWEVM1HBya2/PcagGj44VsWRzAq8rkoKrvap5mx737owyaeBJgzbw6429AounJs1
898+Alkbq3MpjBElVCQdaDsIvkeAdlCVBIb73jZZAoGAAutTjzI49n7A8GRt19Dq
gPgQ5dx/JtzATYNbrVOPRYTKJMduE5L7ZJ9wLx2ewnkV0OSefR3OteRC+na+sRrk
oE/TMtHxK46jsP+elDsw42xzd0Jhzfny0KdZA79b1wymoKi5quOZ+iTdiHMlEPio
9qnI90w7a2PWOPwBo8Sy1C0CgYEA22OtPKrWY65puc+nM/aSpQbKcMCeGzfCNLNK
BK5pw1ZKworPg1uwZT19mCYFiYi+yh5IYHABaU5KAofOIAwSyI+0RmtUMjiHklfQ
H1ilQUrKIPDgG11b89wsHjaxkbQtdrAXIu2aTahkac61iNGTTaAW+8SJxQB1Pvqh
jD/rUSkCgYALMLo/QvTFty6io8jgb6+LY1cKrerl6NQ3R/9ciWsvXmOwbITPGaqq
sXX6rPkNHWyymzWhvjxI/eP8d9FzbcvaEfx9dQRYBbcduxGeVq/+pVtgW5OY4L4C
fNEjJmiv2pXIRoMpfAI5Yg6tdeO3G2glLsv3+1Op+OnuNOicVuo+jg==
-----END RSA PRIVATE KEY-----";

const SERVER_CERT: &str = "-----BEGIN CERTIFICATE-----
MIIDLDCCAhSgAwIBAgIIbMxjSdRKLkkwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgMGI3MzE2MB4XDTIyMTAyNDEzMDIwNloXDTI0MTEy
MzE0MDIwNlowFDESMBAGA1UEAxMJbG9jYWxob3N0MIIBIjANBgkqhkiG9w0BAQEF
AAOCAQ8AMIIBCgKCAQEA0eRJBZe8MzIPBrSZ+2/yLfqFON/QdeJPTu1Oug22fVOl
AgG5c06qCyurwqB5fWyt61m6ZKUJuWI3qZy8y8cnRFv3WKILq6jWBYxN1z3lHiLM
199RGgfHPZdjqUMF9h6q5XilGDa9aydMIIr6iE7PXEcEUSFpGZN2ErVMC1eBeyHL
zcMOLUcG2/GcmjH8+9jGVsk1abcdTKv9ETpjM5ztjE/IDRME3XlJaKQANZQyWaHL
H4pa/HwbKiMfiQ8FQT9SpXsFMERrvMpnQkzaCIW/SRNBlKb2fwQSafqkPWbgp5vf
83TPML0l1x633rFhSqcnJAU5WIAmapFG11jh72sFJwIDAQABo3YwdDAOBgNVHQ8B
Af8EBAMCBaAwHQYDVR0lBBYwFAYIKwYBBQUHAwEGCCsGAQUFBwMCMAwGA1UdEwEB
/wQCMAAwHwYDVR0jBBgwFoAU11kajg4BP9c9bdrgI/tXBSHZG3EwFAYDVR0RBA0w
C4IJbG9jYWxob3N0MA0GCSqGSIb3DQEBCwUAA4IBAQDAaUqQdKGZcqAioKKlrpOF
jezt+JUo8WQpe7gJkF32U8ctRefx3gtZVI2D3ls+nlj7XPmoD8/r4Hq6njY7m893
1BPgiw8XChletjnCx4oCqHj+3dnsMib23b+oPIgGdfAJWFVMUgj9AaXYbenYt9kK
u+J2gL0l1Z/SI2peAmTbGCzVPEgxx7UNfRwRI4Dq1C+D/5xe7W8GhA3AVbPjHF/l
iPOFtCHc3026/D2xBYwApHMK0d3ATO0yk9z+T09e657g5fGVeaNCwuTuwlRXJZGW
U+sf37D2NmcSgVuhx0k4BP8eAyBbD/fDDqrmEFXC4yvhxxTi8DNJswCa54nUzlUg
-----END CERTIFICATE-----";

const SERVER_CA: &str = "-----BEGIN CERTIFICATE-----
MIIDSzCCAjOgAwIBAgIIfFXM74pJVUcwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgN2M1NWNjMCAXDTIyMTEwOTE1MzkwNVoYDzIxMjIx
MTA5MTUzOTA1WjAgMR4wHAYDVQQDExVtaW5pY2Egcm9vdCBjYSA3YzU1Y2MwggEi
MA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQC/lxcrUNTzQsFKy6/0WSZGm80t
pXWWKhQP1a0mmHvQdyJL/7TbbkVh64/zFZgOyTKWCo/1DtdVlAvJ46pBn1OpNdtl
gQ53ZuQmsecN7swJfVAwUyLqFSw4o7ICE+HlpeqCdMZbYw8vdq/JQHiElV1Ev3OB
JZngFq6llA2xdsefzQ3i/YdtKvU7P9vcGjP9s2ITG3NTgbD+NvJodt88D1pcYJ9b
Df7g3veE0IUv9vZglPGef4Kwzuwin85CjHpJEd+yHIpDynwdhOlRgs1YK4YSs58n
vFk5HKQMgdsEBdradX/Pnl9fIpk7iShuDE7/NMez2C02LdpcEYM0wA4JuPmHAgMB
AAGjgYYwgYMwDgYDVR0PAQH/BAQDAgKEMB0GA1UdJQQWMBQGCCsGAQUFBwMBBggr
BgEFBQcDAjASBgNVHRMBAf8ECDAGAQH/AgEAMB0GA1UdDgQWBBTk2YsiK11mSTA3
R5UMr81qf5ja7TAfBgNVHSMEGDAWgBTk2YsiK11mSTA3R5UMr81qf5ja7TANBgkq
hkiG9w0BAQsFAAOCAQEAvKaSjH+X6qOi/HgCGytbf4HA0owCF6IBSYun4i4lYjiD
M5ItDxdNunZXSFA7JkISnPmjJxKHDcdBOL6L3PxaHwLpHeDb1zxtXiS7ggPYZnC/
gwtIV9oRgecLgq3t+nNTRBtRqiZcZgYxKN2hXyEBaqFCnUaCzd3pKqRBZm4QLDk3
Rl6XgFuZYXo07q5B9rLnFcIDAJ8Eu4I4J/Hk/QzGC5XJJfvaFpcSl4Z6nBAXIJBY
KgGo0jSCkkGSDLBDrM9O9LGqIGA+Jh/QbfafsjN6UHxSQB0tFFCUUgHf62bvv085
9eRVFKlA/lFduXBmpcbSr07txDV6ujkpyA3WCXINmw==
-----END CERTIFICATE-----";
