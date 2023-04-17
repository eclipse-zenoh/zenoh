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
use async_std::{prelude::FutureExt, task};
use std::{convert::TryFrom, sync::Arc, time::Duration};
use zenoh_core::zasync_executor_init;
use zenoh_link::EndPoint;
use zenoh_protocol::core::{WhatAmI, ZenohId};
use zenoh_result::ZResult;
use zenoh_transport::{
    DummyTransportPeerEventHandler, TransportEventHandler, TransportManager, TransportMulticast,
    TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler, TransportUnicast,
};

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_millis(100);

macro_rules! ztimeout {
    ($f:expr) => {
        $f.timeout(TIMEOUT).await.unwrap()
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
        Ok(Arc::new(DummyTransportPeerEventHandler::default()))
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
        Ok(Arc::new(DummyTransportPeerEventHandler::default()))
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        panic!();
    }
}

async fn openclose_transport(endpoint: &EndPoint) {
    /* [ROUTER] */
    let router_id = ZenohId::try_from([1]).unwrap();

    let router_handler = Arc::new(SHRouterOpenClose::default());
    // Create the router transport manager
    let unicast = TransportManager::config_unicast()
        .max_links(2)
        .max_sessions(1);
    let router_manager = TransportManager::builder()
        .whatami(WhatAmI::Router)
        .zid(router_id)
        .unicast(unicast)
        .build(router_handler.clone())
        .unwrap();

    /* [CLIENT] */
    let client01_id = ZenohId::try_from([2]).unwrap();
    let client02_id = ZenohId::try_from([3]).unwrap();

    // Create the transport transport manager for the first client
    let unicast = TransportManager::config_unicast()
        .max_links(2)
        .max_sessions(1);
    let client01_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client01_id)
        .unicast(unicast)
        .build(Arc::new(SHClientOpenClose::new()))
        .unwrap();

    // Create the transport transport manager for the second client
    let unicast = TransportManager::config_unicast()
        .max_links(1)
        .max_sessions(1);
    let client02_manager = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client02_id)
        .unicast(unicast)
        .build(Arc::new(SHClientOpenClose::new()))
        .unwrap();

    /* [1] */
    println!("\nTransport Open Close [1a1]");
    // Add the locator on the router
    let res = ztimeout!(router_manager.add_listener(endpoint.clone()));
    println!("Transport Open Close [1a1]: {res:?}");
    assert!(res.is_ok());
    println!("Transport Open Close [1a2]");
    let locators = router_manager.get_listeners();
    println!("Transport Open Close [1a2]: {locators:?}");
    assert_eq!(locators.len(), 1);

    // Open a first transport from the client to the router
    // -> This should be accepted
    let mut links_num = 1;

    println!("Transport Open Close [1c1]");
    let res = ztimeout!(client01_manager.open_transport(endpoint.clone()));
    println!("Transport Open Close [1c2]: {res:?}");
    assert!(res.is_ok());
    let c_ses1 = res.unwrap();
    println!("Transport Open Close [1d1]");
    let transports = client01_manager.get_transports();
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
            let transports = router_manager.get_transports();
            let s = transports
                .iter()
                .find(|s| s.get_zid().unwrap() == client01_id);

            match s {
                Some(s) => {
                    let links = s.get_links().unwrap();
                    assert_eq!(links.len(), links_num);
                    break;
                }
                None => task::sleep(SLEEP).await,
            }
        }
    });

    /* [2] */
    // Open a second transport from the client to the router
    // -> This should be accepted
    links_num = 2;

    println!("\nTransport Open Close [2a1]");
    let res = ztimeout!(client01_manager.open_transport(endpoint.clone()));
    println!("Transport Open Close [2a2]: {res:?}");
    assert!(res.is_ok());
    let c_ses2 = res.unwrap();
    println!("Transport Open Close [2b1]");
    let transports = client01_manager.get_transports();
    println!("Transport Open Close [2b2]: {transports:?}");
    assert_eq!(transports.len(), 1);
    assert_eq!(c_ses2.get_zid().unwrap(), router_id);
    println!("Transport Open Close [2c1]");
    let links = c_ses2.get_links().unwrap();
    println!("Transport Open Close [2c2]: {links:?}");
    assert_eq!(links.len(), links_num);
    assert_eq!(c_ses2, c_ses1);

    // Verify that the transport has been open on the router
    println!("Transport Open Close [2d1]");
    ztimeout!(async {
        loop {
            let transports = router_manager.get_transports();
            let s = transports
                .iter()
                .find(|s| s.get_zid().unwrap() == client01_id)
                .unwrap();

            let links = s.get_links().unwrap();
            if links.len() == links_num {
                break;
            }
            task::sleep(SLEEP).await;
        }
    });

    /* [3] */
    // Open transport -> This should be rejected because
    // of the maximum limit of links per transport
    println!("\nTransport Open Close [3a1]");
    let res = ztimeout!(client01_manager.open_transport(endpoint.clone()));
    println!("Transport Open Close [3a2]: {res:?}");
    assert!(res.is_err());
    println!("Transport Open Close [3b1]");
    let transports = client01_manager.get_transports();
    println!("Transport Open Close [3b2]: {transports:?}");
    assert_eq!(transports.len(), 1);
    assert_eq!(c_ses1.get_zid().unwrap(), router_id);
    println!("Transport Open Close [3c1]");
    let links = c_ses1.get_links().unwrap();
    println!("Transport Open Close [3c2]: {links:?}");
    assert_eq!(links.len(), links_num);

    // Verify that the transport has not been open on the router
    println!("Transport Open Close [3d1]");
    ztimeout!(async {
        task::sleep(SLEEP).await;
        let transports = router_manager.get_transports();
        assert_eq!(transports.len(), 1);
        let s = transports
            .iter()
            .find(|s| s.get_zid().unwrap() == client01_id)
            .unwrap();
        let links = s.get_links().unwrap();
        assert_eq!(links.len(), links_num);
    });

    /* [4] */
    // Close the open transport on the client
    println!("\nTransport Open Close [4a1]");
    let res = ztimeout!(c_ses1.close());
    println!("Transport Open Close [4a2]: {res:?}");
    assert!(res.is_ok());
    println!("Transport Open Close [4b1]");
    let transports = client01_manager.get_transports();
    println!("Transport Open Close [4b2]: {transports:?}");
    assert_eq!(transports.len(), 0);

    // Verify that the transport has been closed also on the router
    println!("Transport Open Close [4c1]");
    ztimeout!(async {
        loop {
            let transports = router_manager.get_transports();
            let index = transports
                .iter()
                .find(|s| s.get_zid().unwrap() == client01_id);
            if index.is_none() {
                break;
            }
            task::sleep(SLEEP).await;
        }
    });

    /* [5] */
    // Open transport -> This should be accepted because
    // the number of links should be back to 0
    links_num = 1;

    println!("\nTransport Open Close [5a1]");
    let res = ztimeout!(client01_manager.open_transport(endpoint.clone()));
    println!("Transport Open Close [5a2]: {res:?}");
    assert!(res.is_ok());
    let c_ses3 = res.unwrap();
    println!("Transport Open Close [5b1]");
    let transports = client01_manager.get_transports();
    println!("Transport Open Close [5b2]: {transports:?}");
    assert_eq!(transports.len(), 1);
    assert_eq!(c_ses3.get_zid().unwrap(), router_id);
    println!("Transport Open Close [5c1]");
    let links = c_ses3.get_links().unwrap();
    println!("Transport Open Close [5c2]: {links:?}");
    assert_eq!(links.len(), links_num);

    // Verify that the transport has been open on the router
    println!("Transport Open Close [5d1]");
    ztimeout!(async {
        task::sleep(SLEEP).await;
        let transports = router_manager.get_transports();
        assert_eq!(transports.len(), 1);
        let s = transports
            .iter()
            .find(|s| s.get_zid().unwrap() == client01_id)
            .unwrap();
        let links = s.get_links().unwrap();
        assert_eq!(links.len(), links_num);
    });

    /* [6] */
    // Open transport -> This should be rejected because
    // of the maximum limit of transports
    println!("\nTransport Open Close [6a1]");
    let res = ztimeout!(client02_manager.open_transport(endpoint.clone()));
    println!("Transport Open Close [6a2]: {res:?}");
    assert!(res.is_err());
    println!("Transport Open Close [6b1]");
    let transports = client02_manager.get_transports();
    println!("Transport Open Close [6b2]: {transports:?}");
    assert_eq!(transports.len(), 0);

    // Verify that the transport has not been open on the router
    println!("Transport Open Close [6c1]");
    ztimeout!(async {
        task::sleep(SLEEP).await;
        let transports = router_manager.get_transports();
        assert_eq!(transports.len(), 1);
        let s = transports
            .iter()
            .find(|s| s.get_zid().unwrap() == client01_id)
            .unwrap();
        let links = s.get_links().unwrap();
        assert_eq!(links.len(), links_num);
    });

    /* [7] */
    // Close the open transport on the client
    println!("\nTransport Open Close [7a1]");
    let res = ztimeout!(c_ses3.close());
    println!("Transport Open Close [7a2]: {res:?}");
    assert!(res.is_ok());
    println!("Transport Open Close [7b1]");
    let transports = client01_manager.get_transports();
    println!("Transport Open Close [7b2]: {transports:?}");
    assert_eq!(transports.len(), 0);

    // Verify that the transport has been closed also on the router
    println!("Transport Open Close [7c1]");
    ztimeout!(async {
        loop {
            let transports = router_manager.get_transports();
            if transports.is_empty() {
                break;
            }
            task::sleep(SLEEP).await;
        }
    });

    /* [8] */
    // Open transport -> This should be accepted because
    // the number of transports should be back to 0
    links_num = 1;

    println!("\nTransport Open Close [8a1]");
    let res = ztimeout!(client02_manager.open_transport(endpoint.clone()));
    println!("Transport Open Close [8a2]: {res:?}");
    assert!(res.is_ok());
    let c_ses4 = res.unwrap();
    println!("Transport Open Close [8b1]");
    let transports = client02_manager.get_transports();
    println!("Transport Open Close [8b2]: {transports:?}");
    assert_eq!(transports.len(), 1);
    println!("Transport Open Close [8c1]");
    let links = c_ses4.get_links().unwrap();
    println!("Transport Open Close [8c2]: {links:?}");
    assert_eq!(links.len(), links_num);

    // Verify that the transport has been open on the router
    println!("Transport Open Close [8d1]");
    ztimeout!(async {
        loop {
            let transports = router_manager.get_transports();
            let s = transports
                .iter()
                .find(|s| s.get_zid().unwrap() == client02_id);
            match s {
                Some(s) => {
                    let links = s.get_links().unwrap();
                    assert_eq!(links.len(), links_num);
                    break;
                }
                None => task::sleep(SLEEP).await,
            }
        }
    });

    /* [9] */
    // Close the open transport on the client
    println!("Transport Open Close [9a1]");
    let res = ztimeout!(c_ses4.close());
    println!("Transport Open Close [9a2]: {res:?}");
    assert!(res.is_ok());
    println!("Transport Open Close [9b1]");
    let transports = client02_manager.get_transports();
    println!("Transport Open Close [9b2]: {transports:?}");
    assert_eq!(transports.len(), 0);

    // Verify that the transport has been closed also on the router
    println!("Transport Open Close [9c1]");
    ztimeout!(async {
        loop {
            let transports = router_manager.get_transports();
            if transports.is_empty() {
                break;
            }
            task::sleep(SLEEP).await;
        }
    });

    /* [10] */
    // Perform clean up of the open locators
    println!("\nTransport Open Close [10a1]");
    let res = ztimeout!(router_manager.del_listener(endpoint));
    println!("Transport Open Close [10a2]: {res:?}");
    assert!(res.is_ok());

    ztimeout!(async {
        while !router_manager.get_listeners().is_empty() {
            task::sleep(SLEEP).await;
        }
    });

    // Wait a little bit
    task::sleep(SLEEP).await;

    ztimeout!(router_manager.close());
    ztimeout!(client01_manager.close());
    ztimeout!(client02_manager.close());

    // Wait a little bit
    task::sleep(SLEEP).await;
}

#[cfg(feature = "transport_tcp")]
#[test]
fn openclose_tcp_only() {
    let _ = env_logger::try_init();
    task::block_on(async {
        zasync_executor_init!();
    });

    let endpoint: EndPoint = format!("tcp/127.0.0.1:{}", 13000).parse().unwrap();
    task::block_on(openclose_transport(&endpoint));
}

#[cfg(feature = "transport_udp")]
#[test]
fn openclose_udp_only() {
    let _ = env_logger::try_init();
    task::block_on(async {
        zasync_executor_init!();
    });

    let endpoint: EndPoint = format!("udp/127.0.0.1:{}", 13010).parse().unwrap();
    task::block_on(openclose_transport(&endpoint));
}

#[cfg(feature = "transport_ws")]
#[test]
fn openclose_ws_only() {
    let _ = env_logger::try_init();
    task::block_on(async {
        zasync_executor_init!();
    });

    let endpoint: EndPoint = format!("ws/127.0.0.1:{}", 13020).parse().unwrap();
    task::block_on(openclose_transport(&endpoint));
}

#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
#[test]
fn openclose_unix_only() {
    let _ = env_logger::try_init();
    task::block_on(async {
        zasync_executor_init!();
    });

    let f1 = "zenoh-test-unix-socket-9.sock";
    let _ = std::fs::remove_file(f1);
    let endpoint: EndPoint = format!("unixsock-stream/{f1}").parse().unwrap();
    task::block_on(openclose_transport(&endpoint));
    let _ = std::fs::remove_file(f1);
    let _ = std::fs::remove_file(format!("{f1}.lock"));
}

#[cfg(feature = "transport_tls")]
#[test]
fn openclose_tls_only() {
    use zenoh_link::tls::config::*;

    let _ = env_logger::try_init();
    task::block_on(async {
        zasync_executor_init!();
    });

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
        .extend(
            [
                (TLS_ROOT_CA_CERTIFICATE_RAW, ca),
                (TLS_SERVER_PRIVATE_KEY_RAW, key),
                (TLS_SERVER_CERTIFICATE_RAW, cert),
            ]
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
        )
        .unwrap();

    task::block_on(openclose_transport(&endpoint));
}

#[cfg(feature = "transport_quic")]
#[test]
fn openclose_quic_only() {
    use zenoh_link::quic::config::*;

    task::block_on(async {
        zasync_executor_init!();
    });

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
        .extend(
            [
                (TLS_ROOT_CA_CERTIFICATE_RAW, ca),
                (TLS_SERVER_PRIVATE_KEY_RAW, key),
                (TLS_SERVER_CERTIFICATE_RAW, cert),
            ]
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
        )
        .unwrap();

    task::block_on(openclose_transport(&endpoint));
}
