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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use zenoh::net::protocol::core::{whatami, CongestionControl, PeerId, Reliability, ResKey};
use zenoh::net::protocol::io::RBuf;
use zenoh::net::protocol::link::{Link, Locator, LocatorProperty};
use zenoh::net::protocol::proto::ZenohMessage;
use zenoh::net::protocol::session::{
    Session, SessionDispatcher, SessionEventHandler, SessionHandler, SessionManager,
    SessionManagerConfig, SessionManagerOptionalConfig,
};
use zenoh_util::core::ZResult;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

const MSG_COUNT: usize = 1_000;
const MSG_SIZE_ALL: [usize; 2] = [1_024, 131_072];
const MSG_SIZE_NOFRAG: [usize; 1] = [1_024];

// Session Handler for the router
struct SHRouter {
    count: Arc<AtomicUsize>,
}

impl SHRouter {
    fn new() -> Self {
        Self {
            count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn get_count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl SessionHandler for SHRouter {
    async fn new_session(
        &self,
        _session: Session,
    ) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
        let arc = Arc::new(SCRouter::new(self.count.clone()));
        Ok(arc)
    }
}

// Session Callback for the router
pub struct SCRouter {
    count: Arc<AtomicUsize>,
}

impl SCRouter {
    pub fn new(count: Arc<AtomicUsize>) -> Self {
        Self { count }
    }
}

#[async_trait]
impl SessionEventHandler for SCRouter {
    async fn handle_message(&self, _message: ZenohMessage) -> ZResult<()> {
        self.count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn new_link(&self, _link: Link) {}
    async fn del_link(&self, _link: Link) {}
    async fn closing(&self) {}
    async fn closed(&self) {}
}

// Session Handler for the client
struct SHClient {}

impl SHClient {
    fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl SessionHandler for SHClient {
    async fn new_session(
        &self,
        _session: Session,
    ) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
        Ok(Arc::new(SCClient::new()))
    }
}

// Session Callback for the client
pub struct SCClient;

impl SCClient {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SessionEventHandler for SCClient {
    async fn handle_message(&self, _message: ZenohMessage) -> ZResult<()> {
        Ok(())
    }

    async fn new_link(&self, _link: Link) {}
    async fn del_link(&self, _link: Link) {}
    async fn closing(&self) {}
    async fn closed(&self) {}
}

async fn open_session(
    locators: &[Locator],
    locator_property: Option<Vec<LocatorProperty>>,
) -> (SessionManager, Arc<SHRouter>, Session) {
    // Define client and router IDs
    let client_id = PeerId::new(1, [0u8; PeerId::MAX_SIZE]);
    let router_id = PeerId::new(1, [1u8; PeerId::MAX_SIZE]);

    // Create the router session manager
    let router_handler = Arc::new(SHRouter::new());
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
        max_sessions: None,
        max_links: None,
        peer_authenticator: None,
        link_authenticator: None,
        locator_property: locator_property.clone(),
    };
    let router_manager = SessionManager::new(config, Some(opt_config));

    // Create the client session manager
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::CLIENT,
        id: client_id,
        handler: SessionDispatcher::SessionHandler(Arc::new(SHClient::new())),
    };
    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: None,
        max_links: None,
        peer_authenticator: None,
        link_authenticator: None,
        locator_property,
    };
    let client_manager = SessionManager::new(config, Some(opt_config));

    // Create the listener on the router
    for l in locators.iter() {
        println!("Add locator: {}", l);
        let _ = router_manager
            .add_listener(l)
            .timeout(TIMEOUT)
            .await
            .unwrap()
            .unwrap();
    }

    // Create an empty session with the client
    // Open session -> This should be accepted
    for l in locators.iter() {
        println!("Opening session with {}", l);
        let _ = client_manager
            .open_session(l)
            .timeout(TIMEOUT)
            .await
            .unwrap()
            .unwrap();
    }

    let client_session = client_manager
        .get_session(&router_id)
        .timeout(TIMEOUT)
        .await
        .unwrap()
        .unwrap();

    // Return the handlers
    (router_manager, router_handler, client_session)
}

async fn close_session(
    router_manager: SessionManager,
    client_session: Session,
    locators: &[Locator],
) {
    // Close the client session
    let mut ll = "".to_string();
    for l in locators.iter() {
        ll.push_str(&format!("{} ", l));
    }
    println!("Closing session with {}", ll);
    let _ = client_session
        .close()
        .timeout(TIMEOUT)
        .await
        .unwrap()
        .unwrap();

    // Wait a little bit
    task::sleep(SLEEP).await;

    // Stop the locators on the manager
    for l in locators.iter() {
        println!("Del locator: {}", l);
        let _ = router_manager
            .del_listener(l)
            .timeout(TIMEOUT)
            .await
            .unwrap()
            .unwrap();
    }

    // Wait a little bit
    task::sleep(SLEEP).await;
}

async fn single_run(
    router_handler: Arc<SHRouter>,
    client_session: Session,
    reliability: Reliability,
    congestion_control: CongestionControl,
    msg_size: usize,
) {
    // Create the message to send
    let key = ResKey::RName("/test".to_string());
    let payload = RBuf::from(vec![0u8; msg_size]);
    let data_info = None;
    let routing_context = None;
    let reply_context = None;
    let attachment = None;
    let message = ZenohMessage::make_data(
        key,
        payload,
        reliability,
        congestion_control,
        data_info,
        routing_context,
        reply_context,
        attachment,
    );

    println!(
        "Sending {} messages... {:?} {:?} {}",
        MSG_COUNT, reliability, congestion_control, msg_size
    );
    for _ in 0..MSG_COUNT {
        client_session.schedule(message.clone()).await.unwrap();
    }

    macro_rules! some {
        () => {
            // Wait to receive something
            let count = async {
                while router_handler.get_count() == 0 {
                    task::sleep(SLEEP).await;
                }
            };
            let _ = count.timeout(TIMEOUT).await.unwrap();
        };
    }

    macro_rules! all {
        () => {
            // Wait for the messages to arrive to the other side
            let count = async {
                while router_handler.get_count() != MSG_COUNT {
                    task::sleep(SLEEP).await;
                }
            };
            let _ = count.timeout(TIMEOUT).await.unwrap();
        };
    }

    match reliability {
        Reliability::Reliable => match congestion_control {
            CongestionControl::Block => {
                all!();
            }
            CongestionControl::Drop => {
                some!();
            }
        },
        Reliability::BestEffort => {
            some!();
        }
    }

    // Wait a little bit
    task::sleep(SLEEP).await;
}

async fn run(
    locators: &[Locator],
    properties: Option<Vec<LocatorProperty>>,
    reliability: &[Reliability],
    congestion_control: &[CongestionControl],
    msg_size: &[usize],
) {
    for rl in reliability.iter() {
        for cc in congestion_control.iter() {
            for ms in msg_size.iter() {
                let (router_manager, router_handler, client_session) =
                    open_session(locators, properties.clone()).await;
                single_run(
                    router_handler.clone(),
                    client_session.clone(),
                    *rl,
                    *cc,
                    *ms,
                )
                .await;
                close_session(router_manager, client_session, locators).await;
            }
        }
    }
}

#[cfg(feature = "transport_tcp")]
#[test]
fn transport_tcp_only() {
    // Define the locators
    let locators: Vec<Locator> = vec!["tcp/127.0.0.1:10447".parse().unwrap()];
    let properties = None;
    // Define the reliability and congestion control
    let reliability = [Reliability::Reliable, Reliability::BestEffort];
    let congestion_control = [CongestionControl::Block, CongestionControl::Drop];
    // Run
    task::block_on(run(
        &locators,
        properties,
        &reliability,
        &congestion_control,
        &MSG_SIZE_ALL,
    ));
}

#[cfg(feature = "transport_udp")]
#[test]
fn transport_udp_only() {
    // Define the locator
    let locators: Vec<Locator> = vec!["udp/127.0.0.1:10447".parse().unwrap()];
    let properties = None;
    // Define the reliability and congestion control
    let reliability = [Reliability::BestEffort];
    let congestion_control = [CongestionControl::Block, CongestionControl::Drop];
    // Run
    task::block_on(run(
        &locators,
        properties,
        &reliability,
        &congestion_control,
        &MSG_SIZE_NOFRAG,
    ));
}

#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
#[test]
fn transport_unix_only() {
    let _ = std::fs::remove_file("zenoh-test-unix-socket-5.sock");
    // Define the locator
    let locators: Vec<Locator> = vec!["unixsock-stream/zenoh-test-unix-socket-5.sock"
        .parse()
        .unwrap()];
    let properties = None;
    // Define the reliability and congestion control
    let reliability = [Reliability::BestEffort];
    let congestion_control = [CongestionControl::Block, CongestionControl::Drop];
    // Run
    task::block_on(run(
        &locators,
        properties,
        &reliability,
        &congestion_control,
        &MSG_SIZE_ALL,
    ));
    let _ = std::fs::remove_file("zenoh-test-unix-socket-5.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-5.sock.lock");
}

#[cfg(all(feature = "transport_tcp", feature = "transport_udp"))]
#[test]
fn transport_tcp_udp() {
    // Define the locator
    let locators: Vec<Locator> = vec![
        "tcp/127.0.0.1:10448".parse().unwrap(),
        "udp/127.0.0.1:10448".parse().unwrap(),
    ];
    let properties = None;
    // Define the reliability and congestion control
    let reliability = [Reliability::BestEffort];
    let congestion_control = [CongestionControl::Block, CongestionControl::Drop];
    // Run
    task::block_on(run(
        &locators,
        properties,
        &reliability,
        &congestion_control,
        &MSG_SIZE_NOFRAG,
    ));
}

#[cfg(all(
    feature = "transport_tcp",
    feature = "transport_unixsock-stream",
    target_family = "unix"
))]
#[test]
fn transport_tcp_unix() {
    let _ = std::fs::remove_file("zenoh-test-unix-socket-6.sock");
    // Define the locator
    let locators: Vec<Locator> = vec![
        "tcp/127.0.0.1:10449".parse().unwrap(),
        "unixsock-stream/zenoh-test-unix-socket-6.sock"
            .parse()
            .unwrap(),
    ];
    let properties = None;
    // Define the reliability and congestion control
    let reliability = [Reliability::BestEffort];
    let congestion_control = [CongestionControl::Block, CongestionControl::Drop];
    // Run
    task::block_on(run(
        &locators,
        properties,
        &reliability,
        &congestion_control,
        &MSG_SIZE_ALL,
    ));
    let _ = std::fs::remove_file("zenoh-test-unix-socket-6.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-6.sock.lock");
}

#[cfg(all(
    feature = "transport_udp",
    feature = "transport_unixsock-stream",
    target_family = "unix"
))]
#[test]
fn transport_udp_unix() {
    let _ = std::fs::remove_file("zenoh-test-unix-socket-7.sock");
    // Define the locator
    let locators: Vec<Locator> = vec![
        "udp/127.0.0.1:10449".parse().unwrap(),
        "unixsock-stream/zenoh-test-unix-socket-7.sock"
            .parse()
            .unwrap(),
    ];
    let properties = None;
    // Define the reliability and congestion control
    let reliability = [Reliability::BestEffort];
    let congestion_control = [CongestionControl::Block, CongestionControl::Drop];
    // Run
    task::block_on(run(
        &locators,
        properties,
        &reliability,
        &congestion_control,
        &MSG_SIZE_NOFRAG,
    ));
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
fn transport_tcp_udp_unix() {
    let _ = std::fs::remove_file("zenoh-test-unix-socket-8.sock");
    // Define the locator
    let locators: Vec<Locator> = vec![
        "tcp/127.0.0.1:10450".parse().unwrap(),
        "udp/127.0.0.1:10450".parse().unwrap(),
        "unixsock-stream/zenoh-test-unix-socket-8.sock"
            .parse()
            .unwrap(),
    ];
    let properties = None;
    // Define the reliability and congestion control
    let reliability = [Reliability::BestEffort];
    let congestion_control = [CongestionControl::Block, CongestionControl::Drop];
    // Run
    task::block_on(run(
        &locators,
        properties,
        &reliability,
        &congestion_control,
        &MSG_SIZE_NOFRAG,
    ));
    let _ = std::fs::remove_file("zenoh-test-unix-socket-8.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-8.sock.lock");
}

#[cfg(feature = "transport_tls")]
#[test]
fn transport_tls() {
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
    let locators = vec!["tls/localhost:10451".parse().unwrap()];
    let properties = vec![(client_config, server_config).into()];

    // Define the reliability and congestion control
    let reliability = [Reliability::BestEffort];
    let congestion_control = [CongestionControl::Block, CongestionControl::Drop];
    // Run
    task::block_on(run(
        &locators,
        Some(properties),
        &reliability,
        &congestion_control,
        &MSG_SIZE_ALL,
    ));
}

#[cfg(feature = "transport_quic")]
#[test]
fn transport_quic() {
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
    let locators = vec!["quic/localhost:10452".parse().unwrap()];
    let properties = vec![(client_config, server_config).into()];

    // Define the reliability and congestion control
    let reliability = [Reliability::BestEffort];
    let congestion_control = [CongestionControl::Block, CongestionControl::Drop];
    // Run
    task::block_on(run(
        &locators,
        Some(properties),
        &reliability,
        &congestion_control,
        &MSG_SIZE_ALL,
    ));
}
