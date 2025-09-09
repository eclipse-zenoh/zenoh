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
use std::{
    convert::TryFrom,
    sync::Arc,
    time::{Duration, Instant},
};

use zenoh_core::ztimeout;
use zenoh_link::EndPoint;
use zenoh_protocol::core::{WhatAmI, ZenohIdProto};
use zenoh_result::ZResult;
use zenoh_transport::{
    multicast::TransportMulticast,
    unicast::{test_helpers::make_transport_manager_builder, TransportUnicast},
    DummyTransportPeerEventHandler, TransportEventHandler, TransportManager,
    TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler,
};

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

async fn time_transport(
    listen_endpoint: &EndPoint,
    connect_endpoint: &EndPoint,
    lowlatency_transport: bool,
) {
    if lowlatency_transport {
        println!(">>> Low latency transport");
    } else {
        println!(">>> Universal transport");
    }
    /* [ROUTER] */
    let router_id = ZenohIdProto::try_from([1]).unwrap();

    let router_handler = Arc::new(SHRouterOpenClose);
    // Create the router transport manager
    let unicast = make_transport_manager_builder(
        #[cfg(feature = "transport_multilink")]
        1,
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
        1,
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
    // Add the locator on the router
    let start = Instant::now();
    ztimeout!(router_manager.add_listener(listen_endpoint.clone())).unwrap();
    println!("Add listener {}: {:#?}", listen_endpoint, start.elapsed());

    // Open a transport from the client to the router
    let start = Instant::now();
    let c_ses1 =
        ztimeout_expected!(client01_manager.open_transport_unicast(connect_endpoint.clone()))
            .unwrap();
    println!(
        "Open transport {}: {:#?}",
        connect_endpoint,
        start.elapsed()
    );

    // Verify that the transport has been open on the router
    ztimeout!(async {
        loop {
            let transports = ztimeout!(router_manager.get_transports_unicast());
            let s = transports
                .iter()
                .find(|s| s.get_zid().unwrap() == client01_id);

            match s {
                Some(s) => {
                    let links = s.get_links().unwrap();
                    assert_eq!(links.len(), 1);
                    break;
                }
                None => tokio::time::sleep(SLEEP).await,
            }
        }
    });

    /* [2] */
    // Close the open transport on the client
    let start = Instant::now();
    ztimeout!(c_ses1.close()).unwrap();
    println!(
        "Close transport {}: {:#?}",
        connect_endpoint,
        start.elapsed()
    );

    // Verify that the transport has been closed also on the router
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

    /* [3] */
    let start = Instant::now();
    ztimeout!(router_manager.del_listener(listen_endpoint)).unwrap();
    println!(
        "Delete listener {}: {:#?}",
        listen_endpoint,
        start.elapsed()
    );

    ztimeout!(async {
        while !router_manager.get_listeners().await.is_empty() {
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

async fn time_universal_transport(endpoint: &EndPoint) {
    time_transport(endpoint, endpoint, false).await
}

async fn time_lowlatency_transport(endpoint: &EndPoint) {
    time_transport(endpoint, endpoint, true).await
}

#[cfg(feature = "transport_tcp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn time_tcp_only() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = format!("tcp/127.0.0.1:{}", 13000).parse().unwrap();
    time_universal_transport(&endpoint).await;
}

#[cfg(feature = "transport_tcp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn time_tcp_only_with_lowlatency_transport() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = format!("tcp/127.0.0.1:{}", 13100).parse().unwrap();
    time_lowlatency_transport(&endpoint).await;
}

#[cfg(feature = "transport_udp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn time_udp_only() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = format!("udp/127.0.0.1:{}", 13010).parse().unwrap();
    time_universal_transport(&endpoint).await;
}

#[cfg(feature = "transport_udp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn time_udp_only_with_lowlatency_transport() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = format!("udp/127.0.0.1:{}", 13110).parse().unwrap();
    time_lowlatency_transport(&endpoint).await;
}

#[cfg(feature = "transport_ws")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn time_ws_only() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = format!("ws/127.0.0.1:{}", 13020).parse().unwrap();
    time_universal_transport(&endpoint).await;
}

#[cfg(feature = "transport_ws")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn time_ws_only_with_lowlatency_transport() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = format!("ws/127.0.0.1:{}", 13120).parse().unwrap();
    time_lowlatency_transport(&endpoint).await;
}

#[cfg(feature = "transport_unixpipe")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn time_unixpipe_only() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = "unixpipe/time_unixpipe_only".parse().unwrap();
    time_universal_transport(&endpoint).await;
}

#[cfg(feature = "transport_unixpipe")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn time_unixpipe_only_with_lowlatency_transport() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = "unixpipe/time_unixpipe_only_with_lowlatency_transport"
        .parse()
        .unwrap();
    time_lowlatency_transport(&endpoint).await;
}

#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn time_unix_only() {
    zenoh_util::init_log_from_env_or("error");
    let f1 = "zenoh-test-unix-socket-9.sock";
    let _ = std::fs::remove_file(f1);
    let endpoint: EndPoint = format!("unixsock-stream/{f1}").parse().unwrap();
    time_universal_transport(&endpoint).await;
    let _ = std::fs::remove_file(f1);
    let _ = std::fs::remove_file(format!("{f1}.lock"));
}

#[cfg(feature = "transport_tls")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn time_tls_only() {
    use zenoh_link_commons::tls::config::*;
    zenoh_util::init_log_from_env_or("error");
    // NOTE: this an auto-generated pair of certificate and key.
    //       The target domain is localhost, so it has no real
    //       mapping to any existing domain. The certificate and key
    //       have been generated using: https://github.com/jsha/minica
    let key = "-----BEGIN RSA PRIVATE KEY-----
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

    let cert = "-----BEGIN CERTIFICATE-----
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

    // Configure the client
    let ca = "-----BEGIN CERTIFICATE-----
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

    let mut endpoint: EndPoint = format!("tls/localhost:{}", 13030).parse().unwrap();
    endpoint
        .config_mut()
        .extend_from_iter(
            [
                (TLS_ROOT_CA_CERTIFICATE_RAW, ca),
                (TLS_LISTEN_PRIVATE_KEY_RAW, key),
                (TLS_LISTEN_CERTIFICATE_RAW, cert),
            ]
            .iter()
            .copied(),
        )
        .unwrap();

    time_universal_transport(&endpoint).await;
}

#[cfg(feature = "transport_quic")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn time_quic_only() {
    use zenoh_link_commons::tls::config::*;

    // NOTE: this an auto-generated pair of certificate and key.
    //       The target domain is localhost, so it has no real
    //       mapping to any existing domain. The certificate and key
    //       have been generated using: https://github.com/jsha/minica
    let key = "-----BEGIN RSA PRIVATE KEY-----
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

    let cert = "-----BEGIN CERTIFICATE-----
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

    // Configure the client
    let ca = "-----BEGIN CERTIFICATE-----
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

    // Define the locator
    let mut endpoint: EndPoint = format!("quic/localhost:{}", 13040).parse().unwrap();
    endpoint
        .config_mut()
        .extend_from_iter(
            [
                (TLS_ROOT_CA_CERTIFICATE_RAW, ca),
                (TLS_LISTEN_PRIVATE_KEY_RAW, key),
                (TLS_LISTEN_CERTIFICATE_RAW, cert),
            ]
            .iter()
            .copied(),
        )
        .unwrap();

    time_universal_transport(&endpoint).await;
}

#[cfg(all(feature = "transport_vsock", target_os = "linux"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn time_vsock_only() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = "vsock/VMADDR_CID_LOCAL:17000".parse().unwrap();
    time_lowlatency_transport(&endpoint).await;
}
