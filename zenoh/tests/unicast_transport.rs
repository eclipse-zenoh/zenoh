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
use zenoh_util::properties::Properties;
use zenoh_util::zasync_executor_init;

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
    endpoints: &[EndPoint],
) -> (
    TransportManager,
    Arc<SHRouter>,
    TransportManager,
    TransportUnicast,
) {
    // Define client and router IDs
    let client_id = ZenohId::new(1, [0_u8; ZenohId::MAX_SIZE]);
    let router_id = ZenohId::new(1, [1_u8; ZenohId::MAX_SIZE]);

    // Create the router transport manager
    let router_handler = Arc::new(SHRouter::default());
    let unicast = TransportManager::config_unicast().max_links(endpoints.len());

    let router_manager = TransportManager::builder()
        .zid(router_id)
        .whatami(WhatAmI::Router)
        .unicast(unicast)
        .build(router_handler.clone())
        .unwrap();

    // Create the client transport manager
    let unicast = TransportManager::config_unicast().max_links(endpoints.len());
    let client_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client_id)
        .unicast(unicast)
        .build(Arc::new(SHClient::default()))
        .unwrap();

    // Create the listener on the router
    for e in endpoints.iter() {
        println!("Add endpoint: {}", e);
        let _ = ztimeout!(router_manager.add_listener(e.clone())).unwrap();
    }

    // Create an empty transport with the client
    // Open transport -> This should be accepted
    for e in endpoints.iter() {
        println!("Opening transport with {}", e);
        let _ = ztimeout!(client_manager.open_transport(e.clone())).unwrap();
    }

    let client_transport = client_manager.get_transport(&router_id).unwrap();

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
    let mut ee = "".to_string();
    for e in endpoints.iter() {
        ee.push_str(&format!("{} ", e));
    }
    println!("Closing transport with {}", ee);
    let _ = ztimeout!(client_transport.close()).unwrap();

    ztimeout!(async {
        while !router_manager.get_transports().is_empty() {
            task::sleep(SLEEP).await;
        }
    });

    // Stop the locators on the manager
    for e in endpoints.iter() {
        println!("Del locator: {}", e);
        let _ = ztimeout!(router_manager.del_listener(e)).unwrap();
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

async fn run_single(endpoints: &[EndPoint], channel: Channel, msg_size: usize) {
    #[allow(unused_variables)] // Used when stats feature is enabled
    let (router_manager, router_handler, client_manager, client_transport) =
        open_transport(endpoints).await;

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
            .get_transport_unicast(&client_manager.config.pid)
            .unwrap()
            .get_stats()
            .unwrap();
        println!("\tRouter: {:?}", r_stats);
    }

    close_transport(router_manager, client_manager, client_transport, endpoints).await;
}

async fn run(endpoints: &[EndPoint], channel: &[Channel], msg_size: &[usize]) {
    for ch in channel.iter() {
        for ms in msg_size.iter() {
            run_single(endpoints, *ch, *ms).await;
        }
    }
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
    task::block_on(run(&endpoints, &channel, &MSG_SIZE_ALL));
    let _ = std::fs::remove_file("zenoh-test-unix-socket-5.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-5.sock.lock");
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
    task::block_on(run(&endpoints, &channel, &MSG_SIZE_NOFRAG));
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
    task::block_on(run(&endpoints, &channel, &MSG_SIZE_ALL));
    let _ = std::fs::remove_file("zenoh-test-unix-socket-6.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-6.sock.lock");
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
    task::block_on(run(&endpoints, &channel, &MSG_SIZE_NOFRAG));
    let _ = std::fs::remove_file("zenoh-test-unix-socket-7.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-7.sock.lock");
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
    task::block_on(run(&endpoints, &channel, &MSG_SIZE_NOFRAG));
    let _ = std::fs::remove_file("zenoh-test-unix-socket-8.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-8.sock.lock");
}

#[cfg(all(feature = "transport_tls", target_family = "unix"))]
#[test]
fn transport_unicast_tls_only() {
    use zenoh::net::link::tls::config::*;

    task::block_on(async {
        zasync_executor_init!();
    });

    // NOTE: this an auto-generated pair of certificate and key.
    //       The target domain is localhost, so it has no real
    //       mapping to any existing domain. The certificate and key
    //       have been generated using: https://github.com/jsha/minica
    let key = "-----BEGIN RSA PRIVATE KEY-----
MIIEowIBAAKCAQEAz105EYUbOdW5uJ8o/TqtxtOtKJL7AQdy5yiXoslosAsulaew
4JSJetVa6Fa6Bq5BK6fsphGD9bpGGeiBZFBt75JRjOrkj4DwlLGa0CPLTgG5hul4
Ufe9B7VG3J5P8OwUqIYmPzj8uTbNtkgFRcYumHR28h4GkYdG5Y04AV4vIjgKE47j
AgV5ACRHkcmGrTzF2HOes2wT73l4yLSkKR4GlIWu5cLRdI8PTUmjMFAh/GIh1ahd
+VqXz051V3jok0n1klVNjc6DnWuH3j/MSOg/52C3YfcUjCeIJGVfcqDnPTJKSNEF
yVTYCUjWy+B0B4fMz3MpU17dDWpvS5hfc4VrgQIDAQABAoIBAQCq+i208XBqdnwk
6y7r5Tcl6qErBE3sIk0upjypX7Ju/TlS8iqYckENQ+AqFGBcY8+ehF5O68BHm2hz
sk8F/H84+wc8zuzYGjPEFtEUb38RecCUqeqog0Gcmm6sN+ioOLAr6DifBojy2mox
sx6N0oPW9qigp/s4gTcGzTLxhcwNRHWuoWjQwq6y6qwt2PJXnllii5B5iIJhKAxE
EOmcVCmFbPavQ1Xr9F5jd5rRc1TYq28hXX8dZN2JhdVUbLlHzaiUfTnA/8yI4lyq
bEmqu29Oqe+CmDtB6jRnrLiIwyZxzXKuxXaO6NqgxqtaVjLcdISEgZMeHEftuOtf
C1xxodaVAoGBAOb1Y1SvUGx+VADSt1d30h3bBm1kU/1LhLKZOAQrnFMrEfyOfYbz
AZ4FJgXE6ZsB1BA7hC0eJDVHz8gTgDJQrOOO8WJWDGRe4TbZkCi5IizYg5UH/6az
I/WKlfdA4j1tftbQhycHL+9bGzdoRzrwIK489PG4oVAJJCaK2CVtx+l3AoGBAOXY
75sHOiMaIvDA7qlqFbaBkdi1NzH7bCgy8IntNfLxlOCmGjxeNZzKrkode3JWY9SI
Mo/nuWj8EZBEHj5omCapzOtkW/Nhnzc4C6U3BCspdrQ4mzbmzEGTdhqvxepa7U7K
iRcoD1iU7kINCEwg2PsB/BvCSrkn6lpIJlYXlJDHAoGAY7QjgXd9fJi8ou5Uf8oW
RxU6nRbmuz5Sttc2O3aoMa8yQJkyz4Mwe4s1cuAjCOutJKTM1r1gXC/4HyNsAEyb
llErG4ySJPJgv1EEzs+9VSbTBw9A6jIDoAiH3QmBoYsXapzy+4I6y1XFVhIKTgND
2HQwOfm+idKobIsb7GyMFNkCgYBIsixWZBrHL2UNsHfLrXngl2qBmA81B8hVjob1
mMkPZckopGB353Qdex1U464/o4M/nTQgv7GsuszzTBgktQAqeloNuVg7ygyJcnh8
cMIoxJx+s8ijvKutse4Q0rdOQCP+X6CsakcwRSp2SZjuOxVljmMmhHUNysocc+Vs
JVkf0QKBgHiCVLU60EoPketADvhRJTZGAtyCMSb3q57Nb0VIJwxdTB5KShwpul1k
LPA8Z7Y2i9+IEXcPT0r3M+hTwD7noyHXNlNuzwXot4B8PvbgKkMLyOpcwBjppJd7
ns4PifoQbhDFnZPSfnrpr+ZXSEzxtiyv7Ql69jznl/vB8b75hBL4
-----END RSA PRIVATE KEY-----";

    let cert = "-----BEGIN CERTIFICATE-----
MIIDLDCCAhSgAwIBAgIIIXlwQVKrtaAwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgMmJiOTlkMB4XDTIxMDIwMjE0NDYzNFoXDTIzMDMw
NDE0NDYzNFowFDESMBAGA1UEAxMJbG9jYWxob3N0MIIBIjANBgkqhkiG9w0BAQEF
AAOCAQ8AMIIBCgKCAQEAz105EYUbOdW5uJ8o/TqtxtOtKJL7AQdy5yiXoslosAsu
laew4JSJetVa6Fa6Bq5BK6fsphGD9bpGGeiBZFBt75JRjOrkj4DwlLGa0CPLTgG5
hul4Ufe9B7VG3J5P8OwUqIYmPzj8uTbNtkgFRcYumHR28h4GkYdG5Y04AV4vIjgK
E47jAgV5ACRHkcmGrTzF2HOes2wT73l4yLSkKR4GlIWu5cLRdI8PTUmjMFAh/GIh
1ahd+VqXz051V3jok0n1klVNjc6DnWuH3j/MSOg/52C3YfcUjCeIJGVfcqDnPTJK
SNEFyVTYCUjWy+B0B4fMz3MpU17dDWpvS5hfc4VrgQIDAQABo3YwdDAOBgNVHQ8B
Af8EBAMCBaAwHQYDVR0lBBYwFAYIKwYBBQUHAwEGCCsGAQUFBwMCMAwGA1UdEwEB
/wQCMAAwHwYDVR0jBBgwFoAULXa6lBiO7OLL5Z6XuF5uF5wR9PQwFAYDVR0RBA0w
C4IJbG9jYWxob3N0MA0GCSqGSIb3DQEBCwUAA4IBAQBOMkNXfzPEDU475zbiSi3v
JOhpZLyuoaYY62RzZc9VF8YRybJlWKUWdR3szAiUd1xCJe/beNX7b9lPg6wNadKq
DGTWFmVxSfpVMO9GQYBXLDcNaAUXzsDLC5sbAFST7jkAJELiRn6KtQYxZ2kEzo7G
QmzNMfNMc1KeL8Qr4nfEHZx642yscSWj9edGevvx4o48j5KXcVo9+pxQQFao9T2O
F5QxyGdov+uNATWoYl92Gj8ERi7ovHimU3H7HLIwNPqMJEaX4hH/E/Oz56314E9b
AXVFFIgCSluyrolaD6CWD9MqOex4YOfJR2bNxI7lFvuK4AwjyUJzT1U1HXib17mM
-----END CERTIFICATE-----";

    // Configure the client
    let ca = "-----BEGIN CERTIFICATE-----
MIIDSzCCAjOgAwIBAgIIK7mduKtTVxkwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgMmJiOTlkMCAXDTIxMDIwMjEzMTc0NVoYDzIxMjEw
MjAyMTMxNzQ1WjAgMR4wHAYDVQQDExVtaW5pY2Egcm9vdCBjYSAyYmI5OWQwggEi
MA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQCoBZOxIfVq7LoEpVCMlQzuDnFy
d+yuk5pFasEQvZ3IvWVta4rPFJ3WGl4UNF6v9bZegNHp+oo70guZ8ps9ez34qrwB
rrNtZ0YJLDvR0ygloinZZeiclrZcu+x9vRdnyfWqrAulJBMlJIbbHcNx2OCkq7MM
HdpLJMXxKVbIlQQYGUzRkNTAaK2PiFX5BaqmnZZyo7zNbz7L2asg+0K/FpiS2IRA
coHPTa9BtsLUJUPRHPr08pgTjM1MQwa+Xxg1+wtMh85xdrqMi6Oe0cxefS+0L04F
KVfMD3bW8AyuugvcTEpGnea2EvMoPfLWpnPGU3XO8lRZyotZDQzrPvNyYKM3AgMB
AAGjgYYwgYMwDgYDVR0PAQH/BAQDAgKEMB0GA1UdJQQWMBQGCCsGAQUFBwMBBggr
BgEFBQcDAjASBgNVHRMBAf8ECDAGAQH/AgEAMB0GA1UdDgQWBBQtdrqUGI7s4svl
npe4Xm4XnBH09DAfBgNVHSMEGDAWgBQtdrqUGI7s4svlnpe4Xm4XnBH09DANBgkq
hkiG9w0BAQsFAAOCAQEAJliEt607VUOSDsUeabhG8MIhYDhxe+mjJ4i7N/0xk9JU
piCUdQr26HyYCzN+bNdjw663rxuVGtTTdHSw2CJHsPSOEDinbYkLMSyDeomsnr0S
4e0hKUeqXXYg0iC/O2283ZEvvQK5SE+cjm0La0EmqO0mj3Mkc4Fsg8hExYuOur4M
M0AufDKUhroksKKiCmjsFj1x55VcU45Ag8069lzBk7ntcGQpHUUkwZzvD4FXf8IR
pVVHiH6WC99p77T9Di99dE5ufjsprfbzkuafgTo2Rz03HgPq64L4po/idP8uBMd6
tOzot3pwe+3SJtpk90xAQrABEO0Zh2unrC8i83ySfg==
-----END CERTIFICATE-----";

    // Define the locator
    let mut endpoint: EndPoint = "tls/localhost:10451".parse().unwrap();
    let mut config = Properties::default();
    config.insert(TLS_ROOT_CA_CERTIFICATE_RAW.to_string(), ca.to_string());
    config.insert(TLS_SERVER_PRIVATE_KEY_RAW.to_string(), key.to_string());
    config.insert(TLS_SERVER_CERTIFICATE_RAW.to_string(), cert.to_string());
    endpoint.config = Some(Arc::new(config));

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
    task::block_on(run(&endpoints, &channel, &MSG_SIZE_ALL));
}

#[cfg(feature = "transport_quic")]
#[test]
fn transport_unicast_quic_only() {
    use zenoh::net::link::quic::config::*;

    task::block_on(async {
        zasync_executor_init!();
    });
    // NOTE: this an auto-generated pair of certificate and key.
    //       The target domain is localhost, so it has no real
    //       mapping to any existing domain. The certificate and key
    //       have been generated using: https://github.com/jsha/minica
    let key = "-----BEGIN RSA PRIVATE KEY-----
MIIEowIBAAKCAQEAz105EYUbOdW5uJ8o/TqtxtOtKJL7AQdy5yiXoslosAsulaew
4JSJetVa6Fa6Bq5BK6fsphGD9bpGGeiBZFBt75JRjOrkj4DwlLGa0CPLTgG5hul4
Ufe9B7VG3J5P8OwUqIYmPzj8uTbNtkgFRcYumHR28h4GkYdG5Y04AV4vIjgKE47j
AgV5ACRHkcmGrTzF2HOes2wT73l4yLSkKR4GlIWu5cLRdI8PTUmjMFAh/GIh1ahd
+VqXz051V3jok0n1klVNjc6DnWuH3j/MSOg/52C3YfcUjCeIJGVfcqDnPTJKSNEF
yVTYCUjWy+B0B4fMz3MpU17dDWpvS5hfc4VrgQIDAQABAoIBAQCq+i208XBqdnwk
6y7r5Tcl6qErBE3sIk0upjypX7Ju/TlS8iqYckENQ+AqFGBcY8+ehF5O68BHm2hz
sk8F/H84+wc8zuzYGjPEFtEUb38RecCUqeqog0Gcmm6sN+ioOLAr6DifBojy2mox
sx6N0oPW9qigp/s4gTcGzTLxhcwNRHWuoWjQwq6y6qwt2PJXnllii5B5iIJhKAxE
EOmcVCmFbPavQ1Xr9F5jd5rRc1TYq28hXX8dZN2JhdVUbLlHzaiUfTnA/8yI4lyq
bEmqu29Oqe+CmDtB6jRnrLiIwyZxzXKuxXaO6NqgxqtaVjLcdISEgZMeHEftuOtf
C1xxodaVAoGBAOb1Y1SvUGx+VADSt1d30h3bBm1kU/1LhLKZOAQrnFMrEfyOfYbz
AZ4FJgXE6ZsB1BA7hC0eJDVHz8gTgDJQrOOO8WJWDGRe4TbZkCi5IizYg5UH/6az
I/WKlfdA4j1tftbQhycHL+9bGzdoRzrwIK489PG4oVAJJCaK2CVtx+l3AoGBAOXY
75sHOiMaIvDA7qlqFbaBkdi1NzH7bCgy8IntNfLxlOCmGjxeNZzKrkode3JWY9SI
Mo/nuWj8EZBEHj5omCapzOtkW/Nhnzc4C6U3BCspdrQ4mzbmzEGTdhqvxepa7U7K
iRcoD1iU7kINCEwg2PsB/BvCSrkn6lpIJlYXlJDHAoGAY7QjgXd9fJi8ou5Uf8oW
RxU6nRbmuz5Sttc2O3aoMa8yQJkyz4Mwe4s1cuAjCOutJKTM1r1gXC/4HyNsAEyb
llErG4ySJPJgv1EEzs+9VSbTBw9A6jIDoAiH3QmBoYsXapzy+4I6y1XFVhIKTgND
2HQwOfm+idKobIsb7GyMFNkCgYBIsixWZBrHL2UNsHfLrXngl2qBmA81B8hVjob1
mMkPZckopGB353Qdex1U464/o4M/nTQgv7GsuszzTBgktQAqeloNuVg7ygyJcnh8
cMIoxJx+s8ijvKutse4Q0rdOQCP+X6CsakcwRSp2SZjuOxVljmMmhHUNysocc+Vs
JVkf0QKBgHiCVLU60EoPketADvhRJTZGAtyCMSb3q57Nb0VIJwxdTB5KShwpul1k
LPA8Z7Y2i9+IEXcPT0r3M+hTwD7noyHXNlNuzwXot4B8PvbgKkMLyOpcwBjppJd7
ns4PifoQbhDFnZPSfnrpr+ZXSEzxtiyv7Ql69jznl/vB8b75hBL4
-----END RSA PRIVATE KEY-----";

    let cert = "-----BEGIN CERTIFICATE-----
MIIDLDCCAhSgAwIBAgIIIXlwQVKrtaAwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgMmJiOTlkMB4XDTIxMDIwMjE0NDYzNFoXDTIzMDMw
NDE0NDYzNFowFDESMBAGA1UEAxMJbG9jYWxob3N0MIIBIjANBgkqhkiG9w0BAQEF
AAOCAQ8AMIIBCgKCAQEAz105EYUbOdW5uJ8o/TqtxtOtKJL7AQdy5yiXoslosAsu
laew4JSJetVa6Fa6Bq5BK6fsphGD9bpGGeiBZFBt75JRjOrkj4DwlLGa0CPLTgG5
hul4Ufe9B7VG3J5P8OwUqIYmPzj8uTbNtkgFRcYumHR28h4GkYdG5Y04AV4vIjgK
E47jAgV5ACRHkcmGrTzF2HOes2wT73l4yLSkKR4GlIWu5cLRdI8PTUmjMFAh/GIh
1ahd+VqXz051V3jok0n1klVNjc6DnWuH3j/MSOg/52C3YfcUjCeIJGVfcqDnPTJK
SNEFyVTYCUjWy+B0B4fMz3MpU17dDWpvS5hfc4VrgQIDAQABo3YwdDAOBgNVHQ8B
Af8EBAMCBaAwHQYDVR0lBBYwFAYIKwYBBQUHAwEGCCsGAQUFBwMCMAwGA1UdEwEB
/wQCMAAwHwYDVR0jBBgwFoAULXa6lBiO7OLL5Z6XuF5uF5wR9PQwFAYDVR0RBA0w
C4IJbG9jYWxob3N0MA0GCSqGSIb3DQEBCwUAA4IBAQBOMkNXfzPEDU475zbiSi3v
JOhpZLyuoaYY62RzZc9VF8YRybJlWKUWdR3szAiUd1xCJe/beNX7b9lPg6wNadKq
DGTWFmVxSfpVMO9GQYBXLDcNaAUXzsDLC5sbAFST7jkAJELiRn6KtQYxZ2kEzo7G
QmzNMfNMc1KeL8Qr4nfEHZx642yscSWj9edGevvx4o48j5KXcVo9+pxQQFao9T2O
F5QxyGdov+uNATWoYl92Gj8ERi7ovHimU3H7HLIwNPqMJEaX4hH/E/Oz56314E9b
AXVFFIgCSluyrolaD6CWD9MqOex4YOfJR2bNxI7lFvuK4AwjyUJzT1U1HXib17mM
-----END CERTIFICATE-----";

    // Configure the client
    let ca = "-----BEGIN CERTIFICATE-----
MIIDSzCCAjOgAwIBAgIIK7mduKtTVxkwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgMmJiOTlkMCAXDTIxMDIwMjEzMTc0NVoYDzIxMjEw
MjAyMTMxNzQ1WjAgMR4wHAYDVQQDExVtaW5pY2Egcm9vdCBjYSAyYmI5OWQwggEi
MA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQCoBZOxIfVq7LoEpVCMlQzuDnFy
d+yuk5pFasEQvZ3IvWVta4rPFJ3WGl4UNF6v9bZegNHp+oo70guZ8ps9ez34qrwB
rrNtZ0YJLDvR0ygloinZZeiclrZcu+x9vRdnyfWqrAulJBMlJIbbHcNx2OCkq7MM
HdpLJMXxKVbIlQQYGUzRkNTAaK2PiFX5BaqmnZZyo7zNbz7L2asg+0K/FpiS2IRA
coHPTa9BtsLUJUPRHPr08pgTjM1MQwa+Xxg1+wtMh85xdrqMi6Oe0cxefS+0L04F
KVfMD3bW8AyuugvcTEpGnea2EvMoPfLWpnPGU3XO8lRZyotZDQzrPvNyYKM3AgMB
AAGjgYYwgYMwDgYDVR0PAQH/BAQDAgKEMB0GA1UdJQQWMBQGCCsGAQUFBwMBBggr
BgEFBQcDAjASBgNVHRMBAf8ECDAGAQH/AgEAMB0GA1UdDgQWBBQtdrqUGI7s4svl
npe4Xm4XnBH09DAfBgNVHSMEGDAWgBQtdrqUGI7s4svlnpe4Xm4XnBH09DANBgkq
hkiG9w0BAQsFAAOCAQEAJliEt607VUOSDsUeabhG8MIhYDhxe+mjJ4i7N/0xk9JU
piCUdQr26HyYCzN+bNdjw663rxuVGtTTdHSw2CJHsPSOEDinbYkLMSyDeomsnr0S
4e0hKUeqXXYg0iC/O2283ZEvvQK5SE+cjm0La0EmqO0mj3Mkc4Fsg8hExYuOur4M
M0AufDKUhroksKKiCmjsFj1x55VcU45Ag8069lzBk7ntcGQpHUUkwZzvD4FXf8IR
pVVHiH6WC99p77T9Di99dE5ufjsprfbzkuafgTo2Rz03HgPq64L4po/idP8uBMd6
tOzot3pwe+3SJtpk90xAQrABEO0Zh2unrC8i83ySfg==
-----END CERTIFICATE-----";

    // Define the locator
    let mut endpoint: EndPoint = "quic/localhost:10452".parse().unwrap();
    let mut config = Properties::default();
    config.insert(TLS_ROOT_CA_CERTIFICATE_RAW.to_string(), ca.to_string());
    config.insert(TLS_SERVER_PRIVATE_KEY_RAW.to_string(), key.to_string());
    config.insert(TLS_SERVER_CERTIFICATE_RAW.to_string(), cert.to_string());
    endpoint.config = Some(Arc::new(config));

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
    task::block_on(run(&endpoints, &channel, &MSG_SIZE_ALL));
}
