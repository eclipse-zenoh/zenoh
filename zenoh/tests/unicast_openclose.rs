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
use std::time::Duration;
use zenoh::net::link::EndPoint;
use zenoh::net::protocol::core::{WhatAmI, ZenohId};
use zenoh::net::transport::{
    DummyTransportPeerEventHandler, TransportEventHandler, TransportManager, TransportMulticast,
    TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler, TransportUnicast,
};
use zenoh_util::core::Result as ZResult;
use zenoh_util::properties::Properties;
use zenoh_util::zasync_executor_init;

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
    let router_id = ZenohId::new(1, [0_u8; ZenohId::MAX_SIZE]);

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
    let client01_id = ZenohId::new(1, [1_u8; ZenohId::MAX_SIZE]);
    let client02_id = ZenohId::new(1, [2_u8; ZenohId::MAX_SIZE]);

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
    println!("Transport Open Close [1a1]: {:?}", res);
    assert!(res.is_ok());
    println!("Transport Open Close [1a2]");
    let locators = router_manager.get_listeners();
    println!("Transport Open Close [1a2]: {:?}", locators);
    assert_eq!(locators.len(), 1);

    // Open a first transport from the client to the router
    // -> This should be accepted
    let mut links_num = 1;

    println!("Transport Open Close [1c1]");
    let res = ztimeout!(client01_manager.open_transport(endpoint.clone()));
    println!("Transport Open Close [1c2]: {:?}", res);
    assert!(res.is_ok());
    let c_ses1 = res.unwrap();
    println!("Transport Open Close [1d1]");
    let transports = client01_manager.get_transports();
    println!("Transport Open Close [1d2]: {:?}", transports);
    assert_eq!(transports.len(), 1);
    assert_eq!(c_ses1.get_zid().unwrap(), router_id);
    println!("Transport Open Close [1e1]");
    let links = c_ses1.get_links().unwrap();
    println!("Transport Open Close [1e2]: {:?}", links);
    assert_eq!(links.len(), links_num);

    // Verify that the transport has been open on the router
    println!("Transport Open Close [1f1]");
    let res = ztimeout!(async {
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
    println!("Transport Open Close [1f2]: {:?}", res);

    /* [2] */
    // Open a second transport from the client to the router
    // -> This should be accepted
    links_num = 2;

    println!("\nTransport Open Close [2a1]");
    let res = ztimeout!(client01_manager.open_transport(endpoint.clone()));
    println!("Transport Open Close [2a2]: {:?}", res);
    assert!(res.is_ok());
    let c_ses2 = res.unwrap();
    println!("Transport Open Close [2b1]");
    let transports = client01_manager.get_transports();
    println!("Transport Open Close [2b2]: {:?}", transports);
    assert_eq!(transports.len(), 1);
    assert_eq!(c_ses2.get_zid().unwrap(), router_id);
    println!("Transport Open Close [2c1]");
    let links = c_ses2.get_links().unwrap();
    println!("Transport Open Close [2c2]: {:?}", links);
    assert_eq!(links.len(), links_num);
    assert_eq!(c_ses2, c_ses1);

    // Verify that the transport has been open on the router
    println!("Transport Open Close [2d1]");
    let res = ztimeout!(async {
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
    println!("Transport Open Close [2d2]: {:?}", res);

    /* [3] */
    // Open transport -> This should be rejected because
    // of the maximum limit of links per transport
    println!("\nTransport Open Close [3a1]");
    let res = ztimeout!(client01_manager.open_transport(endpoint.clone()));
    println!("Transport Open Close [3a2]: {:?}", res);
    assert!(res.is_err());
    println!("Transport Open Close [3b1]");
    let transports = client01_manager.get_transports();
    println!("Transport Open Close [3b2]: {:?}", transports);
    assert_eq!(transports.len(), 1);
    assert_eq!(c_ses1.get_zid().unwrap(), router_id);
    println!("Transport Open Close [3c1]");
    let links = c_ses1.get_links().unwrap();
    println!("Transport Open Close [3c2]: {:?}", links);
    assert_eq!(links.len(), links_num);

    // Verify that the transport has not been open on the router
    println!("Transport Open Close [3d1]");
    let res = ztimeout!(async {
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
    println!("Transport Open Close [3d2]: {:?}", res);

    /* [4] */
    // Close the open transport on the client
    println!("\nTransport Open Close [4a1]");
    let res = ztimeout!(c_ses1.close());
    println!("Transport Open Close [4a2]: {:?}", res);
    assert!(res.is_ok());
    println!("Transport Open Close [4b1]");
    let transports = client01_manager.get_transports();
    println!("Transport Open Close [4b2]: {:?}", transports);
    assert_eq!(transports.len(), 0);

    // Verify that the transport has been closed also on the router
    println!("Transport Open Close [4c1]");
    let res = ztimeout!(async {
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
    println!("Transport Open Close [4c2]: {:?}", res);

    /* [5] */
    // Open transport -> This should be accepted because
    // the number of links should be back to 0
    links_num = 1;

    println!("\nTransport Open Close [5a1]");
    let res = ztimeout!(client01_manager.open_transport(endpoint.clone()));
    println!("Transport Open Close [5a2]: {:?}", res);
    assert!(res.is_ok());
    let c_ses3 = res.unwrap();
    println!("Transport Open Close [5b1]");
    let transports = client01_manager.get_transports();
    println!("Transport Open Close [5b2]: {:?}", transports);
    assert_eq!(transports.len(), 1);
    assert_eq!(c_ses3.get_zid().unwrap(), router_id);
    println!("Transport Open Close [5c1]");
    let links = c_ses3.get_links().unwrap();
    println!("Transport Open Close [5c2]: {:?}", links);
    assert_eq!(links.len(), links_num);

    // Verify that the transport has been open on the router
    println!("Transport Open Close [5d1]");
    let res = ztimeout!(async {
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
    println!("Transport Open Close [5d2]: {:?}", res);

    /* [6] */
    // Open transport -> This should be rejected because
    // of the maximum limit of transports
    println!("\nTransport Open Close [6a1]");
    let res = ztimeout!(client02_manager.open_transport(endpoint.clone()));
    println!("Transport Open Close [6a2]: {:?}", res);
    assert!(res.is_err());
    println!("Transport Open Close [6b1]");
    let transports = client02_manager.get_transports();
    println!("Transport Open Close [6b2]: {:?}", transports);
    assert_eq!(transports.len(), 0);

    // Verify that the transport has not been open on the router
    println!("Transport Open Close [6c1]");
    let res = ztimeout!(async {
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
    println!("Transport Open Close [6c2]: {:?}", res);

    /* [7] */
    // Close the open transport on the client
    println!("\nTransport Open Close [7a1]");
    let res = ztimeout!(c_ses3.close());
    println!("Transport Open Close [7a2]: {:?}", res);
    assert!(res.is_ok());
    println!("Transport Open Close [7b1]");
    let transports = client01_manager.get_transports();
    println!("Transport Open Close [7b2]: {:?}", transports);
    assert_eq!(transports.len(), 0);

    // Verify that the transport has been closed also on the router
    println!("Transport Open Close [7c1]");
    let res = ztimeout!(async {
        loop {
            let transports = router_manager.get_transports();
            if transports.is_empty() {
                break;
            }
            task::sleep(SLEEP).await;
        }
    });
    println!("Transport Open Close [7c2]: {:?}", res);

    /* [8] */
    // Open transport -> This should be accepted because
    // the number of transports should be back to 0
    links_num = 1;

    println!("\nTransport Open Close [8a1]");
    let res = ztimeout!(client02_manager.open_transport(endpoint.clone()));
    println!("Transport Open Close [8a2]: {:?}", res);
    assert!(res.is_ok());
    let c_ses4 = res.unwrap();
    println!("Transport Open Close [8b1]");
    let transports = client02_manager.get_transports();
    println!("Transport Open Close [8b2]: {:?}", transports);
    assert_eq!(transports.len(), 1);
    println!("Transport Open Close [8c1]");
    let links = c_ses4.get_links().unwrap();
    println!("Transport Open Close [8c2]: {:?}", links);
    assert_eq!(links.len(), links_num);

    // Verify that the transport has been open on the router
    println!("Transport Open Close [8d1]");
    let res = ztimeout!(async {
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
    println!("Transport Open Close [8d2]: {:?}", res);

    /* [9] */
    // Close the open transport on the client
    println!("Transport Open Close [9a1]");
    let res = ztimeout!(c_ses4.close());
    println!("Transport Open Close [9a2]: {:?}", res);
    assert!(res.is_ok());
    println!("Transport Open Close [9b1]");
    let transports = client02_manager.get_transports();
    println!("Transport Open Close [9b2]: {:?}", transports);
    assert_eq!(transports.len(), 0);

    // Verify that the transport has been closed also on the router
    println!("Transport Open Close [9c1]");
    let res = ztimeout!(async {
        loop {
            let transports = router_manager.get_transports();
            if transports.is_empty() {
                break;
            }
            task::sleep(SLEEP).await;
        }
    });
    println!("Transport Open Close [9c2]: {:?}", res);

    /* [10] */
    // Perform clean up of the open locators
    println!("\nTransport Open Close [10a1]");
    let res = ztimeout!(router_manager.del_listener(endpoint));
    println!("Transport Open Close [10a2]: {:?}", res);
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
    task::block_on(async {
        zasync_executor_init!();
    });

    let endpoint: EndPoint = "tcp/127.0.0.1:8447".parse().unwrap();
    task::block_on(openclose_transport(&endpoint));
}

#[cfg(feature = "transport_udp")]
#[test]
fn openclose_udp_only() {
    task::block_on(async {
        zasync_executor_init!();
    });

    let endpoint: EndPoint = "udp/127.0.0.1:8447".parse().unwrap();
    task::block_on(openclose_transport(&endpoint));
}

#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
#[test]
fn openclose_unix_only() {
    task::block_on(async {
        zasync_executor_init!();
    });

    let _ = std::fs::remove_file("zenoh-test-unix-socket-9.sock");
    let endpoint: EndPoint = "unixsock-stream/zenoh-test-unix-socket-9.sock"
        .parse()
        .unwrap();
    task::block_on(openclose_transport(&endpoint));
    let _ = std::fs::remove_file("zenoh-test-unix-socket-9.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-9.sock.lock");
}

#[cfg(feature = "transport_tls")]
#[test]
fn openclose_tls_only() {
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

    let mut endpoint: EndPoint = "tls/localhost:8448".parse().unwrap();
    let mut config = Properties::default();
    config.insert(TLS_ROOT_CA_CERTIFICATE_RAW.to_string(), ca.to_string());
    config.insert(TLS_SERVER_PRIVATE_KEY_RAW.to_string(), key.to_string());
    config.insert(TLS_SERVER_CERTIFICATE_RAW.to_string(), cert.to_string());
    endpoint.config = Some(Arc::new(config));

    task::block_on(openclose_transport(&endpoint));
}

#[cfg(feature = "transport_quic")]
#[test]
fn openclose_quic_only() {
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
    let mut endpoint: EndPoint = "quic/localhost:8449".parse().unwrap();
    let mut config = Properties::default();
    config.insert(TLS_ROOT_CA_CERTIFICATE_RAW.to_string(), ca.to_string());
    config.insert(TLS_SERVER_PRIVATE_KEY_RAW.to_string(), key.to_string());
    config.insert(TLS_SERVER_CERTIFICATE_RAW.to_string(), cert.to_string());
    endpoint.config = Some(Arc::new(config));

    task::block_on(openclose_transport(&endpoint));
}
