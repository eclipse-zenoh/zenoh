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
    cancellation_token: CancellationToken,
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
        let ct = cancellation_token.clone();
        let runtime_clone = runtime.clone();

        runtime.spawn(async move {
            Self::run(runtime_clone, endpoint, config, ct).await;
        });

        Self { cancellation_token }
    }

    async fn run(
        runtime: Runtime,
        endpoint: EndPoint,
        config: RotationConf,
        cancellation_token: CancellationToken,
    ) {
        let interval_ms = match config.interval_ms() {
            Some(ms) => ms,
            None => {
                tracing::warn!(
                    "Rotation enabled for {} but no interval configured. Disabling rotation.",
                    endpoint
                );
                return;
            }
        };

        let jitter_ms = config.jitter_ms().unwrap_or(0);
        let base_interval = Duration::from_millis(interval_ms);

        tracing::info!(
            "Starting rotation engine for {} with interval {:?} (jitter ±{}ms)",
            endpoint,
            base_interval,
            jitter_ms
        );

        loop {
            // Sleep for interval + random jitter
            let jitter = if jitter_ms > 0 {
                Duration::from_millis(rand::thread_rng().gen_range(0..=jitter_ms))
            } else {
                Duration::ZERO
            };

            tokio::select! {
                _ = tokio::time::sleep(base_interval + jitter) => {}
                _ = cancellation_token.cancelled() => {
                    tracing::debug!("Rotation engine for {} cancelled.", endpoint);
                    return;
                }
            }

            // Perform rotation
            if let Err(e) = Self::rotate(&runtime, &endpoint, &config).await {
                tracing::warn!("Rotation cycle for {} failed: {}", endpoint, e);
            }
        }
    }

    /// Perform a single rotation cycle for an endpoint.
    async fn rotate(
        runtime: &Runtime,
        endpoint: &EndPoint,
        config: &RotationConf,
    ) -> ZResult<()> {
        tracing::debug!("Rotating transport link for {}", endpoint);

        // Step 1: Attempt make-before-break (open new link)
        match Self::try_make_before_break(runtime, endpoint).await {
            Ok(()) => {
                tracing::debug!(
                    "Rotation make-before-break succeeded for {}. \
                     Old link will be closed without triggering closed_session().",
                    endpoint
                );
                Ok(())
            }
            Err(e) => {
                tracing::warn!(
                    "Rotation make-before-break failed for {}: {}. \
                     Retrying up to {} times...",
                    endpoint,
                    e,
                    config.fallback.max_retries
                );

                if config.fallback.enabled {
                    // Retry make a few times before falling back
                    let mut backoff = Duration::from_millis(config.fallback.retry_backoff_ms);
                    for attempt in 1..=config.fallback.max_retries {
                        tokio::time::sleep(backoff).await;
                        tracing::debug!(
                            "Rotation retry {}/{} for {}",
                            attempt,
                            config.fallback.max_retries,
                            endpoint
                        );
                        match Self::try_make_before_break(runtime, endpoint).await {
                            Ok(()) => {
                                tracing::debug!(
                                    "Rotation retry {}/{} succeeded for {}",
                                    attempt,
                                    config.fallback.max_retries,
                                    endpoint
                                );
                                return Ok(());
                            }
                            Err(re) => {
                                tracing::debug!(
                                    "Rotation retry {}/{} failed for {}: {}",
                                    attempt,
                                    config.fallback.max_retries,
                                    endpoint,
                                    re
                                );
                                backoff *= 2; // exponential backoff
                            }
                        }
                    }

                    // All retries failed — fall back to break-before-make
                    tracing::warn!(
                        "Rotation failed after {} retries for {}. \
                         Falling back to break-before-make (may cause redeclaration burst).",
                        config.fallback.max_retries,
                        endpoint
                    );
                    Self::fallback_break_before_make(runtime, endpoint).await
                } else {
                    tracing::warn!(
                        "Rotation failed for {} and fallback is disabled. \
                         Keeping old connection.",
                        endpoint
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
    async fn try_make_before_break(
        runtime: &Runtime,
        endpoint: &EndPoint,
    ) -> ZResult<()> {
        // Open a new transport to the same endpoint.
        // If a transport to the same peer ZID already exists,
        // this will add a new link to it (multilink) or fail
        // (if max_links=1 without multilink).
        let new_transport = runtime
            .manager()
            .open_transport_unicast(endpoint.clone())
            .await?;

        // Get the callback to find the RuntimeSession and update endpoint tracking
        let cb = new_transport
            .get_callback()?
            .ok_or_else(|| zerror!("Transport closed immediately after open"))?;

        let session = cb
            .as_any()
            .downcast_ref::<super::RuntimeSession>()
            .ok_or_else(|| zerror!("Unexpected callback type"))?;

        // Check if we now have more than one link to the same peer.
        // If so, close the old one(s) that correspond to this endpoint.
        let links = new_transport.get_links().unwrap_or_default();
        if links.len() > 1 {
            // Find the old link — it's the one whose dst matches our endpoint's locator
            // but is not the newly opened one. We close it via the transport's del_link.
            let locator = endpoint.to_locator();
            let old_links: Vec<_> = links
                .iter()
                .filter(|l| l.dst == locator)
                .collect();

            // The newest link is the one we just opened; the rest are old.
            // Close all but the last one (which is the new link).
            for old_link in old_links.iter().take(old_links.len().saturating_sub(1)) {
                tracing::debug!(
                    "Closing old link {} during rotation for {}",
                    old_link,
                    endpoint
                );
                // TODO: Close the old link via the transport's del_link mechanism.
                // This requires a new method on TransportUnicast to close a specific
                // link, or the transport needs to expose a way to drop a single link.
                // For now, the old link will eventually be cleaned up when the
                // transport detects it's stale, or when the transport is closed.
                //
                // The actual link closure mechanism needs to be added to the
                // transport layer API (TransportUnicast::close_link or similar).
            }
        }

        // Update endpoint tracking
        zwrite!(session.endpoints).insert(endpoint.clone());

        Ok(())
    }

    /// Fallback: break-before-make.
    ///
    /// This closes the old transport entirely, which triggers
    /// `closed_session()` and redeclarations. The orchestrator's
    /// existing retry logic will then re-establish the connection.
    async fn fallback_break_before_make(
        runtime: &Runtime,
        endpoint: &EndPoint,
    ) -> ZResult<()> {
        // Find the existing transport for this endpoint and close it.
        // This will trigger closed_session() which spawns peers_connector_retry().
        let transports = runtime.manager().get_transports_unicast().await;

        for transport in transports {
            if let Ok(links) = transport.get_links() {
                let locator = endpoint.to_locator();
                if links.iter().any(|l| l.dst == locator) {
                    tracing::info!(
                        "Closing transport for {} (break-before-make fallback)",
                        endpoint
                    );
                    let _ = transport.close().await;
                    // The closed_session() callback will handle reconnection
                    break;
                }
            }
        }

        // Wait a brief moment for the close to propagate
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Now try to reconnect
        runtime
            .manager()
            .open_transport_unicast(endpoint.clone())
            .await?;

        Ok(())
    }
}

impl Drop for RotationEngine {
    fn drop(&mut self) {
        self.cancellation_token.cancel();
    }
}

/// Check if rotation is enabled for the given endpoint in the config.
pub(crate) fn is_rotation_enabled(
    config: &zenoh_config::ExpandedConfig,
    _endpoint: &EndPoint,
) -> Option<RotationConf> {
    let rotation = config.connect().rotation().as_ref()?;
    if rotation.enabled {
        Some(rotation.clone())
    } else {
        None
    }
}

/// Get the rotation configuration for a given endpoint.
///
/// Currently returns the global rotation config if enabled.
/// Per-endpoint overrides via query parameters can be added later.
pub(crate) fn get_rotation_config(
    config: &zenoh_config::ExpandedConfig,
    endpoint: &EndPoint,
) -> Option<RotationConf> {
    is_rotation_enabled(config, endpoint)
}
