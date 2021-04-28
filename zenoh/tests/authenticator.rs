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
use async_std::sync::Arc;
use async_std::task;
use async_trait::async_trait;
use std::collections::HashMap;
use std::time::Duration;
use zenoh::net::protocol::core::{whatami, PeerId};
use zenoh::net::protocol::link::{Link, Locator, LocatorProperty};
use zenoh::net::protocol::proto::ZenohMessage;
#[cfg(feature = "zero-copy")]
use zenoh::net::protocol::session::authenticator::SharedMemoryAuthenticator;
use zenoh::net::protocol::session::authenticator::UserPasswordAuthenticator;
use zenoh::net::protocol::session::{
    DummySessionEventHandler, Session, SessionDispatcher, SessionEventHandler, SessionHandler,
    SessionManager, SessionManagerConfig, SessionManagerOptionalConfig,
};
use zenoh_util::core::ZResult;

const SLEEP: Duration = Duration::from_millis(100);

#[cfg(test)]
struct SHRouterAuthenticator;

impl SHRouterAuthenticator {
    fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SessionHandler for SHRouterAuthenticator {
    async fn new_session(
        &self,
        _session: Session,
    ) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
        Ok(Arc::new(MHRouterAuthenticator::new()))
    }
}

struct MHRouterAuthenticator;

impl MHRouterAuthenticator {
    fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SessionEventHandler for MHRouterAuthenticator {
    async fn handle_message(&self, _msg: ZenohMessage) -> ZResult<()> {
        Ok(())
    }
    async fn new_link(&self, _link: Link) {}
    async fn del_link(&self, _link: Link) {}
    async fn closing(&self) {}
    async fn closed(&self) {}
}

// Session Handler for the client
struct SHClientAuthenticator {}

impl SHClientAuthenticator {
    fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl SessionHandler for SHClientAuthenticator {
    async fn new_session(
        &self,
        _session: Session,
    ) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
        Ok(Arc::new(DummySessionEventHandler::new()))
    }
}

async fn authenticator_user_password(
    locator: Locator,
    locator_property: Option<Vec<LocatorProperty>>,
) {
    /* [CLIENT] */
    let client01_id = PeerId::new(1, [1u8; PeerId::MAX_SIZE]);
    let user01 = "user01".to_string();
    let password01 = "password01".to_string();

    let client02_id = PeerId::new(1, [2u8; PeerId::MAX_SIZE]);
    let user02 = "invalid".to_string();
    let password02 = "invalid".to_string();

    let client03_id = client01_id.clone();
    let user03 = "user03".to_string();
    let password03 = "password03".to_string();

    /* [ROUTER] */
    let router_id = PeerId::new(1, [0u8; PeerId::MAX_SIZE]);
    let router_handler = Arc::new(SHRouterAuthenticator::new());
    // Create the router session manager
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::ROUTER,
        id: router_id.clone(),
        handler: SessionDispatcher::SessionHandler(router_handler.clone()),
    };

    let mut lookup: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
    lookup.insert(user01.clone().into(), password01.clone().into());
    lookup.insert(user03.clone().into(), password03.clone().into());

    let peer_authenticator_router = Arc::new(UserPasswordAuthenticator::new(lookup, None));
    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: None,
        max_links: None,
        peer_authenticator: Some(vec![peer_authenticator_router.clone().into()]),
        link_authenticator: None,
        locator_property: locator_property.clone(),
    };
    let router_manager = SessionManager::new(config, Some(opt_config));

    // Create the transport session manager for the first client
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::CLIENT,
        id: client01_id.clone(),
        handler: SessionDispatcher::SessionHandler(Arc::new(SHClientAuthenticator::new())),
    };
    let lookup: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
    let peer_authenticator_client01 = UserPasswordAuthenticator::new(
        lookup,
        Some((user01.clone().into(), password01.clone().into())),
    );
    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: None,
        max_links: None,
        peer_authenticator: Some(vec![peer_authenticator_client01.into()]),
        link_authenticator: None,
        locator_property: locator_property.clone(),
    };
    let client01_manager = SessionManager::new(config, Some(opt_config));

    // Create the transport session manager for the second client
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::CLIENT,
        id: client02_id.clone(),
        handler: SessionDispatcher::SessionHandler(Arc::new(SHClientAuthenticator::new())),
    };
    let lookup: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
    let peer_authenticator_client02 = UserPasswordAuthenticator::new(
        lookup,
        Some((user02.clone().into(), password02.clone().into())),
    );
    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: None,
        max_links: None,
        peer_authenticator: Some(vec![peer_authenticator_client02.into()]),
        link_authenticator: None,
        locator_property: locator_property.clone(),
    };
    let client02_manager = SessionManager::new(config, Some(opt_config));

    // Create the transport session manager for the third client
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::CLIENT,
        id: client03_id.clone(),
        handler: SessionDispatcher::SessionHandler(Arc::new(SHClientAuthenticator::new())),
    };
    let lookup: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
    let peer_authenticator_client03 =
        UserPasswordAuthenticator::new(lookup, Some((user03.into(), password03.into())));
    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: None,
        max_links: None,
        peer_authenticator: Some(vec![peer_authenticator_client03.into()]),
        link_authenticator: None,
        locator_property,
    };
    let client03_manager = SessionManager::new(config, Some(opt_config));

    /* [1] */
    println!("\nSession Authenticator UserPassword [1a1]");
    // Add the locator on the router
    let res = router_manager.add_listener(&locator).await;
    println!("Session Authenticator UserPassword [1a1]: {:?}", res);
    assert!(res.is_ok());
    println!("Session Authenticator UserPassword [1a2]");
    let locators = router_manager.get_listeners().await;
    println!("Session Authenticator UserPassword [1a2]: {:?}", locators);
    assert_eq!(locators.len(), 1);

    /* [2] */
    // Open a first session from the client to the router
    // -> This should be accepted
    println!("Session Authenticator UserPassword [2a1]");
    let res = client01_manager.open_session(&locator).await;
    println!("Session Authenticator UserPassword [2a1]: {:?}", res);
    assert!(res.is_ok());
    let c_ses1 = res.unwrap();

    /* [3] */
    println!("Session Authenticator UserPassword [3a1]");
    let res = c_ses1.close().await;
    println!("Session Authenticator UserPassword [3a1]: {:?}", res);
    assert!(res.is_ok());

    /* [4] */
    // Open a second session from the client to the router
    // -> This should be rejected
    println!("Session Authenticator UserPassword [4a1]");
    let res = client02_manager.open_session(&locator).await;
    println!("Session Authenticator UserPassword [4a1]: {:?}", res);
    assert!(res.is_err());

    /* [5] */
    // Open a third session from the client to the router
    // -> This should be accepted
    println!("Session Authenticator UserPassword [5a1]");
    let res = client01_manager.open_session(&locator).await;
    println!("Session Authenticator UserPassword [5a1]: {:?}", res);
    assert!(res.is_ok());
    let c_ses1 = res.unwrap();

    /* [6] */
    // Add client02 credentials on the router
    let res = peer_authenticator_router
        .add_user(user02.into(), password02.into())
        .await;
    assert!(res.is_ok());
    // Open a fourth session from the client to the router
    // -> This should be accepted
    println!("Session Authenticator UserPassword [6a1]");
    let res = client02_manager.open_session(&locator).await;
    println!("Session Authenticator UserPassword [6a1]: {:?}", res);
    assert!(res.is_ok());
    let c_ses2 = res.unwrap();

    /* [7] */
    // Open a fourth session from the client to the router
    // -> This should be rejected
    println!("Session Authenticator UserPassword [7a1]");
    let res = client03_manager.open_session(&locator).await;
    println!("Session Authenticator UserPassword [7a1]: {:?}", res);
    assert!(res.is_err());

    /* [8] */
    println!("Session Authenticator UserPassword [8a1]");
    let res = c_ses1.close().await;
    println!("Session Authenticator UserPassword [8a1]: {:?}", res);
    assert!(res.is_ok());
    println!("Session Authenticator UserPassword [8a2]");
    let res = c_ses2.close().await;
    println!("Session Authenticator UserPassword [8a2]: {:?}", res);
    assert!(res.is_ok());

    task::sleep(SLEEP).await;

    /* [9] */
    // Perform clean up of the open locators
    println!("Session Authenticator UserPassword [9a1]");
    let res = router_manager.del_listener(&locator).await;
    println!("Session Authenticator UserPassword [9a2]: {:?}", res);
    assert!(res.is_ok());

    task::sleep(SLEEP).await;
}

#[cfg(feature = "zero-copy")]
async fn authenticator_shared_memory(
    locator: Locator,
    locator_property: Option<Vec<LocatorProperty>>,
) {
    /* [CLIENT] */
    let client_id = PeerId::new(1, [1u8; PeerId::MAX_SIZE]);

    /* [ROUTER] */
    let router_id = PeerId::new(1, [0u8; PeerId::MAX_SIZE]);
    let router_handler = Arc::new(SHRouterAuthenticator::new());
    // Create the router session manager
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::ROUTER,
        id: router_id.clone(),
        handler: SessionDispatcher::SessionHandler(router_handler.clone()),
    };

    let peer_authenticator_router = SharedMemoryAuthenticator::new();
    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: None,
        max_links: None,
        peer_authenticator: Some(vec![peer_authenticator_router.into()]),
        link_authenticator: None,
        locator_property: locator_property.clone(),
    };
    let router_manager = SessionManager::new(config, Some(opt_config));

    // Create the transport session manager for the first client
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::CLIENT,
        id: client_id.clone(),
        handler: SessionDispatcher::SessionHandler(Arc::new(SHClientAuthenticator::new())),
    };
    let peer_authenticator_client = SharedMemoryAuthenticator::new();
    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: None,
        max_links: None,
        peer_authenticator: Some(vec![peer_authenticator_client.into()]),
        link_authenticator: None,
        locator_property,
    };
    let client_manager = SessionManager::new(config, Some(opt_config));

    /* [1] */
    println!("\nSession Authenticator SharedMemory [1a1]");
    // Add the locator on the router
    let res = router_manager.add_listener(&locator).await;
    println!("Session Authenticator SharedMemory [1a1]: {:?}", res);
    assert!(res.is_ok());
    println!("Session Authenticator SharedMemory [1a2]");
    let locators = router_manager.get_listeners().await;
    println!("Session Authenticator SharedMemory 1a2]: {:?}", locators);
    assert_eq!(locators.len(), 1);

    /* [2] */
    // Open a session from the client to the router
    // -> This should be accepted
    println!("Session Authenticator SharedMemory [2a1]");
    let res = client_manager.open_session(&locator).await;
    println!("Session Authenticator SharedMemory [2a1]: {:?}", res);
    assert!(res.is_ok());
    let c_ses1 = res.unwrap();
    assert!(c_ses1.is_shm().unwrap());

    /* [3] */
    println!("Session Authenticator SharedMemory [3a1]");
    let res = c_ses1.close().await;
    println!("Session Authenticator SharedMemory [3a1]: {:?}", res);
    assert!(res.is_ok());

    task::sleep(SLEEP).await;

    /* [4] */
    // Perform clean up of the open locators
    println!("Session Authenticator SharedMemory [4a1]");
    let res = router_manager.del_listener(&locator).await;
    println!("Session Authenticator SharedMemory [4a2]: {:?}", res);
    assert!(res.is_ok());

    task::sleep(SLEEP).await;
}

#[cfg(feature = "transport_tcp")]
#[test]
fn authenticator_tcp() {
    let locator: Locator = "tcp/127.0.0.1:11447".parse().unwrap();
    task::block_on(async {
        authenticator_user_password(locator.clone(), None).await;
        #[cfg(feature = "zero-copy")]
        authenticator_shared_memory(locator, None).await;
    });
}

#[cfg(feature = "transport_udp")]
#[test]
fn authenticator_udp() {
    let locator: Locator = "udp/127.0.0.1:11447".parse().unwrap();
    task::block_on(async {
        authenticator_user_password(locator.clone(), None).await;
        #[cfg(feature = "zero-copy")]
        authenticator_shared_memory(locator, None).await;
    });
}

#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
#[test]
fn authenticator_unix() {
    let _ = std::fs::remove_file("zenoh-test-unix-socket-10.sock");
    let locator: Locator = "unixsock-stream/zenoh-test-unix-socket-10.sock"
        .parse()
        .unwrap();
    task::block_on(async {
        authenticator_user_password(locator.clone(), None).await;
        #[cfg(feature = "zero-copy")]
        authenticator_shared_memory(locator, None).await;
    });
    let _ = std::fs::remove_file("zenoh-test-unix-socket-10.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-10.sock.lock");
}

#[cfg(feature = "transport_tls")]
#[test]
fn authenticator_tls() {
    use std::io::Cursor;
    use zenoh::net::protocol::link::tls::{
        internal::pemfile, ClientConfig, NoClientAuth, ServerConfig,
    };

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

    // Define the locator
    let locator: Locator = "tls/localhost:11448".parse().unwrap();
    let locator_property = vec![(client_config, server_config).into()];
    task::block_on(async {
        authenticator_user_password(locator.clone(), Some(locator_property.clone())).await;
        #[cfg(feature = "zero-copy")]
        authenticator_shared_memory(locator, Some(locator_property)).await;
    });
}

#[cfg(feature = "transport_quic")]
#[test]
fn authenticator_quic() {
    use zenoh::net::protocol::link::quic::{
        Certificate, CertificateChain, ClientConfigBuilder, PrivateKey, ServerConfig,
        ServerConfigBuilder, TransportConfig, ALPN_QUIC_HTTP,
    };

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
    let keys = PrivateKey::from_pem(key.as_bytes()).unwrap();

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
    let certs = CertificateChain::from_pem(cert.as_bytes()).unwrap();

    // Set this server to use one cert together with the loaded private key
    let mut transport_config = TransportConfig::default();
    // We do not accept unidireactional streams.
    transport_config.max_concurrent_uni_streams(0).unwrap();
    // For the time being we only allow one bidirectional stream
    transport_config.max_concurrent_bidi_streams(1).unwrap();
    let mut server_config = ServerConfig::default();
    server_config.transport = Arc::new(transport_config);
    let mut server_config = ServerConfigBuilder::new(server_config);
    server_config.protocols(ALPN_QUIC_HTTP);
    server_config.certificate(certs, keys).unwrap();

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
    let ca = Certificate::from_pem(ca.as_bytes()).unwrap();

    let mut client_config = ClientConfigBuilder::default();
    client_config.protocols(ALPN_QUIC_HTTP);
    client_config.add_certificate_authority(ca).unwrap();

    // Define the locator
    let locator: Locator = "quic/localhost:11448".parse().unwrap();
    let locator_property = vec![(client_config, server_config).into()];
    task::block_on(async {
        authenticator_user_password(locator.clone(), Some(locator_property.clone())).await;
        #[cfg(feature = "zero-copy")]
        authenticator_shared_memory(locator, Some(locator_property)).await;
    });
}
