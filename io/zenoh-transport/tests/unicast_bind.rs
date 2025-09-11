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
use zenoh_link::EndPoint;
use zenoh_protocol::core::{Address, WhatAmI, ZenohIdProto};
use zenoh_result::ZResult;
use zenoh_transport::{
    multicast::TransportMulticast,
    unicast::{test_helpers::make_transport_manager_builder, TransportUnicast},
    DummyTransportPeerEventHandler, TransportEventHandler, TransportManager,
    TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler,
};
#[cfg(any(feature = "transport_tcp", feature = "transport_udp"))]
use zenoh_util::net::get_ipv4_ipaddrs;

const TIMEOUT: Duration = Duration::from_secs(60);
const TIMEOUT_EXPECTED: Duration = Duration::from_secs(5);
const SLEEP: Duration = Duration::from_millis(100);

macro_rules! ztimeout_expected {
    ($f:expr) => {
        tokio::time::timeout(TIMEOUT_EXPECTED, $f).await.unwrap()
    };
}

#[cfg(test)]
#[derive(Default)]
struct SHRouterOpenClose;

impl TransportEventHandler for SHRouterOpenClose {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        Ok(Arc::new(DummyTransportPeerEventHandler))
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        panic!();
    }
}

// Transport Handler for the client
struct SHClientOpenClose {}

impl SHClientOpenClose {
    fn new() -> Self {
        Self {}
    }
}

impl TransportEventHandler for SHClientOpenClose {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        Ok(Arc::new(DummyTransportPeerEventHandler))
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        panic!();
    }
}

async fn openclose_transport(
    listen_endpoint: &EndPoint,
    connect_endpoint: &EndPoint,
    bind_addr: &Address<'_>,
    lowlatency_transport: bool,
) {
    /* [ROUTER] */
    let router_id = ZenohIdProto::try_from([1]).unwrap();

    let router_handler = Arc::new(SHRouterOpenClose);
    // Create the router transport manager
    let unicast = make_transport_manager_builder(
        #[cfg(feature = "transport_multilink")]
        2,
        lowlatency_transport,
    )
    .max_sessions(1);
    let router_manager = TransportManager::builder()
        .whatami(WhatAmI::Router)
        .zid(router_id)
        .unicast(unicast)
        .build(router_handler.clone())
        .unwrap();

    /* [CLIENT] */
    let client01_id = ZenohIdProto::try_from([2]).unwrap();

    // Create the transport transport manager for the first client
    let unicast = make_transport_manager_builder(
        #[cfg(feature = "transport_multilink")]
        2,
        lowlatency_transport,
    )
    .max_sessions(1);
    let client01_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client01_id)
        .unicast(unicast)
        .build(Arc::new(SHClientOpenClose::new()))
        .unwrap();

    /* [1] */
    println!("\nTransport Open Close [1a1]");
    // Add the locator on the router
    let router_res = ztimeout!(router_manager.add_listener(listen_endpoint.clone()));
    println!("Transport Open Close [1a1]: {router_res:?}");
    assert!(router_res.is_ok());
    println!("Transport Open Close [1a2]");
    let locators = ztimeout!(router_manager.get_listeners());
    println!("Transport Open Close [1a2]: {locators:?}");
    assert_eq!(locators.len(), 1);

    // Open a first transport from the client to the router
    // -> This should be accepted
    let links_num = 1;

    println!("Transport Open Close [1c1]");
    let open_res =
        ztimeout_expected!(client01_manager.open_transport_unicast(connect_endpoint.clone()));
    println!("Transport Open Close [1c2]: {open_res:?}");
    assert!(open_res.is_ok());
    let c_ses1 = open_res.unwrap();
    println!("Transport Open Close [1d1]");
    let transports = ztimeout!(client01_manager.get_transports_unicast());
    println!("Transport Open Close [1d2]: {transports:?}");
    assert_eq!(transports.len(), 1);
    assert_eq!(c_ses1.get_zid().unwrap(), router_id);
    println!("Transport Open Close [1e1]");
    let links = c_ses1.get_links().unwrap();
    println!("Transport Open Close [1e2]: {links:?}");
    assert_eq!(links.len(), links_num);

    // Verify that the transport has been open on the router
    println!("Transport Open Close [1f1]");
    ztimeout!(async {
        loop {
            let transports = ztimeout!(router_manager.get_transports_unicast());
            let s = transports
                .iter()
                .find(|s| s.get_zid().unwrap() == client01_id);

            match s {
                Some(s) => {
                    let links = s.get_links().unwrap();
                    assert_eq!(links.len(), links_num);
                    break;
                }
                None => tokio::time::sleep(SLEEP).await,
            }
        }
    });

    // Check link on each side of connection
    // Expect Client to be bound to bind_addr
    // Expect Router to connect to bind_addr as foreign link
    println!("Transport Open Close [1f2]");
    ztimeout!(async {
        let router_transports = ztimeout!(router_manager.get_transports_unicast());
        let router_to_client_transport = router_transports
            .iter()
            .find(|s| s.get_zid().unwrap() == client01_id);

        let client_transports = ztimeout!(client01_manager.get_transports_unicast());
        let client_to_router_transport = client_transports
            .iter()
            .find(|s| s.get_zid().unwrap() == router_id);

        match (router_to_client_transport, client_to_router_transport) {
            (Some(r_to_c_tx), Some(c_to_r_tx)) => {
                let router_links = r_to_c_tx.get_links().unwrap();
                let client_links = c_to_r_tx.get_links().unwrap();

                println!("Transport Open Close [1f2]: {router_res:?}");
                println!("Router src {:?}", router_links[0].src);
                println!("Router dst {:?}", router_links[0].dst);
                println!("Client src {:?}", client_links[0].src);
                println!("Client dst {:?}", client_links[0].dst);
                println!("Bind Addr {bind_addr:?}");

                if bind_addr.as_str().contains("localhost") {
                    let mut iter = bind_addr.as_str().split(":");
                    let _host = iter.next();
                    let port = iter.next().unwrap();
                    // Create representation for Localhost in v4 and v6
                    let lh_ipv4 = format!("127.0.0.1:{port}");
                    let lh_ipv6 = format!("[::1]:{port}");
                    // Create addr for Localhost in v4 and v6
                    let addr_ipv4 = Address::from(lh_ipv4.as_str());
                    let addr_ipv6 = Address::from(lh_ipv6.as_str());
                    // Create addr for Localhost in v4 and v6
                    let router_check_ipv4 = router_links[0].dst.address() == addr_ipv4;
                    let router_check_ipv6 = router_links[0].dst.address() == addr_ipv6;
                    let client_check_ipv4 = client_links[0].src.address() == addr_ipv4;
                    let client_check_ipv6 = client_links[0].src.address() == addr_ipv6;
                    // Check either ipv4 or ipv6 bind
                    let check_ipv4 = client_check_ipv4 & router_check_ipv4;
                    let check_ipv6 = client_check_ipv6 & router_check_ipv6;

                    assert!(router_links[0].dst == client_links[0].src);
                    // Check either ipv4 or ipv6 bind
                    assert!(check_ipv4 | check_ipv6);
                } else {
                    assert!(router_links[0].dst == client_links[0].src);
                    assert!(router_links[0].dst.address() == *bind_addr);
                    assert!(client_links[0].src.address() == *bind_addr);
                }
            }
            _ => tokio::time::sleep(SLEEP).await,
        }
    });

    /* [2] */
    // Close the open transport on the client
    println!("\nTransport Open Close [2a1]");
    let res = ztimeout!(c_ses1.close());
    println!("Transport Open Close [2a2]: {res:?}");
    assert!(res.is_ok());
    println!("Transport Open Close [2b1]");
    let transports = ztimeout!(client01_manager.get_transports_unicast());
    println!("Transport Open Close [2b2]: {transports:?}");
    assert_eq!(transports.len(), 0);

    // Verify that the transport has been closed also on the router
    println!("Transport Open Close [2c1]");
    ztimeout!(async {
        loop {
            let transports = ztimeout!(router_manager.get_transports_unicast());
            let index = transports
                .iter()
                .find(|s| s.get_zid().unwrap() == client01_id);
            if index.is_none() {
                break;
            }
            tokio::time::sleep(SLEEP).await;
        }
    });

    // Wait a little bit
    tokio::time::sleep(SLEEP).await;

    ztimeout!(router_manager.close());
    ztimeout!(client01_manager.close());

    // Wait a little bit
    tokio::time::sleep(SLEEP).await;
}

#[cfg(feature = "transport_tcp")]
#[should_panic(expected = "assertion failed: open_res.is_ok()")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn openclose_tcp_only_connect_with_bind_and_interface() {
    let addrs = get_ipv4_ipaddrs(None);

    zenoh_util::init_log_from_env_or("error");

    let bind_addr_str = format!("{}:{}", addrs[0], 13002);
    let bind_addr = Address::from(bind_addr_str.as_str());

    let listen_endpoint: EndPoint = format!("tcp/{}:{}", addrs[0], 13001).parse().unwrap();

    // declaring `bind` and `iface` simultaneously should be unsupported, and there for fail
    let connect_endpoint: EndPoint =
        format!("tcp/{}:{}#iface=lo;bind={}", addrs[0], 13001, bind_addr)
            .parse()
            .unwrap();

    // should not connect to local interface and external address
    openclose_transport(&listen_endpoint, &connect_endpoint, &bind_addr, false).await;
}

#[cfg(feature = "transport_tcp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn openclose_tcp_only_connect_with_bind_restriction() {
    let addrs = get_ipv4_ipaddrs(None);

    zenoh_util::init_log_from_env_or("error");

    let listen_endpoint: EndPoint = format!("tcp/{}:{}", addrs[0], 13003).parse().unwrap();
    let bind_addr_str = format!("{}:{}", addrs[0], 13004);
    let bind_addr = Address::from(bind_addr_str.as_str());

    // Bind to different port on same IP address
    // Expect this test to succeed -
    // When running the test multiple times locally a TcpStream does not get cleaned up
    let connect_endpoint: EndPoint = format!("tcp/{}:{}#bind={}", addrs[0], 13003, bind_addr_str)
        .parse()
        .unwrap();

    // should not connect to local interface and external address
    openclose_transport(&listen_endpoint, &connect_endpoint, &bind_addr, false).await;
}

#[cfg(feature = "transport_tcp")]
#[should_panic(expected = "assertion failed: open_res.is_ok()")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn openclose_tcp_only_connect_with_bind_restriction_mismatch_protocols() {
    use zenoh_util::net::get_ipv6_ipaddrs;

    let addrs = get_ipv4_ipaddrs(None);
    let addrs_v6 = get_ipv6_ipaddrs(None);

    let bind_addr_str = format!("{}:{}", addrs_v6[0], 13006);
    let bind_addr = Address::from(bind_addr_str.as_str());

    zenoh_util::init_log_from_env_or("error");

    let listen_endpoint: EndPoint = format!("tcp/{}:{}", addrs[0], 13005).parse().unwrap();

    // Bind to different port on same IP address
    let connect_endpoint: EndPoint = format!("tcp/{}:{}#bind={}", addrs[0], 13005, bind_addr_str)
        .parse()
        .unwrap();

    // should not connect to local interface and external address
    openclose_transport(&listen_endpoint, &connect_endpoint, &bind_addr, false).await;
}

#[cfg(feature = "transport_udp")]
#[should_panic(expected = "assertion failed: open_res.is_ok()")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn openclose_udp_only_connect_with_bind_restriction_mismatch_protocols() {
    use zenoh_util::net::get_ipv6_ipaddrs;

    let addrs = get_ipv4_ipaddrs(None);
    let addrs_v6 = get_ipv6_ipaddrs(None);

    let bind_addr_str = format!("{}:{}", addrs_v6[0], 13006);
    let bind_addr = Address::from(bind_addr_str.as_str());

    zenoh_util::init_log_from_env_or("error");

    let listen_endpoint: EndPoint = format!("udp/{}:{}", addrs[0], 13005).parse().unwrap();

    // Bind to different port on same IP address
    let connect_endpoint: EndPoint = format!("udp/{}:{}#bind={}", addrs[0], 13005, bind_addr_str)
        .parse()
        .unwrap();

    // should not connect to local interface and external address
    openclose_transport(&listen_endpoint, &connect_endpoint, &bind_addr, false).await;
}

#[cfg(feature = "transport_udp")]
#[should_panic(expected = "assertion failed: open_res.is_ok()")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn openclose_udp_only_connect_with_bind_and_interface() {
    let addrs = get_ipv4_ipaddrs(None);
    let bind_addr_str = format!("{}:{}", addrs[0], 13008);
    let bind_addr = Address::from(bind_addr_str.as_str());

    zenoh_util::init_log_from_env_or("error");

    let listen_endpoint: EndPoint = format!("udp/{}:{}", addrs[0], 13007).parse().unwrap();

    let connect_endpoint: EndPoint =
        format!("udp/{}:{}#iface=lo;bind={}", addrs[0], 13007, bind_addr_str)
            .parse()
            .unwrap();

    // should not connect to local interface and external address
    openclose_transport(&listen_endpoint, &connect_endpoint, &bind_addr, false).await;
}

#[cfg(feature = "transport_udp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn openclose_udp_only_connect_with_bind_restriction() {
    let addrs = get_ipv4_ipaddrs(None);
    let bind_addr_str = format!("{}:{}", addrs[0], 13010);
    let bind_addr = Address::from(bind_addr_str.as_str());

    zenoh_util::init_log_from_env_or("error");

    let listen_endpoint: EndPoint = format!("udp/{}:{}", addrs[0], 13009).parse().unwrap();

    let connect_endpoint: EndPoint = format!("udp/{}:{}#bind={}", addrs[0], 13009, bind_addr_str)
        .parse()
        .unwrap();

    // should not connect to local interface and external address
    openclose_transport(&listen_endpoint, &connect_endpoint, &bind_addr, false).await;
}

#[cfg(feature = "transport_quic")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn openclose_quic_only_connect_with_bind_restriction() {
    use zenoh_link_commons::tls::config::*;

    zenoh_util::init_log_from_env_or("error");
    let bind_addr_str = format!("localhost:{}", 13012);
    let bind_addr = Address::from(bind_addr_str.as_str());

    let client_auth = "true";
    // Define the client
    let mut connect_endpoint: EndPoint =
        (format!("quic/localhost:{}#bind={}", 13011, bind_addr_str))
            .parse()
            .unwrap();
    connect_endpoint
        .config_mut()
        .extend_from_iter(
            [
                (TLS_ROOT_CA_CERTIFICATE_RAW, SERVER_CA),
                (TLS_CONNECT_CERTIFICATE_RAW, CLIENT_CERT),
                (TLS_CONNECT_PRIVATE_KEY_RAW, CLIENT_KEY),
                (TLS_ENABLE_MTLS, client_auth),
            ]
            .iter()
            .copied(),
        )
        .unwrap();

    // Define the server
    let mut listen_endpoint: EndPoint = (format!("quic/localhost:{}", 13011)).parse().unwrap();
    listen_endpoint
        .config_mut()
        .extend_from_iter(
            [
                (TLS_ROOT_CA_CERTIFICATE_RAW, CLIENT_CA),
                (TLS_LISTEN_CERTIFICATE_RAW, SERVER_CERT),
                (TLS_LISTEN_PRIVATE_KEY_RAW, SERVER_KEY),
                (TLS_ENABLE_MTLS, client_auth),
            ]
            .iter()
            .copied(),
        )
        .unwrap();

    // should not connect to local interface and external address
    openclose_transport(&listen_endpoint, &connect_endpoint, &bind_addr, false).await;
}

#[cfg(feature = "transport_tls")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn openclose_tls_only_connect_with_bind_restriction() {
    use zenoh_link_commons::tls::config::*;

    zenoh_util::init_log_from_env_or("error");
    let bind_addr_str = format!("localhost:{}", 13014);
    let bind_addr = Address::from(bind_addr_str.as_str());

    let client_auth = "true";
    // Define the client
    let mut connect_endpoint: EndPoint =
        (format!("tls/localhost:{}#bind={}", 13013, bind_addr_str))
            .parse()
            .unwrap();
    connect_endpoint
        .config_mut()
        .extend_from_iter(
            [
                (TLS_ROOT_CA_CERTIFICATE_RAW, SERVER_CA),
                (TLS_CONNECT_CERTIFICATE_RAW, CLIENT_CERT),
                (TLS_CONNECT_PRIVATE_KEY_RAW, CLIENT_KEY),
                (TLS_ENABLE_MTLS, client_auth),
            ]
            .iter()
            .copied(),
        )
        .unwrap();

    // Define the server
    let mut listen_endpoint: EndPoint = (format!("tls/localhost:{}", 13013)).parse().unwrap();
    listen_endpoint
        .config_mut()
        .extend_from_iter(
            [
                (TLS_ROOT_CA_CERTIFICATE_RAW, CLIENT_CA),
                (TLS_LISTEN_CERTIFICATE_RAW, SERVER_CERT),
                (TLS_LISTEN_PRIVATE_KEY_RAW, SERVER_KEY),
                (TLS_ENABLE_MTLS, client_auth),
            ]
            .iter()
            .copied(),
        )
        .unwrap();

    // should not connect to local interface and external address
    openclose_transport(&listen_endpoint, &connect_endpoint, &bind_addr, false).await;
}

#[cfg(any(feature = "transport_tls", feature = "transport_quic"))]
const CLIENT_KEY: &str = "-----BEGIN RSA PRIVATE KEY-----
MIIEpAIBAAKCAQEAsfqAuhElN4HnyeqLovSd4Qe+nNv5AwCjSO+HFiF30x3vQ1Hi
qRA0UmyFlSqBnFH3TUHm4Jcad40QfrX8f11NKGZdpvKHsMYqYjZnYkRFGS2s4fQy
aDbV5M06s3UDX8ETPgY41Y8fCKTSVdi9iHkwcVrXMxUu4IBBx0C1r2GSo3gkIBnU
cELdFdaUOSbdCipJhbnkwixEr2h7PXxwba7SIZgZtRaQWak1VE9b716qe3iMuMha
Efo/UoFmeZCPu5spfwaOZsnCsxRPk2IjbzlsHTJ09lM9wmbEFHBMVAXejLTk++Sr
Xt8jASZhNen/2GzyLQNAquGn98lCMQ6SsE9vLQIDAQABAoIBAGQkKggHm6Q20L+4
2+bNsoOqguLplpvM4RMpyx11qWE9h6GeUmWD+5yg+SysJQ9aw0ZSHWEjRD4ePji9
lxvm2IIxzuIftp+NcM2gBN2ywhpfq9XbO/2NVR6PJ0dQQJzBG12bzKDFDdYkP0EU
WdiPL+WoEkvo0F57bAd77n6G7SZSgxYekBF+5S6rjbu5I1cEKW+r2vLehD4uFCVX
Q0Tu7TyIOE1KJ2anRb7ZXVUaguNj0/Er7EDT1+wN8KJKvQ1tYGIq/UUBtkP9nkOI
9XJd25k6m5AQPDddzd4W6/5+M7kjyVPi3CsQcpBPss6ueyecZOMaKqdWAHeEyaak
r67TofUCgYEA6GBa+YkRvp0Ept8cd5mh4gCRM8wUuhtzTQnhubCPivy/QqMWScdn
qD0OiARLAsqeoIfkAVgyqebVnxwTrKTvWe0JwpGylEVWQtpGz3oHgjST47yZxIiY
CSAaimi2CYnJZ+QB2oBkFVwNCuXdPEGX6LgnOGva19UKrm6ONsy6V9MCgYEAxBJu
fu4dGXZreARKEHa/7SQjI9ayAFuACFlON/EgSlICzQyG/pumv1FsMEiFrv6w7PRj
4AGqzyzGKXWVDRMrUNVeGPSKJSmlPGNqXfPaXRpVEeB7UQhAs5wyMrWDl8jEW7Ih
XcWhMLn1f/NOAKyrSDSEaEM+Nuu+xTifoAghvP8CgYEAlta9Fw+nihDIjT10cBo0
38w4dOP7bFcXQCGy+WMnujOYPzw34opiue1wOlB3FIfL8i5jjY/fyzPA5PhHuSCT
Ec9xL3B9+AsOFHU108XFi/pvKTwqoE1+SyYgtEmGKKjdKOfzYA9JaCgJe1J8inmV
jwXCx7gTJVjwBwxSmjXIm+sCgYBQF8NhQD1M0G3YCdCDZy7BXRippCL0OGxVfL2R
5oKtOVEBl9NxH/3+evE5y/Yn5Mw7Dx3ZPHUcygpslyZ6v9Da5T3Z7dKcmaVwxJ+H
n3wcugv0EIHvOPLNK8npovINR6rGVj6BAqD0uZHKYYYEioQxK5rGyGkaoDQ+dgHm
qku12wKBgQDem5FvNp5iW7mufkPZMqf3sEGtu612QeqejIPFM1z7VkUgetsgPBXD
tYsqC2FtWzY51VOEKNpnfH7zH5n+bjoI9nAEAW63TK9ZKkr2hRGsDhJdGzmLfQ7v
F6/CuIw9EsAq6qIB8O88FXQqald+BZOx6AzB8Oedsz/WtMmIEmr/+Q==
-----END RSA PRIVATE KEY-----";

#[cfg(any(feature = "transport_tls", feature = "transport_quic"))]
const CLIENT_CERT: &str = "-----BEGIN CERTIFICATE-----
MIIDLjCCAhagAwIBAgIIeUtmIdFQznMwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgMDc4ZGE3MCAXDTIzMDMwNjE2MDMxOFoYDzIxMjMw
MzA2MTYwMzE4WjAUMRIwEAYDVQQDEwlsb2NhbGhvc3QwggEiMA0GCSqGSIb3DQEB
AQUAA4IBDwAwggEKAoIBAQCx+oC6ESU3gefJ6oui9J3hB76c2/kDAKNI74cWIXfT
He9DUeKpEDRSbIWVKoGcUfdNQebglxp3jRB+tfx/XU0oZl2m8oewxipiNmdiREUZ
Lazh9DJoNtXkzTqzdQNfwRM+BjjVjx8IpNJV2L2IeTBxWtczFS7ggEHHQLWvYZKj
eCQgGdRwQt0V1pQ5Jt0KKkmFueTCLESvaHs9fHBtrtIhmBm1FpBZqTVUT1vvXqp7
eIy4yFoR+j9SgWZ5kI+7myl/Bo5mycKzFE+TYiNvOWwdMnT2Uz3CZsQUcExUBd6M
tOT75Kte3yMBJmE16f/YbPItA0Cq4af3yUIxDpKwT28tAgMBAAGjdjB0MA4GA1Ud
DwEB/wQEAwIFoDAdBgNVHSUEFjAUBggrBgEFBQcDAQYIKwYBBQUHAwIwDAYDVR0T
AQH/BAIwADAfBgNVHSMEGDAWgBTWfAmQ/BUIQm/9/llJJs2jUMWzGzAUBgNVHREE
DTALgglsb2NhbGhvc3QwDQYJKoZIhvcNAQELBQADggEBAG/POnBob0S7iYwsbtI2
3LTTbRnmseIErtJuJmI9yYzgVIm6sUSKhlIUfAIm4rfRuzE94KFeWR2w9RabxOJD
wjYLLKvQ6rFY5g2AV/J0TwDjYuq0absdaDPZ8MKJ+/lpGYK3Te+CTOfq5FJRFt1q
GOkXAxnNpGg0obeRWRKFiAMHbcw6a8LIMfRjCooo3+uSQGsbVzGxSB4CYo720KcC
9vB1K9XALwzoqCewP4aiQsMY1GWpAmzXJftY3w+lka0e9dBYcdEdOqxSoZb5OBBZ
p5e60QweRuJsb60aUaCG8HoICevXYK2fFqCQdlb5sIqQqXyN2K6HuKAFywsjsGyJ
abY=
-----END CERTIFICATE-----";

#[cfg(any(feature = "transport_tls", feature = "transport_quic"))]
const CLIENT_CA: &str = "-----BEGIN CERTIFICATE-----
MIIDSzCCAjOgAwIBAgIIB42n1ZIkOakwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgMDc4ZGE3MCAXDTIzMDMwNjE2MDMwN1oYDzIxMjMw
MzA2MTYwMzA3WjAgMR4wHAYDVQQDExVtaW5pY2Egcm9vdCBjYSAwNzhkYTcwggEi
MA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQDIuCq24O4P4Aep5vAVlrIQ7P8+
uWWgcHIFYa02TmhBUB/hjo0JANCQvAtpVNuQ8NyKPlqnnq1cttePbSYVeA0rrnOs
DcfySAiyGBEY9zMjFfHJtH1wtrPcJEU8XIEY3xUlrAJE2CEuV9dVYgfEEydnvgLc
8Ug0WXSiARjqbnMW3l8jh6bYCp/UpL/gSM4mxdKrgpfyPoweGhlOWXc3RTS7cqM9
T25acURGOSI6/g8GF0sNE4VZmUvHggSTmsbLeXMJzxDWO+xVehRmbQx3IkG7u++b
QdRwGIJcDNn7zHlDMHtQ0Z1DBV94fZNBwCULhCBB5g20XTGw//S7Fj2FPwyhAgMB
AAGjgYYwgYMwDgYDVR0PAQH/BAQDAgKEMB0GA1UdJQQWMBQGCCsGAQUFBwMBBggr
BgEFBQcDAjASBgNVHRMBAf8ECDAGAQH/AgEAMB0GA1UdDgQWBBTWfAmQ/BUIQm/9
/llJJs2jUMWzGzAfBgNVHSMEGDAWgBTWfAmQ/BUIQm/9/llJJs2jUMWzGzANBgkq
hkiG9w0BAQsFAAOCAQEAvtcZFAELKiTuOiAeYts6zeKxc+nnHCzayDeD/BDCbxGJ
e1n+xdHjLtWGd+/Anc+fvftSYBPTFQqCi84lPiUIln5z/rUxE+ke81hNPIfw2obc
yIg87xCabQpVyEh8s+MV+7YPQ1+fH4FuSi2Fck1FejxkVqN2uOZPvOYUmSTsaVr1
8SfRnwJNZ9UMRPM2bD4Jkvj0VcL42JM3QkOClOzYW4j/vll2cSs4kx7er27cIoo1
Ck0v2xSPAiVjg6w65rUQeW6uB5m0T2wyj+wm0At8vzhZPlgS1fKhcmT2dzOq3+oN
R+IdLiXcyIkg0m9N8I17p0ljCSkbrgGMD3bbePRTfg==
-----END CERTIFICATE-----";

#[cfg(any(feature = "transport_tls", feature = "transport_quic"))]
const SERVER_KEY: &str = "-----BEGIN RSA PRIVATE KEY-----
MIIEpAIBAAKCAQEAmDCySqKHPmEZShDH3ldPaV/Zsh9+HlHFLk9H10vJZj5WfzVu
5puZQ8GvBFIOtVrl0L9qLkA6bZiHHXm/8OEVvd135ZMp4NV23fdTsEASXfvGVQY8
y+4UkZN0Dw6sfwlQVPyNRplys2+nFs6tX05Dp9VizV39tSOqe/jd6hyzxSUHqFat
RwQRXAI04CZ6ckDb0Riw7i0yvjrFhBom9lPKq4IkXZGgS5MRl0pRgAZTqHEMlv8z
oX+KcG9mfyQIHtpkVuSHHsQjwVop7fMnT7KCQ3bPI+fgMmAg+h1IR19Dm0JM+9zl
u39j0IbkytrsystGM+pTRbdp7s2lgtOMCFt0+wIDAQABAoIBADNTSO2uvlmlOXgn
DKDJZTiuYKaXxFrJTOx/REUxg+x9XYJtLMeM9jVJnpKgceFrlFHAHDkY5BuN8xNX
ugmsfz6W8BZ2eQsgMoRNIuYv1YHopUyLW/mSg1FNHzjsw/Pb2kGvIp4Kpgopv3oL
naCkrmBtsHJ+Hk/2hUpl9cE8iMwVWcVevLzyHi98jNy1IDdIPhRtl0dhMiqC5MRr
4gLJ5gNkLYX7xf3tw5Hmfk/bVNProqZXDIQVI7rFvItX586nvQ3LNQkmW/D2ShZf
3FEqMu6EdA2Ycc4UZgAlQNGV0VBrWWVXizOQ+9gjLnBk3kJjqfigCU6NG94bTJ+H
0YIhsGECgYEAwdSSyuMSOXgzZQ7Vv+GsNn/7ivi/H8eb/lDzksqS/JroA2ciAmHG
2OF30eUJKRg+STqBTpOfXgS4QUa8QLSwBSnwcw6579x9bYGUhqD2Ypaw9uCnOukA
CwwggZ9cDmF0tb5rYjqkW3bFPqkCnTGb0ylMFaYRhRDU20iG5t8PQckCgYEAyQEM
KK18FLQUKivGrQgP5Ib6IC3myzlHGxDzfobXGpaQntFnHY7Cxp/6BBtmASzt9Jxu
etnrevmzrbKqsLTJSg3ivbiq0YTLAJ1FsZrCp71dx49YR/5o9QFiq0nQoKnwUVeb
/hrDjMAokNkjFL5vouXO711GSS6YyM4WzAKZAqMCgYEAhqGxaG06jmJ4SFx6ibIl
nSFeRhQrJNbP+mCeHrrIR98NArgS/laN+Lz7LfaJW1r0gIa7pCmTi4l5thV80vDu
RlfwJOr4qaucD4Du+mg5WxdSSdiXL6sBlarRtVdMaMy2dTqTegJDgShJLxHTt/3q
P0yzBWJ5TtT3FG0XDqum/EkCgYAYNHwWWe3bQGQ9P9BI/fOL/YUZYu2sA1XAuKXZ
0rsMhJ0dwvG76XkjGhitbe82rQZqsnvLZ3qn8HHmtOFBLkQfGtT3K8nGOUuI42eF
H7HZKUCly2lCIizZdDVBkz4AWvaJlRc/3lE2Hd3Es6E52kTvROVKhdz06xuS8t5j
6twqKQKBgQC01AeiWL6Rzo+yZNzVgbpeeDogaZz5dtmURDgCYH8yFX5eoCKLHfnI
2nDIoqpaHY0LuX+dinuH+jP4tlyndbc2muXnHd9r0atytxA69ay3sSA5WFtfi4ef
ESElGO6qXEA821RpQp+2+uhL90+iC294cPqlS5LDmvTMypVDHzrxPQ==
-----END RSA PRIVATE KEY-----";

#[cfg(any(feature = "transport_tls", feature = "transport_quic"))]
const SERVER_CERT: &str = "-----BEGIN CERTIFICATE-----
MIIDLjCCAhagAwIBAgIIW1mAtJWJAJYwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgNGRjYzJmMCAXDTIzMDMwNjE2NDEwNloYDzIxMjMw
MzA2MTY0MTA2WjAUMRIwEAYDVQQDEwlsb2NhbGhvc3QwggEiMA0GCSqGSIb3DQEB
AQUAA4IBDwAwggEKAoIBAQCYMLJKooc+YRlKEMfeV09pX9myH34eUcUuT0fXS8lm
PlZ/NW7mm5lDwa8EUg61WuXQv2ouQDptmIcdeb/w4RW93Xflkyng1Xbd91OwQBJd
+8ZVBjzL7hSRk3QPDqx/CVBU/I1GmXKzb6cWzq1fTkOn1WLNXf21I6p7+N3qHLPF
JQeoVq1HBBFcAjTgJnpyQNvRGLDuLTK+OsWEGib2U8qrgiRdkaBLkxGXSlGABlOo
cQyW/zOhf4pwb2Z/JAge2mRW5IcexCPBWint8ydPsoJDds8j5+AyYCD6HUhHX0Ob
Qkz73OW7f2PQhuTK2uzKy0Yz6lNFt2nuzaWC04wIW3T7AgMBAAGjdjB0MA4GA1Ud
DwEB/wQEAwIFoDAdBgNVHSUEFjAUBggrBgEFBQcDAQYIKwYBBQUHAwIwDAYDVR0T
AQH/BAIwADAfBgNVHSMEGDAWgBTX46+p+Po1npE6QLQ7mMI+83s6qDAUBgNVHREE
DTALgglsb2NhbGhvc3QwDQYJKoZIhvcNAQELBQADggEBAAxrmQPG54ybKgMVliN8
Mg5povSdPIVVnlU/HOVG9yxzAOav/xQP003M4wqpatWxI8tR1PcLuZf0EPmcdJgb
tVl9nZMVZtveQnYMlU8PpkEVu56VM4Zr3rH9liPRlr0JEAXODdKw76kWKzmdqWZ/
rzhup3Ek7iEX6T5j/cPUvTWtMD4VEK2I7fgoKSHIX8MIVzqM7cuboGWPtS3eRNXl
MgvahA4TwLEXPEe+V1WAq6nSb4g2qSXWIDpIsy/O1WGS/zzRnKvXu9/9NkXWqZMl
C1LSpiiQUaRSglOvYf/Zx6r+4BOS4OaaArwHkecZQqBSCcBLEAyb/FaaXdBowI0U
PQ4=
-----END CERTIFICATE-----";

#[cfg(any(feature = "transport_tls", feature = "transport_quic"))]
const SERVER_CA: &str = "-----BEGIN CERTIFICATE-----
MIIDSzCCAjOgAwIBAgIITcwv1N10nqEwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgNGRjYzJmMCAXDTIzMDMwNjE2NDEwNloYDzIxMjMw
MzA2MTY0MTA2WjAgMR4wHAYDVQQDExVtaW5pY2Egcm9vdCBjYSA0ZGNjMmYwggEi
MA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQC2WUgN7NMlXIknew1cXiTWGmS0
1T1EjcNNDAq7DqZ7/ZVXrjD47yxTt5EOiOXK/cINKNw4Zq/MKQvq9qu+Oax4lwiV
Ha0i8ShGLSuYI1HBlXu4MmvdG+3/SjwYoGsGaShr0y/QGzD3cD+DQZg/RaaIPHlO
MdmiUXxkMcy4qa0hFJ1imlJdq/6Tlx46X+0vRCh8nkekvOZR+t7Z5U4jn4XE54Kl
0PiwcyX8vfDZ3epa/FSHZvVQieM/g5Yh9OjIKCkdWRg7tD0IEGsaW11tEPJ5SiQr
mDqdRneMzZKqY0xC+QqXSvIlzpOjiu8PYQx7xugaUFE/npKRQdvh8ojHJMdNAgMB
AAGjgYYwgYMwDgYDVR0PAQH/BAQDAgKEMB0GA1UdJQQWMBQGCCsGAQUFBwMBBggr
BgEFBQcDAjASBgNVHRMBAf8ECDAGAQH/AgEAMB0GA1UdDgQWBBTX46+p+Po1npE6
QLQ7mMI+83s6qDAfBgNVHSMEGDAWgBTX46+p+Po1npE6QLQ7mMI+83s6qDANBgkq
hkiG9w0BAQsFAAOCAQEAaN0IvEC677PL/JXzMrXcyBV88IvimlYN0zCt48GYlhmx
vL1YUDFLJEB7J+dyERGE5N6BKKDGblC4WiTFgDMLcHFsMGRc0v7zKPF1PSBwRYJi
ubAmkwdunGG5pDPUYtTEDPXMlgClZ0YyqSFJMOqA4IzQg6exVjXtUxPqzxNhyC7S
vlgUwPbX46uNi581a9+Ls2V3fg0ZnhkTSctYZHGZNeh0Nsf7Am8xdUDYG/bZcVef
jbQ9gpChosdjF0Bgblo7HSUct/2Va+YlYwW+WFjJX8k4oN6ZU5W5xhdfO8Czmgwk
US5kJ/+1M0uR8zUhZHL61FbsdPxEj+fYKrHv4woo+A==
-----END CERTIFICATE-----";
