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
use std::time::Duration;
use zenoh::net::link::{EndPoint, Link};
use zenoh::net::protocol::core::{WhatAmI, ZenohId};
use zenoh::net::protocol::message::ZenohMessage;
use zenoh::net::transport::{
    TransportEventHandler, TransportManager, TransportMulticast, TransportMulticastEventHandler,
    TransportPeer, TransportPeerEventHandler, TransportUnicast,
};
use zenoh_util::core::Result as ZResult;
use zenoh_util::properties::Properties;
use zenoh_util::zasync_executor_init;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_millis(100);

const RUNS: usize = 10;

macro_rules! ztimeout {
    ($f:expr) => {
        $f.timeout(TIMEOUT).await.unwrap()
    };
}

// Transport Handler
#[derive(Default)]
struct SH;

impl TransportEventHandler for SH {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        let arc = Arc::new(SC::default());
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
#[derive(Default)]
pub struct SC;

impl TransportPeerEventHandler for SC {
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

async fn run(endpoints: &[EndPoint]) {
    // Create the transport manager
    let sm = TransportManager::builder()
        .whatami(WhatAmI::Peer)
        .zid(ZenohId::new(1, [0_u8; ZenohId::MAX_SIZE]))
        .build(Arc::new(SH::default()))
        .unwrap();

    for _ in 0..RUNS {
        // Create the listeners
        for e in endpoints.iter() {
            println!("Add {}", e);
            let res = ztimeout!(sm.add_listener(e.clone()));
            println!("Res: {:?}", res);
            assert!(res.is_ok());
        }

        task::sleep(SLEEP).await;

        // Delete the listeners
        for e in endpoints.iter() {
            println!("Del {}", e);
            let res = ztimeout!(sm.del_listener(e));
            println!("Res: {:?}", res);
            assert!(res.is_ok());
        }

        task::sleep(SLEEP).await;
    }
}

#[test]
fn endpoint_parsing() {
    let s = "tcp/127.0.0.1:0";
    println!("Endpoint parsing: {}", s);
    let endpoint: EndPoint = s.parse().unwrap();
    assert!(endpoint.config.is_none());
    assert!(endpoint.locator.metadata.is_none());

    let s = "tcp/127.0.0.1:0?one=1";
    println!("Endpoint parsing: {}", s);
    let endpoint: EndPoint = s.parse().unwrap();
    assert!(endpoint.config.is_none());
    let metadata = endpoint.locator.metadata.as_ref().unwrap();
    assert_eq!(metadata.len(), 1);
    assert_eq!(metadata.get("one").unwrap(), "1");

    let s = "tcp/127.0.0.1:0#a=A";
    println!("Endpoint parsing: {}", s);
    let endpoint: EndPoint = s.parse().unwrap();
    assert!(endpoint.locator.metadata.is_none());
    let config = endpoint.config.as_ref().unwrap();
    assert_eq!(config.len(), 1);
    assert_eq!(config.get("a").unwrap(), "A");

    let s = "tcp/127.0.0.1:0?one=1#a=A";
    println!("Endpoint parsing: {}", s);
    let endpoint: EndPoint = s.parse().unwrap();
    let metadata = endpoint.locator.metadata.as_ref().unwrap();
    assert_eq!(metadata.len(), 1);
    assert_eq!(metadata.get("one").unwrap(), "1");
    let config = endpoint.config.as_ref().unwrap();
    assert_eq!(config.len(), 1);
    assert_eq!(config.get("a").unwrap(), "A");

    let s = "tcp/127.0.0.1:0#a=A?one=1";
    println!("Endpoint parsing: {}", s);
    assert!(s.parse::<EndPoint>().is_err());

    let s = "tcp/127.0.0.1:0?one=1;two=2";
    println!("Endpoint parsing: {}", s);
    let endpoint: EndPoint = s.parse().unwrap();
    assert!(endpoint.config.is_none());
    let metadata = endpoint.locator.metadata.as_ref().unwrap();
    assert_eq!(metadata.len(), 2);
    assert_eq!(metadata.get("one").unwrap(), "1");
    assert_eq!(metadata.get("two").unwrap(), "2");

    let s = "tcp/127.0.0.1:0#a=A;b=B";
    println!("Endpoint parsing: {}", s);
    let endpoint: EndPoint = s.parse().unwrap();
    assert!(endpoint.locator.metadata.is_none());
    let config = endpoint.config.as_ref().unwrap();
    assert_eq!(config.len(), 2);
    assert_eq!(config.get("a").unwrap(), "A");
    assert_eq!(config.get("b").unwrap(), "B");

    let s = "tcp/127.0.0.1:0?one=1;two=2#a=A;b=B";
    println!("Endpoint parsing: {}", s);
    let endpoint: EndPoint = s.parse().unwrap();
    let metadata = endpoint.locator.metadata.as_ref().unwrap();
    assert_eq!(metadata.len(), 2);
    assert_eq!(metadata.get("one").unwrap(), "1");
    assert_eq!(metadata.get("two").unwrap(), "2");
    let config = endpoint.config.as_ref().unwrap();
    assert_eq!(config.len(), 2);
    assert_eq!(config.get("a").unwrap(), "A");
    assert_eq!(config.get("b").unwrap(), "B");

    let s = "tcp/127.0.0.1:0?";
    println!("Endpoint parsing: {}", s);
    assert!(s.parse::<EndPoint>().is_err());

    let s = "tcp/127.0.0.1:0#";
    println!("Endpoint parsing: {}", s);
    assert!(s.parse::<EndPoint>().is_err());

    let s = "tcp/127.0.0.1:0?#";
    println!("Endpoint parsing: {}", s);
    assert!(s.parse::<EndPoint>().is_err());

    let s = "tcp/127.0.0.1:0#?";
    println!("Endpoint parsing: {}", s);
    assert!(s.parse::<EndPoint>().is_err());
}

#[cfg(feature = "transport_tcp")]
#[test]
fn endpoint_tcp() {
    task::block_on(async {
        zasync_executor_init!();
    });

    // Define the locators
    let endpoints: Vec<EndPoint> = vec![
        "tcp/127.0.0.1:9447".parse().unwrap(),
        "tcp/[::1]:9447".parse().unwrap(),
        "tcp/localhost:9448".parse().unwrap(),
    ];
    task::block_on(run(&endpoints));
}

#[cfg(feature = "transport_udp")]
#[test]
fn endpoint_udp() {
    task::block_on(async {
        zasync_executor_init!();
    });

    // Define the locators
    let endpoints: Vec<EndPoint> = vec![
        "udp/127.0.0.1:9447".parse().unwrap(),
        "udp/[::1]:9447".parse().unwrap(),
        "udp/localhost:9448".parse().unwrap(),
    ];
    task::block_on(run(&endpoints));
}

#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
#[test]
fn endpoint_unix() {
    task::block_on(async {
        zasync_executor_init!();
    });

    // Remove the files if they still exists
    let _ = std::fs::remove_file("zenoh-test-unix-socket-0.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-1.sock");
    // Define the locators
    let endpoints: Vec<EndPoint> = vec![
        "unixsock-stream/zenoh-test-unix-socket-0.sock"
            .parse()
            .unwrap(),
        "unixsock-stream/zenoh-test-unix-socket-1.sock"
            .parse()
            .unwrap(),
    ];
    task::block_on(run(&endpoints));
    let _ = std::fs::remove_file("zenoh-test-unix-socket-0.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-1.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-0.sock.lock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-1.sock.lock");
}

#[cfg(all(feature = "transport_tcp", feature = "transport_udp"))]
#[test]
fn endpoint_tcp_udp() {
    task::block_on(async {
        zasync_executor_init!();
    });

    // Define the locators
    let endpoints: Vec<EndPoint> = vec![
        "tcp/127.0.0.1:9449".parse().unwrap(),
        "udp/127.0.0.1:9449".parse().unwrap(),
        "tcp/[::1]:9449".parse().unwrap(),
        "udp/[::1]:9449".parse().unwrap(),
    ];
    task::block_on(run(&endpoints));
}

#[cfg(all(
    feature = "transport_tcp",
    feature = "transport_udp",
    feature = "transport_unixsock-stream",
    target_family = "unix"
))]
#[test]
fn endpoint_tcp_udp_unix() {
    task::block_on(async {
        zasync_executor_init!();
    });

    // Remove the file if it still exists
    let _ = std::fs::remove_file("zenoh-test-unix-socket-2.sock");
    // Define the locators
    let endpoints: Vec<EndPoint> = vec![
        "tcp/127.0.0.1:9450".parse().unwrap(),
        "udp/127.0.0.1:9450".parse().unwrap(),
        "tcp/[::1]:9450".parse().unwrap(),
        "udp/[::1]:9450".parse().unwrap(),
        "unixsock-stream/zenoh-test-unix-socket-2.sock"
            .parse()
            .unwrap(),
    ];
    task::block_on(run(&endpoints));
    let _ = std::fs::remove_file("zenoh-test-unix-socket-2.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-2.sock.lock");
}

#[cfg(all(
    feature = "transport_tcp",
    feature = "transport_unixsock-stream",
    target_family = "unix"
))]
#[test]
fn endpoint_tcp_unix() {
    task::block_on(async {
        zasync_executor_init!();
    });

    // Remove the file if it still exists
    let _ = std::fs::remove_file("zenoh-test-unix-socket-3.sock");
    // Define the locators
    let endpoints: Vec<EndPoint> = vec![
        "tcp/127.0.0.1:9451".parse().unwrap(),
        "tcp/[::1]:9451".parse().unwrap(),
        "unixsock-stream/zenoh-test-unix-socket-3.sock"
            .parse()
            .unwrap(),
    ];
    task::block_on(run(&endpoints));
    let _ = std::fs::remove_file("zenoh-test-unix-socket-3.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-3.sock.lock");
}

#[cfg(all(
    feature = "transport_udp",
    feature = "transport_unixsock-stream",
    target_family = "unix"
))]
#[test]
fn endpoint_udp_unix() {
    task::block_on(async {
        zasync_executor_init!();
    });

    // Remove the file if it still exists
    let _ = std::fs::remove_file("zenoh-test-unix-socket-4.sock");
    // Define the locators
    let endpoints: Vec<EndPoint> = vec![
        "udp/127.0.0.1:9451".parse().unwrap(),
        "udp/[::1]:9451".parse().unwrap(),
        "unixsock-stream/zenoh-test-unix-socket-4.sock"
            .parse()
            .unwrap(),
    ];
    task::block_on(run(&endpoints));
    let _ = std::fs::remove_file("zenoh-test-unix-socket-4.sock");
    let _ = std::fs::remove_file("zenoh-test-unix-socket-4.sock.lock");
}

#[cfg(feature = "transport_tls")]
#[test]
fn endpoint_tls() {
    use zenoh::net::link::tls::config::*;

    task::block_on(async {
        zasync_executor_init!();
    });

    // NOTE: this an auto-generated pair of certificate and key.
    //       The target domain is localhost, so it has no real
    //       mapping to any existing domain. The certificate and key
    //       have been generated using: https://github.com/jsha/minica

    // Configure the server
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

    // Define the locators
    let mut endpoint: EndPoint = "tls/localhost:9452".parse().unwrap();
    let mut config = Properties::default();
    config.insert(TLS_SERVER_PRIVATE_KEY_RAW.to_string(), key.to_string());
    config.insert(TLS_SERVER_CERTIFICATE_RAW.to_string(), cert.to_string());
    endpoint.config = Some(Arc::new(config));

    let endpoints = vec![endpoint];
    task::block_on(run(&endpoints));
}

#[cfg(feature = "transport_quic")]
#[test]
fn endpoint_quic() {
    use zenoh::net::link::quic::config::*;

    task::block_on(async {
        zasync_executor_init!();
    });

    // NOTE: this an auto-generated pair of certificate and key.
    //       The target domain is localhost, so it has no real
    //       mapping to any existing domain. The certificate and key
    //       have been generated using: https://github.com/jsha/minica

    // Configure the server
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

    // Define the locators
    let mut endpoint: EndPoint = "quic/localhost:9453".parse().unwrap();
    let mut config = Properties::default();
    config.insert(TLS_SERVER_PRIVATE_KEY_RAW.to_string(), key.to_string());
    config.insert(TLS_SERVER_CERTIFICATE_RAW.to_string(), cert.to_string());
    endpoint.config = Some(Arc::new(config));

    let endpoints = vec![endpoint];
    task::block_on(run(&endpoints));
}
