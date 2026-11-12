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
//! Full-duplex stress for [`LinkUnicastTls`](super::LinkUnicastTls).
//!
//! rustls does not allow a read and a write on the same connection at once.
//! Zenoh's transport does exactly that (rx task and tx task). This test pumps
//! both directions together so that race fails the process instead of shipping.
//!
//! Each write stays at 8 KiB. `write_all` does not flush, and tokio-rustls may
//! keep ciphertext in its buffer once a write fills the socket. A modest size
//! still overlaps read and write, which is what trips the race.
//!
//! Intensity can be raised without a rebuild:
//! `TLS_RACE_PAIRS`, `TLS_RACE_ROUNDS`, `TLS_RACE_BYTES`, `TLS_RACE_TIMEOUT_SECS`.

use std::{
    env,
    io::Cursor,
    sync::{Arc, OnceLock},
    time::Duration,
};

use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider},
    pki_types::{CertificateDer, ServerName, UnixTime},
    DigitallySignedStruct, Error, SignatureScheme,
};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, TlsConnector, TlsStream};
use zenoh_link_commons::{LinkAuthId, LinkUnicastTrait};

use super::{abort_socket, LinkUnicastTls};

fn env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

#[derive(Debug)]
struct SkipServerVerification(Arc<CryptoProvider>);

impl SkipServerVerification {
    fn new(provider: Arc<CryptoProvider>) -> Arc<Self> {
        Arc::new(Self(provider))
    }
}

impl ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

struct TlsEnds {
    server: Arc<rustls::ServerConfig>,
    client: Arc<rustls::ClientConfig>,
}

const CERT_PEM: &[u8] = b"-----BEGIN CERTIFICATE-----
MIIBfTCCASOgAwIBAgIUFCCgjj1A4yw15OS5zdvdwZUBPJwwCgYIKoZIzj0EAwIw
FDESMBAGA1UEAwwJbG9jYWxob3N0MB4XDTI2MTEwMzE1NTIzNloXDTM2MTAzMTE1
NTIzNlowFDESMBAGA1UEAwwJbG9jYWxob3N0MFkwEwYHKoZIzj0CAQYIKoZIzj0D
AQcDQgAEdJOZ4027hsu5kWVNKJ3fKS8ZuofJYK+cvev3yqbd8SGBhJyoMm1fDxDW
CzVNh/vD41lcC0W0kt3rBbVpJlFkkaNTMFEwHQYDVR0OBBYEFOrHLnePcUXlql8y
P6EGWjM+JmiXMB8GA1UdIwQYMBaAFOrHLnePcUXlql8yP6EGWjM+JmiXMA8GA1Ud
EwEB/wQFMAMBAf8wCgYIKoZIzj0EAwIDSAAwRQIhAK5yKysrvlIWdvHn+kc1JG9h
r0SR+jtt3GO21NLy4O2jAiBYZckd6xYSUc18SbOMt5uB8JZImQCsvpzh5280X/Ue
3w==
-----END CERTIFICATE-----
";

const KEY_PEM: &[u8] = b"-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQg3whi2DEKrL4t/CAy
U09twYqqZeaF6Do+WPEfR6u5ZhChRANCAAR0k5njTbuGy7mRZU0ond8pLxm6h8lg
r5y96/fKpt3xIYGEnKgybV8PENYLNU2H+8PjWVwLRbSS3esFtWkmUWSR
-----END PRIVATE KEY-----
";

fn tls_ends() -> &'static TlsEnds {
    static ENDS: OnceLock<TlsEnds> = OnceLock::new();
    ENDS.get_or_init(|| {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let cert_der = rustls_pemfile::certs(&mut Cursor::new(CERT_PEM))
            .next()
            .expect("cert pem")
            .expect("cert");
        let key_der = rustls_pemfile::private_key(&mut Cursor::new(KEY_PEM))
            .expect("key pem")
            .expect("key");
        let server = rustls::ServerConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .expect("tls versions")
            .with_no_client_auth()
            .with_single_cert(vec![cert_der], key_der)
            .expect("server config");
        let client = rustls::ClientConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .expect("tls versions")
            .dangerous()
            .with_custom_certificate_verifier(SkipServerVerification::new(provider))
            .with_no_client_auth();
        TlsEnds {
            server: Arc::new(server),
            client: Arc::new(client),
        }
    })
}

fn link_from_stream(stream: TlsStream<TcpStream>) -> LinkUnicastTls {
    let src_addr = stream.get_ref().0.local_addr().expect("local addr");
    let dst_addr = stream.get_ref().0.peer_addr().expect("peer addr");
    let abort_socket = abort_socket(&stream).expect("abort socket");
    LinkUnicastTls::new(
        stream,
        abort_socket,
        src_addr,
        dst_addr,
        LinkAuthId::Tls(None),
        None,
    )
}

async fn exchange(link: Arc<LinkUnicastTls>, payload: Arc<[u8]>, rounds: usize) {
    let writer_link = Arc::clone(&link);
    let writer_payload = Arc::clone(&payload);
    let writer = tokio::spawn(async move {
        for _ in 0..rounds {
            writer_link
                .write_all(&writer_payload, None)
                .await
                .expect("tls write");
        }
    });
    let reader = tokio::spawn(async move {
        let mut buf = vec![0u8; payload.len()];
        for _ in 0..rounds {
            link.read_exact(&mut buf, None).await.expect("tls read");
            assert_eq!(&buf[..], &payload[..]);
        }
    });
    writer.await.expect("writer task");
    reader.await.expect("reader task");
}

async fn one_pair(payload: Arc<[u8]>, rounds: usize) {
    let ends = tls_ends();
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("listener addr");
    let acceptor = TlsAcceptor::from(Arc::clone(&ends.server));
    let connector = TlsConnector::from(Arc::clone(&ends.client));

    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.expect("accept");
        tcp.set_nodelay(true).expect("nodelay");
        let tls = acceptor.accept(tcp).await.expect("server handshake");
        link_from_stream(TlsStream::Server(tls))
    });

    let tcp = TcpStream::connect(addr).await.expect("connect");
    tcp.set_nodelay(true).expect("nodelay");
    let server_name = ServerName::try_from("localhost").expect("server name");
    let tls = connector
        .connect(server_name, tcp)
        .await
        .expect("client handshake");
    let client = Arc::new(link_from_stream(TlsStream::Client(tls)));
    let server = Arc::new(server.await.expect("server task"));

    // Keep both halves alive until each side has read everything the other wrote.
    // Dropping a link closes the TCP socket without close_notify.
    tokio::join!(
        exchange(Arc::clone(&client), Arc::clone(&payload), rounds),
        exchange(Arc::clone(&server), payload, rounds),
    );

    LinkUnicastTrait::close(client.as_ref())
        .await
        .expect("close client");
    LinkUnicastTrait::close(server.as_ref())
        .await
        .expect("close server");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn tls_full_duplex_does_not_race() {
    let pairs = env_usize("TLS_RACE_PAIRS", 8);
    let rounds = env_usize("TLS_RACE_ROUNDS", 40);
    let bytes = env_usize("TLS_RACE_BYTES", 8 * 1024);
    let payload: Arc<[u8]> = (0..bytes).map(|index| (index % 251) as u8).collect();

    let mut joins = Vec::with_capacity(pairs);
    for _ in 0..pairs {
        let payload = Arc::clone(&payload);
        joins.push(tokio::spawn(async move {
            let timeout_secs = env_usize("TLS_RACE_TIMEOUT_SECS", 15);
            tokio::time::timeout(
                Duration::from_secs(timeout_secs as u64),
                one_pair(payload, rounds),
            )
            .await
            .expect("full duplex timed out");
        }));
    }
    for join in joins {
        join.await.expect("pair");
    }
}
