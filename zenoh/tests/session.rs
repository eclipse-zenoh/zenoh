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
use async_trait::async_trait;
use std::io::Cursor;
use std::time::Duration;
use zenoh::net::protocol::core::{whatami, PeerId};
use zenoh::net::protocol::link::tls::{
    internal::pemfile, ClientConfig, NoClientAuth, ServerConfig,
};
use zenoh::net::protocol::link::{Locator, LocatorProperty};
use zenoh::net::protocol::session::{
    DummySessionEventHandler, Session, SessionDispatcher, SessionEventHandler, SessionHandler,
    SessionManager, SessionManagerConfig, SessionManagerOptionalConfig,
};
use zenoh_util::core::ZResult;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_millis(100);

#[cfg(test)]
struct SHRouterOpenClose;

impl SHRouterOpenClose {
    fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SessionHandler for SHRouterOpenClose {
    async fn new_session(
        &self,
        _session: Session,
    ) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
        Ok(Arc::new(DummySessionEventHandler::new()))
    }
}

// Session Handler for the client
struct SHClientOpenClose {}

impl SHClientOpenClose {
    fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl SessionHandler for SHClientOpenClose {
    async fn new_session(
        &self,
        _session: Session,
    ) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
        Ok(Arc::new(DummySessionEventHandler::new()))
    }
}

async fn session_open_close(locator: Locator, locator_property: Option<Vec<LocatorProperty>>) {
    /* [ROUTER] */
    let router_id = PeerId::new(1, [0u8; PeerId::MAX_SIZE]);

    let router_handler = Arc::new(SHRouterOpenClose::new());
    // Create the router session manager
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::ROUTER,
        id: router_id.clone(),
        handler: SessionDispatcher::SessionHandler(router_handler.clone()),
    };
    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: Some(1),
        max_links: Some(2),
        peer_authenticator: None,
        link_authenticator: None,
        locator_property: locator_property.clone(),
    };
    let router_manager = SessionManager::new(config, Some(opt_config));

    /* [CLIENT] */
    let client01_id = PeerId::new(1, [1u8; PeerId::MAX_SIZE]);
    let client02_id = PeerId::new(1, [2u8; PeerId::MAX_SIZE]);

    // Create the transport session manager for the first client
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::CLIENT,
        id: client01_id.clone(),
        handler: SessionDispatcher::SessionHandler(Arc::new(SHClientOpenClose::new())),
    };
    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: Some(1),
        max_links: Some(2),
        peer_authenticator: None,
        link_authenticator: None,
        locator_property: locator_property.clone(),
    };
    let client01_manager = SessionManager::new(config, Some(opt_config));

    // Create the transport session manager for the second client
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::CLIENT,
        id: client02_id.clone(),
        handler: SessionDispatcher::SessionHandler(Arc::new(SHClientOpenClose::new())),
    };
    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: Some(1),
        max_links: Some(2),
        peer_authenticator: None,
        link_authenticator: None,
        locator_property,
    };
    let client02_manager = SessionManager::new(config, Some(opt_config));

    /* [1] */
    println!("\nSession Open Close [1a1]");
    // Add the locator on the router
    let res = router_manager.add_listener(&locator).await;
    println!("Session Open Close [1a1]: {:?}", res);
    assert!(res.is_ok());
    println!("Session Open Close [1a2]");
    let locators = router_manager.get_listeners().await;
    println!("Session Open Close [1a2]: {:?}", locators);
    assert_eq!(locators.len(), 1);

    // Open a first session from the client to the router
    // -> This should be accepted
    println!("Session Open Close [1c1]");
    let res = client01_manager.open_session(&locator).await;
    println!("Session Open Close [1c2]: {:?}", res);
    assert!(res.is_ok());
    let c_ses1 = res.unwrap();
    println!("Session Open Close [1d1]");
    let sessions = client01_manager.get_sessions().await;
    println!("Session Open Close [1d2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    assert_eq!(c_ses1.get_pid().unwrap(), router_id);
    println!("Session Open Close [1e1]");
    let links = c_ses1.get_links().await.unwrap();
    println!("Session Open Close [1e2]: {:?}", links);
    assert_eq!(links.len(), 1);

    // Verify that the session has been open on the router
    println!("Session Open Close [1f1]");
    let check = async {
        loop {
            let sessions = router_manager.get_sessions().await;
            let s = sessions
                .iter()
                .find(|s| s.get_pid().unwrap() == client01_id);

            match s {
                Some(s) => {
                    let links = s.get_links().await.unwrap();
                    assert_eq!(links.len(), 1);
                    break;
                }
                None => task::sleep(SLEEP).await,
            }
        }
    };
    let res = check.timeout(TIMEOUT).await.unwrap();
    println!("Session Open Close [1f2]: {:?}", res);

    /* [2] */
    // Open a second session from the client to the router
    // -> This should be accepted
    println!("\nSession Open Close [2a1]");
    let res = client01_manager.open_session(&locator).await;
    println!("Session Open Close [2a2]: {:?}", res);
    assert!(res.is_ok());
    let c_ses2 = res.unwrap();
    println!("Session Open Close [2b1]");
    let sessions = client01_manager.get_sessions().await;
    println!("Session Open Close [2b2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    assert_eq!(c_ses2.get_pid().unwrap(), router_id);
    println!("Session Open Close [2c1]");
    let links = c_ses2.get_links().await.unwrap();
    println!("Session Open Close [2c2]: {:?}", links);
    assert_eq!(links.len(), 2);
    assert_eq!(c_ses2, c_ses1);

    // Verify that the session has been open on the router
    println!("Session Open Close [2d1]");
    let check = async {
        loop {
            let sessions = router_manager.get_sessions().await;
            let s = sessions
                .iter()
                .find(|s| s.get_pid().unwrap() == client01_id)
                .unwrap();

            let links = s.get_links().await.unwrap();
            if links.len() == 2 {
                break;
            }
            task::sleep(SLEEP).await;
        }
    };
    let res = check.timeout(TIMEOUT).await.unwrap();
    println!("Session Open Close [2d2]: {:?}", res);

    /* [3] */
    // Open session -> This should be rejected because
    // of the maximum limit of links per session
    println!("\nSession Open Close [3a1]");
    let res = client01_manager.open_session(&locator).await;
    println!("Session Open Close [3a2]: {:?}", res);
    assert!(res.is_err());
    println!("Session Open Close [3b1]");
    let sessions = client01_manager.get_sessions().await;
    println!("Session Open Close [3b2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    assert_eq!(c_ses1.get_pid().unwrap(), router_id);
    println!("Session Open Close [3c1]");
    let links = c_ses1.get_links().await.unwrap();
    println!("Session Open Close [3c2]: {:?}", links);
    assert_eq!(links.len(), 2);

    // Verify that the session has not been open on the router
    println!("Session Open Close [3d1]");
    let check = async {
        task::sleep(SLEEP).await;
        let sessions = router_manager.get_sessions().await;
        assert_eq!(sessions.len(), 1);
        let s = sessions
            .iter()
            .find(|s| s.get_pid().unwrap() == client01_id)
            .unwrap();
        let links = s.get_links().await.unwrap();
        assert_eq!(links.len(), 2);
    };
    let res = check.timeout(TIMEOUT).await.unwrap();
    println!("Session Open Close [3d2]: {:?}", res);

    /* [4] */
    // Close the open session on the client
    println!("\nSession Open Close [4a1]");
    let res = c_ses1.close().await;
    println!("Session Open Close [4a2]: {:?}", res);
    assert!(res.is_ok());
    println!("Session Open Close [4b1]");
    let sessions = client01_manager.get_sessions().await;
    println!("Session Open Close [4b2]: {:?}", sessions);
    assert_eq!(sessions.len(), 0);

    // Verify that the session has been closed also on the router
    println!("Session Open Close [4c1]");
    let check = async {
        loop {
            let sessions = router_manager.get_sessions().await;
            let index = sessions
                .iter()
                .find(|s| s.get_pid().unwrap() == client01_id);
            if index.is_none() {
                break;
            }
            task::sleep(SLEEP).await;
        }
    };
    let res = check.timeout(TIMEOUT).await.unwrap();
    println!("Session Open Close [4c2]: {:?}", res);

    /* [5] */
    // Open session -> This should be accepted because
    // the number of links should be back to 0
    println!("\nSession Open Close [5a1]");
    let res = client01_manager.open_session(&locator).await;
    println!("Session Open Close [5a2]: {:?}", res);
    assert!(res.is_ok());
    let c_ses3 = res.unwrap();
    println!("Session Open Close [5b1]");
    let sessions = client01_manager.get_sessions().await;
    println!("Session Open Close [5b2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    assert_eq!(c_ses3.get_pid().unwrap(), router_id);
    println!("Session Open Close [5c1]");
    let links = c_ses3.get_links().await.unwrap();
    println!("Session Open Close [5c2]: {:?}", links);
    assert_eq!(links.len(), 1);

    // Verify that the session has not been open on the router
    println!("Session Open Close [5d1]");
    let check = async {
        task::sleep(SLEEP).await;
        let sessions = router_manager.get_sessions().await;
        assert_eq!(sessions.len(), 1);
        let s = sessions
            .iter()
            .find(|s| s.get_pid().unwrap() == client01_id)
            .unwrap();
        let links = s.get_links().await.unwrap();
        assert_eq!(links.len(), 1);
    };
    let res = check.timeout(TIMEOUT).await.unwrap();
    println!("Session Open Close [5d2]: {:?}", res);

    /* [6] */
    // Open session -> This should be rejected because
    // of the maximum limit of sessions
    println!("\nSession Open Close [6a1]");
    let res = client02_manager.open_session(&locator).await;
    println!("Session Open Close [6a2]: {:?}", res);
    assert!(res.is_err());
    println!("Session Open Close [6b1]");
    let sessions = client02_manager.get_sessions().await;
    println!("Session Open Close [6b2]: {:?}", sessions);
    assert_eq!(sessions.len(), 0);

    // Verify that the session has not been open on the router
    println!("Session Open Close [6c1]");
    let check = async {
        task::sleep(SLEEP).await;
        let sessions = router_manager.get_sessions().await;
        assert_eq!(sessions.len(), 1);
        let s = sessions
            .iter()
            .find(|s| s.get_pid().unwrap() == client01_id)
            .unwrap();
        let links = s.get_links().await.unwrap();
        assert_eq!(links.len(), 1);
    };
    let res = check.timeout(TIMEOUT).await.unwrap();
    println!("Session Open Close [6c2]: {:?}", res);

    /* [7] */
    // Close the open session on the client
    println!("\nSession Open Close [7a1]");
    let res = c_ses3.close().await;
    println!("Session Open Close [7a2]: {:?}", res);
    assert!(res.is_ok());
    println!("Session Open Close [7b1]");
    let sessions = client01_manager.get_sessions().await;
    println!("Session Open Close [7b2]: {:?}", sessions);
    assert_eq!(sessions.len(), 0);

    // Verify that the session has been closed also on the router
    println!("Session Open Close [7c1]");
    let check = async {
        loop {
            let sessions = router_manager.get_sessions().await;
            if sessions.is_empty() {
                break;
            }
            task::sleep(SLEEP).await;
        }
    };
    let res = check.timeout(TIMEOUT).await.unwrap();
    println!("Session Open Close [7c2]: {:?}", res);

    /* [8] */
    // Open session -> This should be accepted because
    // the number of sessions should be back to 0
    println!("\nSession Open Close [8a1]");
    let res = client02_manager.open_session(&locator).await;
    println!("Session Open Close [8a2]: {:?}", res);
    assert!(res.is_ok());
    let c_ses4 = res.unwrap();
    println!("Session Open Close [8b1]");
    let sessions = client02_manager.get_sessions().await;
    println!("Session Open Close [8b2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    println!("Session Open Close [8c1]");
    let links = c_ses4.get_links().await.unwrap();
    println!("Session Open Close [8c2]: {:?}", links);
    assert_eq!(links.len(), 1);

    // Verify that the session has been open on the router
    println!("Session Open Close [8d1]");
    let check = async {
        loop {
            let sessions = router_manager.get_sessions().await;
            let s = sessions
                .iter()
                .find(|s| s.get_pid().unwrap() == client02_id);
            match s {
                Some(s) => {
                    let links = s.get_links().await.unwrap();
                    assert_eq!(links.len(), 1);
                    break;
                }
                None => task::sleep(SLEEP).await,
            }
        }
    };
    let res = check.timeout(TIMEOUT).await.unwrap();
    println!("Session Open Close [8d2]: {:?}", res);

    /* [9] */
    // Close the open session on the client
    println!("Session Open Close [9a1]");
    let res = c_ses4.close().await;
    println!("Session Open Close [9a2]: {:?}", res);
    assert!(res.is_ok());
    println!("Session Open Close [9b1]");
    let sessions = client02_manager.get_sessions().await;
    println!("Session Open Close [9b2]: {:?}", sessions);
    assert_eq!(sessions.len(), 0);

    // Verify that the session has been closed also on the router
    println!("Session Open Close [9c1]");
    let check = async {
        loop {
            let sessions = router_manager.get_sessions().await;
            if sessions.is_empty() {
                break;
            }
            task::sleep(SLEEP).await;
        }
    };
    let res = check.timeout(TIMEOUT).await.unwrap();
    println!("Session Open Close [9c2]: {:?}", res);

    /* [10] */
    // Perform clean up of the open locators
    println!("\nSession Open Close [10a1]");
    let res = router_manager.del_listener(&locator).await;
    println!("Session Open Close [10a2]: {:?}", res);
    assert!(res.is_ok());

    task::sleep(SLEEP).await;
}

#[cfg(feature = "transport_tcp")]
#[test]
fn session_tcp_only() {
    let locator = "tcp/127.0.0.1:8447".parse().unwrap();
    task::block_on(session_open_close(locator, None));
}

#[cfg(feature = "transport_udp")]
#[test]
fn session_udp_only() {
    let locator = "udp/127.0.0.1:8447".parse().unwrap();
    task::block_on(session_open_close(locator, None));
}

#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
#[test]
fn session_unix_only() {
    let _ = std::fs::remove_file("zenoh-test-unix-socket-9.sock");
    let locator = "unixsock-stream/zenoh-test-unix-socket-9.sock"
        .parse()
        .unwrap();
    task::block_on(session_open_close(locator, None));
    let _ = std::fs::remove_file("zenoh-test-unix-socket-9.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-9.sock.lock");
}

#[cfg(feature = "transport_tls")]
#[test]
fn session_tls_only() {
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
    let mut keys = pemfile::rsa_private_keys(&mut Cursor::new(key.as_bytes())).unwrap();

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
    let certs = pemfile::certs(&mut Cursor::new(cert.as_bytes())).unwrap();

    // Set this server to use one cert together with the loaded private key
    let mut server_config = ServerConfig::new(NoClientAuth::new());
    server_config
        .set_single_cert(certs, keys.remove(0))
        .unwrap();

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

    let mut client_config = ClientConfig::new();
    client_config
        .root_store
        .add_pem_file(&mut Cursor::new(ca.as_bytes()))
        .unwrap();

    let locator = "tls/localhost:8448".parse().unwrap();
    let property = vec![(client_config, server_config).into()];
    task::block_on(session_open_close(locator, Some(property)));
}
