//
// Copyright (c) 2026 ZettaScale Technology
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
use std::{sync::Arc, u64};

use bytes::BytesMut;
use quinn_proto::{
    crypto::{
        self,
        rustls::{QuicClientConfig, QuicServerConfig},
        CryptoError,
    },
    transport_parameters, ConnectionId, Side, TransportError,
};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};

struct PlainTextSession(Box<dyn crypto::Session>);

impl PlainTextSession {
    fn wrap_packet_keys(
        keys: crypto::KeyPair<Box<dyn crypto::PacketKey>>,
    ) -> crypto::KeyPair<Box<dyn crypto::PacketKey>> {
        crypto::KeyPair {
            local: Box::new(NoOpEncryptionKeys(keys.local)),
            remote: Box::new(NoOpEncryptionKeys(keys.remote)),
        }
    }

    fn wrap_header_keys(
        keys: crypto::KeyPair<Box<dyn crypto::HeaderKey>>,
    ) -> crypto::KeyPair<Box<dyn crypto::HeaderKey>> {
        crypto::KeyPair {
            local: Box::new(NoOpEncryptionKeys(keys.local)),
            remote: Box::new(NoOpEncryptionKeys(keys.remote)),
        }
    }
}

/// No-op wrapper for encryption keys.
/// Disables encryption and decryption of payloads
struct NoOpEncryptionKeys<T>(T);

impl crypto::PacketKey for NoOpEncryptionKeys<Box<dyn crypto::PacketKey>> {
    fn encrypt(&self, _packet: u64, _buf: &mut [u8], _header_len: usize) {}

    fn decrypt(
        &self,
        _packet: u64,
        _header: &[u8],
        _payload: &mut BytesMut,
    ) -> Result<(), CryptoError> {
        Ok(())
    }

    fn tag_len(&self) -> usize {
        // NO AEAD tag
        0
    }

    fn confidentiality_limit(&self) -> u64 {
        // Set to max to limit unnecessary key updates
        u64::MAX
    }

    fn integrity_limit(&self) -> u64 {
        // TODO: maybe this can be set to 1 since we don't expect decryption failures?
        self.0.integrity_limit()
    }
}

impl crypto::HeaderKey for NoOpEncryptionKeys<Box<dyn crypto::HeaderKey>> {
    // No ECB decryption
    fn decrypt(&self, _pn_offset: usize, _packet: &mut [u8]) {}

    // No ECB encryption
    fn encrypt(&self, _pn_offset: usize, _packet: &mut [u8]) {}

    fn sample_size(&self) -> usize {
        // No header encryption: set sample_size to 0 to avoid unnecessary padding of payloads
        0
    }
}

pub(crate) struct PlainTextClientConfig {
    inner: Arc<QuicClientConfig>,
}

impl PlainTextClientConfig {
    pub(crate) fn new(config: Arc<QuicClientConfig>) -> Self {
        Self { inner: config }
    }
}

pub(crate) struct PlainTextServerConfig {
    inner: Arc<QuicServerConfig>,
}

impl PlainTextServerConfig {
    pub(crate) fn new(config: Arc<QuicServerConfig>) -> Self {
        Self { inner: config }
    }
}

impl crypto::Session for PlainTextSession {
    fn initial_keys(&self, dst_cid: &ConnectionId, side: Side) -> crypto::Keys {
        let mut keys = self.0.initial_keys(dst_cid, side);
        keys.header = Self::wrap_header_keys(keys.header);
        keys.packet = Self::wrap_packet_keys(keys.packet);
        keys
    }

    fn handshake_data(&self) -> Option<Box<dyn std::any::Any>> {
        self.0.handshake_data()
    }

    fn peer_identity(&self) -> Option<Box<dyn std::any::Any>> {
        self.0.peer_identity()
    }

    fn early_crypto(&self) -> Option<(Box<dyn crypto::HeaderKey>, Box<dyn crypto::PacketKey>)> {
        let (hkey, pkey) = self.0.early_crypto()?;
        Some((
            Box::new(NoOpEncryptionKeys(hkey)),
            Box::new(NoOpEncryptionKeys(pkey)),
        ))
    }

    fn early_data_accepted(&self) -> Option<bool> {
        self.0.early_data_accepted()
    }

    fn is_handshaking(&self) -> bool {
        self.0.is_handshaking()
    }

    fn read_handshake(&mut self, buf: &[u8]) -> Result<bool, TransportError> {
        self.0.read_handshake(buf)
    }

    fn transport_parameters(
        &self,
    ) -> Result<Option<transport_parameters::TransportParameters>, TransportError> {
        self.0.transport_parameters()
    }

    fn write_handshake(&mut self, buf: &mut Vec<u8>) -> Option<crypto::Keys> {
        let keys = self.0.write_handshake(buf)?;
        Some(crypto::Keys {
            header: Self::wrap_header_keys(keys.header),
            packet: Self::wrap_packet_keys(keys.packet),
        })
    }

    fn next_1rtt_keys(&mut self) -> Option<crypto::KeyPair<Box<dyn crypto::PacketKey>>> {
        let keys = self.0.next_1rtt_keys()?;
        Some(Self::wrap_packet_keys(keys))
    }

    fn is_valid_retry(&self, orig_dst_cid: &ConnectionId, header: &[u8], payload: &[u8]) -> bool {
        self.0.is_valid_retry(orig_dst_cid, header, payload)
    }

    fn export_keying_material(
        &self,
        output: &mut [u8],
        label: &[u8],
        context: &[u8],
    ) -> Result<(), crypto::ExportKeyingMaterialError> {
        self.0.export_keying_material(output, label, context)
    }
}

impl crypto::ClientConfig for PlainTextClientConfig {
    fn start_session(
        self: std::sync::Arc<Self>,
        version: u32,
        server_name: &str,
        params: &transport_parameters::TransportParameters,
    ) -> Result<Box<dyn crypto::Session>, quinn::ConnectError> {
        let tls = self
            .inner
            .clone()
            .start_session(version, server_name, params)?;

        Ok(Box::new(PlainTextSession(tls)))
    }
}

impl crypto::ServerConfig for PlainTextServerConfig {
    fn initial_keys(
        &self,
        version: u32,
        dst_cid: &ConnectionId,
    ) -> Result<crypto::Keys, crypto::UnsupportedVersion> {
        let mut keys = self.inner.initial_keys(version, dst_cid)?;
        keys.header = PlainTextSession::wrap_header_keys(keys.header);
        keys.packet = PlainTextSession::wrap_packet_keys(keys.packet);
        Ok(keys)
    }

    fn retry_tag(&self, version: u32, orig_dst_cid: &ConnectionId, packet: &[u8]) -> [u8; 16] {
        self.inner.retry_tag(version, orig_dst_cid, packet)
    }

    fn start_session(
        self: Arc<Self>,
        version: u32,
        params: &transport_parameters::TransportParameters,
    ) -> Box<dyn crypto::Session> {
        Box::new(PlainTextSession(
            self.inner.clone().start_session(version, params),
        ))
    }
}

#[derive(Debug)]
pub(crate) struct SkipServerVerification(Arc<rustls::crypto::CryptoProvider>);

impl SkipServerVerification {
    pub(crate) fn new(provider: Arc<rustls::crypto::CryptoProvider>) -> Arc<Self> {
        Arc::new(Self(provider))
    }
}

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
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
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}
