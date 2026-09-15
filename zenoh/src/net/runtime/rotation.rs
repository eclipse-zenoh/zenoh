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
    /// If make fails (e.g. because `max_links` is reached), the rotation
    /// is skipped and the old connection is kept untouched. If fallback
    /// is enabled, the old transport is closed and the orchestrator's
    /// existing `closed_session()` reconnect logic handles reconnection.
    pub(crate) fn start(runtime: Runtime, endpoint: EndPoint, config: RotationConf) -> Self {
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
            // Jitter is ±jitter_ms: subtract a random offset from the base interval
            // (clamped at a minimum of 1ms to avoid zero-duration sleeps).
            let jitter = if jitter_ms > 0 {
                let j = rand::thread_rng().gen_range(0..=jitter_ms);
                base_interval
                    .checked_sub(Duration::from_millis(j))
                    .unwrap_or(Duration::from_millis(1))
            } else {
                base_interval
            };

            tokio::select! {
                _ = tokio::time::sleep(jitter) => {}
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
    async fn rotate(runtime: &Runtime, endpoint: &EndPoint, config: &RotationConf) -> ZResult<()> {
        tracing::debug!("Rotating transport link for {endpoint}");

        match Self::try_make_before_break(runtime, endpoint).await {
            Ok(()) => {
                tracing::debug!(
                    "Rotation make-before-break succeeded for {endpoint}. \
                     Old link closed without triggering closed_session()."
                );
                Ok(())
            }
            Err(e) => {
                // Make-before-break failed. This typically happens when
                // max_links=1 (no multilink) and the connection lands on
                // the same router. In this case, skip the rotation and
                // keep the old connection — do not retry, since retrying
                // would likely hit the same max_links limit.
                if config.fallback.enabled {
                    tracing::warn!(
                        "Rotation make-before-break failed for {endpoint}: {e}. \
                         Falling back to break-before-make (orchestrator will reconnect)."
                    );
                    Self::fallback_break_before_make(runtime, endpoint).await
                } else {
                    tracing::warn!(
                        "Rotation failed for {endpoint}: {e}. \
                         Keeping old connection (fallback disabled)."
                    );
                    Ok(())
                }
            }
        }
    }

    /// Attempt make-before-break: open a new link to the endpoint.
    ///
    /// If successful, the old link is closed via `close_link` on the
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
        // Compare locators without metadata, since Link.dst may have patched
        // metadata (reliability/priorities) that endpoint.to_locator() does not.
        let links = new_transport.get_links().unwrap_or_default();
        if links.len() > 1 {
            let target_proto = endpoint.protocol().to_string();
            let target_addr = endpoint.address().to_string();
            let old_links: Vec<_> = links
                .into_iter()
                .filter(|l| {
                    l.dst.protocol().as_str() == target_proto
                        && l.dst.address().as_str() == target_addr
                })
                .collect();
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
    /// Closes the old transport entirely. This triggers `closed_session()`
    /// which causes the orchestrator's existing retry logic to re-establish
    /// the connection. We do NOT call `open_transport_unicast` ourselves
    /// to avoid racing with the orchestrator's reconnect logic.
    async fn fallback_break_before_make(runtime: &Runtime, endpoint: &EndPoint) -> ZResult<()> {
        let transports = runtime.manager().get_transports_unicast().await;
        let target_proto = endpoint.protocol().to_string();
        let target_addr = endpoint.address().to_string();

        for transport in transports {
            if let Ok(links) = transport.get_links() {
                // Match without metadata (same as try_make_before_break)
                if links.iter().any(|l| {
                    l.dst.protocol().as_str() == target_proto
                        && l.dst.address().as_str() == target_addr
                }) {
                    tracing::info!("Closing transport for {endpoint} (break-before-make fallback)");
                    let _ = transport.close().await;
                    // The orchestrator's closed_session() callback will handle
                    // reconnection via peers_connector_retry(). Do not attempt
                    // to reconnect here to avoid duplicate connection races.
                    return Ok(());
                }
            }
        }

        tracing::warn!(
            "No existing transport found for {endpoint} during break-before-make fallback"
        );
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use zenoh_config::Config;

    use super::*;

    fn expanded_config_with_rotation(rotation_json5: Option<&str>) -> zenoh_config::ExpandedConfig {
        let mut config = Config::default();
        if let Some(json5) = rotation_json5 {
            config.insert_json5("connect/rotation", json5).unwrap();
        }
        config.expanded()
    }

    fn probe_endpoint() -> EndPoint {
        EndPoint::from_str("tcp/127.0.0.1:7447").unwrap()
    }

    #[test]
    fn returns_none_when_rotation_is_absent() {
        let config = expanded_config_with_rotation(None);
        assert!(get_rotation_config(&config, &probe_endpoint()).is_none());
    }

    #[test]
    fn returns_none_when_rotation_is_present_but_disabled() {
        let config = expanded_config_with_rotation(Some(
            r#"{
                enabled: false,
                policy: { type: "interval", interval_ms: 1000 },
            }"#,
        ));
        assert!(get_rotation_config(&config, &probe_endpoint()).is_none());
    }

    #[test]
    fn returns_some_when_rotation_is_enabled() {
        let config = expanded_config_with_rotation(Some(
            r#"{
                enabled: true,
                policy: { type: "interval", interval_ms: 1000, jitter_ms: 100 },
            }"#,
        ));
        let rotation_conf = get_rotation_config(&config, &probe_endpoint())
            .expect("rotation config must be Some when enabled");
        assert!(rotation_conf.enabled);
        assert_eq!(rotation_conf.interval_ms(), Some(1000));
        assert_eq!(rotation_conf.jitter_ms(), Some(100));
    }

    #[test]
    fn ignores_the_endpoint_argument_since_rotation_is_global() {
        // `get_rotation_config` is documented as global (not per-endpoint):
        // any two distinct endpoints must yield the same result for a given config.
        let config = expanded_config_with_rotation(Some(
            r#"{
                enabled: true,
                policy: { type: "interval", interval_ms: 1000 },
            }"#,
        ));
        let a = EndPoint::from_str("tcp/127.0.0.1:7447").unwrap();
        let b = EndPoint::from_str("tcp/192.168.1.1:7000").unwrap();
        assert_eq!(
            get_rotation_config(&config, &a),
            get_rotation_config(&config, &b)
        );
    }

    #[test]
    fn enabled_without_a_policy_yields_a_config_with_no_interval() {
        // The engine itself disables the loop when there is no interval configured
        // (see `RotationEngine::run`), but `get_rotation_config` should still
        // surface the config as enabled so the caller can decide.
        let config = expanded_config_with_rotation(Some(r#"{ enabled: true }"#));
        let rotation_conf = get_rotation_config(&config, &probe_endpoint()).unwrap();
        assert!(rotation_conf.enabled);
        assert_eq!(rotation_conf.interval_ms(), None);
    }
}
