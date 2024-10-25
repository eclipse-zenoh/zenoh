use crate::LinkUnicastTrait;
use alloc::vec::Vec;
use rustls::{
    client::{
        danger::{ServerCertVerified, ServerCertVerifier},
        verify_server_cert_signed_by_trust_anchor,
    },
    crypto::{verify_tls12_signature, verify_tls13_signature},
    pki_types::{CertificateDer, ServerName, UnixTime},
    server::ParsedCertificate,
    RootCertStore,
};
use std::{collections::BTreeMap, sync::Weak};
use time::OffsetDateTime;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use webpki::ALL_VERIFICATION_ALGS;
use zenoh_protocol::core::Locator;

impl ServerCertVerifier for WebPkiVerifierAnyServerName {
    /// Will verify the certificate is valid in the following ways:
    /// - Signed by a  trusted `RootCertStore` CA
    /// - Not Expired
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let cert = ParsedCertificate::try_from(end_entity)?;
        verify_server_cert_signed_by_trust_anchor(
            &cert,
            &self.roots,
            intermediates,
            now,
            ALL_VERIFICATION_ALGS,
        )?;
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// `ServerCertVerifier` that verifies that the server is signed by a trusted root, but allows any serverName
/// see the trait impl for more information.
#[derive(Debug)]
pub struct WebPkiVerifierAnyServerName {
    roots: RootCertStore,
}

#[allow(unreachable_pub)]
impl WebPkiVerifierAnyServerName {
    /// Constructs a new `WebPkiVerifierAnyServerName`.
    ///
    /// `roots` is the set of trust anchors to trust for issuing server certs.
    pub fn new(roots: RootCertStore) -> Self {
        Self { roots }
    }
}

pub struct LinkCertExpirationManager {
    pub token: CancellationToken,
    pub tx: NewLinkExpirationSender,
    handle: Option<JoinHandle<()>>,
}

impl Drop for LinkCertExpirationManager {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            zenoh_runtime::ZRuntime::Acceptor.block_in_place(async {
                self.token.cancel();
                let _ = handle.await;
            })
        }
    }
}

impl LinkCertExpirationManager {
    pub fn new(token: CancellationToken) -> Self {
        let (tx, rx) = flume::bounded::<(OffsetDateTime, LinkCertExpirationInfo)>(1);
        let handle = zenoh_runtime::ZRuntime::Acceptor.spawn(expiration_task(token.clone(), rx));
        Self {
            token,
            handle: Some(handle),
            tx,
        }
    }
}

async fn expiration_task(
    token: CancellationToken,
    rx: flume::Receiver<(OffsetDateTime, LinkCertExpirationInfo)>,
) {
    // TODO: Maybe expose this as a locator parameter?
    const PERIODIC_SLEEP_DURATION: tokio::time::Duration = tokio::time::Duration::from_secs(600);
    let mut link_expiration_map: BTreeMap<OffsetDateTime, Vec<LinkCertExpirationInfo>> =
        BTreeMap::new();
    let mut next_wakeup_instant: tokio::time::Instant;

    loop {
        next_wakeup_instant = tokio::time::Instant::now() + PERIODIC_SLEEP_DURATION;

        if let Some((&next_expiration_time, links_to_close)) = link_expiration_map.first_key_value()
        {
            let now = OffsetDateTime::now_utc();
            if next_expiration_time <= now {
                // Close links
                // NOTE: after closing links, transports are reopened with cert chains that expire instantly.
                //       It's not an issue, but maybe we should add some throttling before closing the links...
                for link_info in links_to_close {
                    if let Some(link) = link_info.link.upgrade() {
                        tracing::warn!(
                            "Closing link {} => {} : remote certificate chain expired",
                            link_info.src_locator,
                            link_info.dst_locator,
                        );
                        if let Err(e) = link.close().await {
                            tracing::error!(
                                "Error closing link {} => {} : {}",
                                link_info.src_locator,
                                link_info.dst_locator,
                                e
                            )
                        }
                    }
                }
                link_expiration_map.remove(&next_expiration_time);
                // loop again to check the next deadline
                continue;
            } else {
                // next sleep duration is the minimum between PERIODIC_SLEEP_DURATION and the duration till next expiration
                // this mitigates the unsoundness of using `tokio::time::sleep_until` with long durations
                let next_expiration_duration = std::time::Duration::from_secs_f32(
                    (next_expiration_time - now).as_seconds_f32(),
                );
                next_wakeup_instant = tokio::time::Instant::now()
                    + tokio::time::Duration::min(PERIODIC_SLEEP_DURATION, next_expiration_duration);
            }
        }

        tokio::select! {
            _ = token.cancelled() => break,

            _ = tokio::time::sleep_until(next_wakeup_instant) => {},

            Ok(new_link) = rx.recv_async() => {
                link_expiration_map.entry(new_link.0).or_default().push(new_link.1);
            },
            // if channel was closed, do nothing: we don't expect more links to be created,
            // but it's not a reason to stop this task
        }
    }
}

pub struct LinkCertExpirationInfo {
    link: Weak<dyn LinkUnicastTrait>,
    src_locator: Locator,
    dst_locator: Locator,
}

impl LinkCertExpirationInfo {
    pub fn new(
        link: Weak<dyn LinkUnicastTrait>,
        src_locator: &Locator,
        dst_locator: &Locator,
    ) -> Self {
        Self {
            link,
            src_locator: src_locator.clone(),
            dst_locator: dst_locator.clone(),
        }
    }
}

pub type NewLinkExpirationSender = flume::Sender<(OffsetDateTime, LinkCertExpirationInfo)>;
