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

use serde::{Deserialize, Serialize};

/// Rotation policy configuration.
///
/// Only the `Interval` policy is implemented initially.
/// Additional variants (e.g. byte threshold, DNS change) can be added
/// to this enum when they are implemented.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum RotationPolicyConf {
    /// Rotate at fixed time intervals.
    #[serde(rename = "interval")]
    Interval {
        /// Rotation interval in milliseconds.
        interval_ms: u64,
        /// Random jitter in milliseconds (±) to avoid synchronized rotation across clients.
        /// Defaults to 0 (no jitter) if not specified.
        #[serde(default)]
        jitter_ms: Option<u64>,
    },
}

/// Rotation mode.
///
/// `MakeBeforeBreak` is the only user-selectable mode. `BreakBeforeMake`
/// is not a user-selectable mode — it is only used internally as a
/// fallback when `MakeBeforeBreak` fails.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub enum RotationModeConf {
    #[serde(rename = "make_before_break")]
    MakeBeforeBreak,
}

/// Fallback configuration for when make-before-break fails.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct RotationFallbackConf {
    /// Enable break-before-make as a fallback when make-before-break fails.
    /// If disabled and make-before-break fails, the rotation is simply
    /// skipped and the old connection remains untouched.
    #[serde(default = "default_fallback_enabled")]
    pub enabled: bool,
    /// Number of retries before falling back to break-before-make.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Initial backoff between retries in milliseconds.
    #[serde(default = "default_retry_backoff_ms")]
    pub retry_backoff_ms: u64,
}

fn default_fallback_enabled() -> bool {
    true
}

fn default_max_retries() -> u32 {
    3
}

fn default_retry_backoff_ms() -> u64 {
    1000
}

impl Default for RotationFallbackConf {
    fn default() -> Self {
        Self {
            enabled: default_fallback_enabled(),
            max_retries: default_max_retries(),
            retry_backoff_ms: default_retry_backoff_ms(),
        }
    }
}

/// Rotation configuration for transport links.
///
/// When enabled, the orchestrator will periodically close and re-establish
/// transport links to configured endpoints, following a make-before-break
/// strategy: the new link is opened before the old one is closed.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct RotationConf {
    /// Enable/disable rotation.
    #[serde(default)]
    pub enabled: bool,

    /// Rotation policy. Only `Interval` is implemented initially.
    #[serde(default)]
    pub policy: Option<RotationPolicyConf>,

    /// Rotation mode. Only `MakeBeforeBreak` is user-selectable.
    #[serde(default)]
    pub mode: Option<RotationModeConf>,

    /// Fallback configuration for when make-before-break fails.
    #[serde(default)]
    pub fallback: RotationFallbackConf,

    /// Whether to rotate to a different locator in the same group
    /// (only meaningful with `OneOf` strategy).
    #[serde(default = "default_rotate_across_locators")]
    pub rotate_across_locators: bool,
}

fn default_rotate_across_locators() -> bool {
    true
}

impl Default for RotationConf {
    fn default() -> Self {
        Self {
            enabled: false,
            policy: None,
            mode: None,
            fallback: RotationFallbackConf::default(),
            rotate_across_locators: default_rotate_across_locators(),
        }
    }
}

impl RotationConf {
    /// Get the rotation interval in milliseconds, if configured.
    pub fn interval_ms(&self) -> Option<u64> {
        match &self.policy {
            Some(RotationPolicyConf::Interval { interval_ms, .. }) => Some(*interval_ms),
            None => None,
        }
    }

    /// Get the jitter in milliseconds, if configured.
    pub fn jitter_ms(&self) -> Option<u64> {
        match &self.policy {
            Some(RotationPolicyConf::Interval { jitter_ms, .. }) => *jitter_ms,
            None => None,
        }
    }
}
