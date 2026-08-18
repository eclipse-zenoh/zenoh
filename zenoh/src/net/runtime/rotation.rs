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

//! Transport link rotation engine.
//!
//! When enabled, periodically closes and re-establishes transport links
//! to configured endpoints using a make-before-break strategy: the new
//! link is opened before the old one is closed, avoiding connectivity
//! gaps and redeclaration storms.

use std::time::Duration;

use rand::Rng;
use tokio_util::sync::CancellationToken;
use zenoh_config::RotationConf;
use zenoh_protocol::core::EndPoint;
use zenoh_result::{zerror, ZResult};

use super::Runtime;

/// A handle to a running rotation engine task.
pub(crate) struct RotationEngine {
    _cancellation_token: CancellationToken,
}

impl RotationEngine {
    /// Start a rotation engine for a single endpoint.
    ///
    /// The engine will periodically:
    /// 1. Open a new transport link to the endpoint (make)
    /// 2. Close the old link (break)
    /// 3. Update endpoint tracking
    ///
    /// If make fails, it retries up to `fallback.max_retries` times.
    /// If all retries fail and fallback is enabled, it falls back to
    /// break-before-make (which causes a redeclaration burst).
    pub(crate) fn start(
        runtime: Runtime,
        endpoint: EndPoint,
        config: RotationConf,
    ) -> Self {
        let cancellation_token = runtime.get_cancellation_token();

        runtime.spawn({
            let runtime = runtime.clone();
            let ct = cancellation_token.clone();
            async move {
                Self::run(runtime, endpoint, config, ct).await;
            }
        });

        Self {
            _cancellation_token: cancellation_token,
        }
    }

    async fn run(
        runtime: Runtime,
        endpoint: EndPoint,
        config: RotationConf,
        cancellation_token: CancellationToken,
    ) {
        let Some(interval_ms) = config.interval_ms() else {
            tracing::warn!(
                "Rotation enabled for {endpoint} but no interval configured. Disabling rotation."
            );
            return;
        };

        let jitter_ms = config.jitter_ms().unwrap_or(0);
        let base_interval = Duration::from_millis(interval_ms);

        tracing::info!(
            "Starting rotation engine for {endpoint} with interval {base_interval:?} (jitter ±{jitter_ms}ms)"
        );

        loop {
            let jitter = (jitter_ms > 0)
                .then(|| Duration::from_millis(rand::thread_rng().gen_range(0..=jitter_ms)))
                .unwrap_or(Duration::ZERO);

            tokio::select! {
                _ = tokio::time::sleep(base_interval + jitter) => {}
                _ = cancellation_token.cancelled() => {
                    tracing::debug!("Rotation engine for {endpoint} cancelled.");
                    return;
                }
            }

            if let Err(e) = Self::rotate(&runtime, &endpoint, &config).await {
                tracing::warn!("Rotation cycle for {endpoint} failed: {e}");
            }
        }
    }

    /// Perform a single rotation cycle for an endpoint.
    async fn rotate(
        runtime: &Runtime,
        endpoint: &EndPoint,
        config: &RotationConf,
    ) -> ZResult<()> {
        tracing::debug!("Rotating transport link for {endpoint}");

        match Self::try_make_before_break(runtime, endpoint).await {
            Ok(()) => {
                tracing::debug!(
                    "Rotation make-before-break succeeded for {endpoint}. \
                     Old link will be closed without triggering closed_session()."
                );
                Ok(())
            }
            Err(e) => {
                tracing::warn!(
                    "Rotation make-before-break failed for {endpoint}: {e}. \
                     Retrying up to {} times...",
                    config.fallback.max_retries
                );

                if config.fallback.enabled {
                    let mut backoff = Duration::from_millis(config.fallback.retry_backoff_ms);
                    for attempt in 1..=config.fallback.max_retries {
                        tokio::time::sleep(backoff).await;
                        tracing::debug!(
                            "Rotation retry {}/{} for {endpoint}",
                            attempt,
                            config.fallback.max_retries
                        );
                        match Self::try_make_before_break(runtime, endpoint).await {
                            Ok(()) => {
                                tracing::debug!(
                                    "Rotation retry {}/{} succeeded for {endpoint}",
                                    attempt,
                                    config.fallback.max_retries
                                );
                                return Ok(());
                            }
                            Err(re) => {
                                tracing::debug!(
                                    "Rotation retry {}/{} failed for {endpoint}: {re}",
                                    attempt,
                                    config.fallback.max_retries
                                );
                                backoff *= 2;
                            }
                        }
                    }

                    tracing::warn!(
                        "Rotation failed after {} retries for {endpoint}. \
                         Falling back to break-before-make (may cause redeclaration burst).",
                        config.fallback.max_retries
                    );
                    Self::fallback_break_before_make(runtime, endpoint).await
                } else {
                    tracing::warn!(
                        "Rotation failed for {endpoint} and fallback is disabled. \
                         Keeping old connection."
                    );
                    Ok(())
                }
            }
        }
    }

    /// Attempt make-before-break: open a new link to the endpoint.
    ///
    /// If successful, the old link is closed via `del_link` on the
    /// transport, which does NOT trigger `closed_session()` because
    /// the transport still has the new link.
    async fn try_make_before_break(runtime: &Runtime, endpoint: &EndPoint) -> ZResult<()> {
        // Open a new transport to the same endpoint.
        // If a transport to the same peer ZID already exists,
        // this will add a new link to it (multilink) or fail
        // (if max_links=1 without multilink).
        let new_transport = runtime
            .manager()
            .open_transport_unicast(endpoint.clone())
            .await?;

        let cb = new_transport
            .get_callback()?
            .ok_or_else(|| zerror!("Transport closed immediately after open"))?;

        let session = cb
            .as_any()
            .downcast_ref::<super::RuntimeSession>()
            .ok_or_else(|| zerror!("Unexpected callback type"))?;

        // If we now have more than one link to the same peer, close the old one(s).
        let links = new_transport.get_links().unwrap_or_default();
        if links.len() > 1 {
            let locator = endpoint.to_locator();
            let old_links: Vec<_> = links.into_iter().filter(|l| l.dst == locator).collect();
            for old_link in old_links.iter().take(old_links.len().saturating_sub(1)) {
                tracing::debug!("Closing old link {old_link} during rotation for {endpoint}");
                new_transport.close_link(old_link.clone()).await?;
            }
        }

        zwrite!(session.endpoints).insert(endpoint.clone());
        Ok(())
    }

    /// Fallback: break-before-make.
    ///
    /// Closes the old transport entirely, which triggers `closed_session()`
    /// and redeclarations. The orchestrator's existing retry logic will
    /// then re-establish the connection.
    async fn fallback_break_before_make(runtime: &Runtime, endpoint: &EndPoint) -> ZResult<()> {
        let transports = runtime.manager().get_transports_unicast().await;
        let locator = endpoint.to_locator();

        for transport in transports {
            if let Ok(links) = transport.get_links() {
                if links.iter().any(|l| l.dst == locator) {
                    tracing::info!("Closing transport for {endpoint} (break-before-make fallback)");
                    let _ = transport.close().await;
                    break;
                }
            }
        }

        tokio::time::sleep(Duration::from_millis(100)).await;

        runtime
            .manager()
            .open_transport_unicast(endpoint.clone())
            .await?;

        Ok(())
    }
}

/// Get the rotation configuration for a given endpoint.
///
/// Currently returns the global rotation config if enabled.
/// Per-endpoint overrides via query parameters can be added later.
pub(crate) fn get_rotation_config(
    config: &zenoh_config::ExpandedConfig,
    _endpoint: &EndPoint,
) -> Option<RotationConf> {
    let rotation = config.connect().rotation().as_ref()?;
    rotation.enabled.then(|| rotation.clone())
}
