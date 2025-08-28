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
use std::{any::Any, convert::TryFrom, sync::Arc, time::Duration};

use zenoh_core::ztimeout;
use zenoh_link::{EndPoint, Link};
use zenoh_protocol::{
    core::{WhatAmI, ZenohIdProto},
    network::NetworkMessageMut,
};
use zenoh_result::ZResult;
use zenoh_transport::{
    multicast::TransportMulticast, unicast::TransportUnicast, TransportEventHandler,
    TransportManager, TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler,
};

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_millis(100);

const RUNS: usize = 10;

// Transport Handler
#[derive(Default)]
struct SH;

impl TransportEventHandler for SH {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        let arc = Arc::new(SC);
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
    fn handle_message(&self, _message: NetworkMessageMut) -> ZResult<()> {
        Ok(())
    }
    fn new_link(&self, _link: Link) {}
    fn del_link(&self, _link: Link) {}
    fn closed(&self) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}

async fn run(endpoints: &[EndPoint]) {
    // Create the transport manager
    let sm = TransportManager::builder()
        .whatami(WhatAmI::Peer)
        .zid(ZenohIdProto::try_from([1]).unwrap())
        .build(Arc::new(SH))
        .unwrap();

    for _ in 0..RUNS {
        // Create the listeners
        for e in endpoints.iter() {
            println!("Add {e}");
            ztimeout!(sm.add_listener(e.clone())).unwrap();
        }

        tokio::time::sleep(SLEEP).await;

        // Delete the listeners
        for e in endpoints.iter() {
            println!("Del {e}");
            ztimeout!(sm.del_listener(e)).unwrap();
        }

        tokio::time::sleep(SLEEP).await;
    }
}

#[cfg(feature = "transport_tcp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn endpoint_tcp() {
    zenoh_util::init_log_from_env_or("error");
    // Define the locators
    let endpoints: Vec<EndPoint> = vec![
        format!("tcp/127.0.0.1:{}", 7000).parse().unwrap(),
        format!("tcp/[::1]:{}", 7001).parse().unwrap(),
        format!("tcp/localhost:{}", 7002).parse().unwrap(),
    ];
    run(&endpoints).await;
}

#[cfg(feature = "transport_udp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn endpoint_udp() {
    zenoh_util::init_log_from_env_or("error");
    // Define the locators
    let endpoints: Vec<EndPoint> = vec![
        format!("udp/127.0.0.1:{}", 7010).parse().unwrap(),
        format!("udp/[::1]:{}", 7011).parse().unwrap(),
        format!("udp/localhost:{}", 7012).parse().unwrap(),
    ];
    run(&endpoints).await;
}

#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn endpoint_unix() {
    zenoh_util::init_log_from_env_or("error");
    // Remove the files if they still exists
    let f1 = "zenoh-test-unix-socket-0.sock";
    let f2 = "zenoh-test-unix-socket-1.sock";
    let _ = std::fs::remove_file(f1);
    let _ = std::fs::remove_file(f2);
    // Define the locators
    let endpoints: Vec<EndPoint> = vec![
        format!("unixsock-stream/{f1}").parse().unwrap(),
        format!("unixsock-stream/{f2}").parse().unwrap(),
    ];
    run(&endpoints).await;
    let _ = std::fs::remove_file(f1);
    let _ = std::fs::remove_file(f2);
    let _ = std::fs::remove_file(format!("{f1}.lock"));
    let _ = std::fs::remove_file(format!("{f2}.lock"));
}

#[cfg(feature = "transport_ws")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn endpoint_ws() {
    zenoh_util::init_log_from_env_or("error");
    // Define the locators
    let endpoints: Vec<EndPoint> = vec![
        format!("ws/127.0.0.1:{}", 7020).parse().unwrap(),
        format!("ws/[::1]:{}", 7021).parse().unwrap(),
        format!("ws/localhost:{}", 7022).parse().unwrap(),
    ];
    run(&endpoints).await;
}

#[cfg(feature = "transport_unixpipe")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn endpoint_unixpipe() {
    zenoh_util::init_log_from_env_or("error");
    // Define the locators
    let endpoints: Vec<EndPoint> = vec![
        "unixpipe/endpoint_unixpipe".parse().unwrap(),
        "unixpipe/endpoint_unixpipe2".parse().unwrap(),
        "unixpipe/endpoint_unixpipe3".parse().unwrap(),
        "unixpipe/endpoint_unixpipe4".parse().unwrap(),
    ];
    run(&endpoints).await;
}

#[cfg(all(feature = "transport_tcp", feature = "transport_udp"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn endpoint_tcp_udp() {
    zenoh_util::init_log_from_env_or("error");
    // Define the locators
    let endpoints: Vec<EndPoint> = vec![
        format!("tcp/127.0.0.1:{}", 7030).parse().unwrap(),
        format!("udp/127.0.0.1:{}", 7031).parse().unwrap(),
        format!("tcp/[::1]:{}", 7032).parse().unwrap(),
        format!("udp/[::1]:{}", 7033).parse().unwrap(),
    ];
    run(&endpoints).await;
}

#[cfg(all(
    feature = "transport_tcp",
    feature = "transport_udp",
    feature = "transport_unixsock-stream",
    target_family = "unix"
))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn endpoint_tcp_udp_unix() {
    zenoh_util::init_log_from_env_or("error");
    // Remove the file if it still exists
    let f1 = "zenoh-test-unix-socket-2.sock";
    let _ = std::fs::remove_file(f1);
    // Define the locators
    let endpoints: Vec<EndPoint> = vec![
        format!("tcp/127.0.0.1:{}", 7040).parse().unwrap(),
        format!("udp/127.0.0.1:{}", 7041).parse().unwrap(),
        format!("tcp/[::1]:{}", 7042).parse().unwrap(),
        format!("udp/[::1]:{}", 7043).parse().unwrap(),
        format!("unixsock-stream/{f1}").parse().unwrap(),
    ];
    run(&endpoints).await;
    let _ = std::fs::remove_file(f1);
    let _ = std::fs::remove_file(format!("{f1}.lock"));
}

#[cfg(all(
    feature = "transport_tcp",
    feature = "transport_unixsock-stream",
    target_family = "unix"
))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn endpoint_tcp_unix() {
    zenoh_util::init_log_from_env_or("error");
    // Remove the file if it still exists
    let f1 = "zenoh-test-unix-socket-3.sock";
    let _ = std::fs::remove_file(f1);
    // Define the locators
    let endpoints: Vec<EndPoint> = vec![
        format!("tcp/127.0.0.1:{}", 7050).parse().unwrap(),
        format!("tcp/[::1]:{}", 7051).parse().unwrap(),
        format!("unixsock-stream/{f1}").parse().unwrap(),
    ];
    run(&endpoints).await;
    let _ = std::fs::remove_file(f1);
    let _ = std::fs::remove_file(format!("{f1}.lock"));
}

#[cfg(all(
    feature = "transport_udp",
    feature = "transport_unixsock-stream",
    target_family = "unix"
))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn endpoint_udp_unix() {
    zenoh_util::init_log_from_env_or("error");
    // Remove the file if it still exists
    let f1 = "zenoh-test-unix-socket-4.sock";
    let _ = std::fs::remove_file(f1); // Define the locators
    let endpoints: Vec<EndPoint> = vec![
        format!("udp/127.0.0.1:{}", 7060).parse().unwrap(),
        format!("udp/[::1]:{}", 7061).parse().unwrap(),
        format!("unixsock-stream/{f1}").parse().unwrap(),
    ];
    run(&endpoints).await;
    let _ = std::fs::remove_file(f1);
    let _ = std::fs::remove_file(format!("{f1}.lock"));
}

#[cfg(feature = "transport_tls")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn endpoint_tls() {
    use zenoh_link_commons::tls::config::*;

    zenoh_util::init_log_from_env_or("error");

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
    let mut endpoint: EndPoint = format!("tls/localhost:{}", 7070).parse().unwrap();
    endpoint
        .config_mut()
        .extend_from_iter(
            [
                (TLS_LISTEN_CERTIFICATE_RAW, cert),
                (TLS_LISTEN_PRIVATE_KEY_RAW, key),
            ]
            .iter()
            .copied(),
        )
        .unwrap();

    let endpoints = vec![endpoint];
    run(&endpoints).await;
}

#[cfg(feature = "transport_quic")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn endpoint_quic() {
    use zenoh_link_commons::tls::config::*;

    zenoh_util::init_log_from_env_or("error");

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
    let mut endpoint: EndPoint = format!("quic/localhost:{}", 7080).parse().unwrap();
    endpoint
        .config_mut()
        .extend_from_iter(
            [
                (TLS_LISTEN_CERTIFICATE_RAW, cert),
                (TLS_LISTEN_PRIVATE_KEY_RAW, key),
            ]
            .iter()
            .copied(),
        )
        .unwrap();
    let endpoints = vec![endpoint];
    run(&endpoints).await;
}

#[cfg(all(feature = "transport_vsock", target_os = "linux"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn endpoint_vsock() {
    zenoh_util::init_log_from_env_or("error");
    // Define the locators
    let endpoints: Vec<EndPoint> = vec![
        "vsock/-1:1234".parse().unwrap(),
        "vsock/VMADDR_CID_ANY:VMADDR_PORT_ANY".parse().unwrap(),
        "vsock/VMADDR_CID_LOCAL:2345".parse().unwrap(),
        "vsock/VMADDR_CID_LOCAL:VMADDR_PORT_ANY".parse().unwrap(),
    ];
    run(&endpoints).await;
}
