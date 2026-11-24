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
}

fn default_fallback_enabled() -> bool {
    true
}

impl Default for RotationFallbackConf {
    fn default() -> Self {
        Self {
            enabled: default_fallback_enabled(),
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
        self.policy
            .as_ref()
            .map(|RotationPolicyConf::Interval { interval_ms, .. }| *interval_ms)
    }

    /// Get the jitter in milliseconds, if configured.
    pub fn jitter_ms(&self) -> Option<u64> {
        match &self.policy {
            Some(RotationPolicyConf::Interval { jitter_ms, .. }) => *jitter_ms,
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_disabled_with_no_policy() {
        let conf = RotationConf::default();
        assert!(!conf.enabled);
        assert_eq!(conf.policy, None);
        assert_eq!(conf.mode, None);
        assert!(conf.fallback.enabled);
        assert!(conf.rotate_across_locators);
    }

    #[test]
    fn interval_and_jitter_are_none_without_a_policy() {
        let conf = RotationConf::default();
        assert_eq!(conf.interval_ms(), None);
        assert_eq!(conf.jitter_ms(), None);
    }

    #[test]
    fn interval_and_jitter_reflect_the_interval_policy() {
        let conf = RotationConf {
            enabled: true,
            policy: Some(RotationPolicyConf::Interval {
                interval_ms: 300_000,
                jitter_ms: Some(30_000),
            }),
            ..RotationConf::default()
        };
        assert_eq!(conf.interval_ms(), Some(300_000));
        assert_eq!(conf.jitter_ms(), Some(30_000));
    }

    #[test]
    fn jitter_defaults_to_none_when_omitted_from_the_policy() {
        let conf = RotationConf {
            enabled: true,
            policy: Some(RotationPolicyConf::Interval {
                interval_ms: 60_000,
                jitter_ms: None,
            }),
            ..RotationConf::default()
        };
        assert_eq!(conf.interval_ms(), Some(60_000));
        assert_eq!(conf.jitter_ms(), None);
    }

    #[test]
    fn fallback_conf_default_is_enabled() {
        assert!(RotationFallbackConf::default().enabled);
    }

    #[test]
    fn deserializes_from_json5_matching_default_config() {
        // Mirrors the `connect.rotation` block documented in DEFAULT_CONFIG.json5.
        let json5 = r#"
            {
              enabled: false,
              policy: {
                type: "interval",
                interval_ms: 300000,
                jitter_ms: 30000,
              },
              mode: "make_before_break",
              fallback: {
                enabled: true,
              },
              rotate_across_locators: true,
            }
        "#;

        let conf: RotationConf =
            json5::from_str(json5).expect("DEFAULT_CONFIG.json5 rotation block must deserialize");

        assert!(!conf.enabled);
        assert_eq!(
            conf.policy,
            Some(RotationPolicyConf::Interval {
                interval_ms: 300_000,
                jitter_ms: Some(30_000),
            })
        );
        assert_eq!(conf.mode, Some(RotationModeConf::MakeBeforeBreak));
        assert!(conf.fallback.enabled);
        assert!(conf.rotate_across_locators);
    }

    #[test]
    fn deserializes_with_all_fields_defaulted_when_only_enabled_is_set() {
        let json5 = r#"{ enabled: true }"#;

        let conf: RotationConf = json5::from_str(json5).unwrap();

        assert!(conf.enabled);
        assert_eq!(conf.policy, None);
        assert_eq!(conf.mode, None);
        assert!(conf.fallback.enabled);
        assert!(conf.rotate_across_locators);
        // Without a policy, the engine has nothing to schedule on.
        assert_eq!(conf.interval_ms(), None);
    }

    #[test]
    fn policy_type_tag_rejects_unknown_variants() {
        let json5 = r#"{ type: "byte_threshold", interval_ms: 1000 }"#;
        let result: Result<RotationPolicyConf, _> = json5::from_str(json5);
        assert!(result.is_err(), "unknown policy types must be rejected");
    }

    #[test]
    fn mode_serde_rename_round_trips() {
        let mode = RotationModeConf::MakeBeforeBreak;
        let serialized = serde_json::to_string(&mode).unwrap();
        assert_eq!(serialized, "\"make_before_break\"");

        let back: RotationModeConf = serde_json::from_str(&serialized).unwrap();
        assert_eq!(back, RotationModeConf::MakeBeforeBreak);
    }

    #[test]
    fn interval_policy_round_trips_through_json() {
        let policy = RotationPolicyConf::Interval {
            interval_ms: 45_000,
            jitter_ms: Some(5_000),
        };
        let serialized = serde_json::to_string(&policy).unwrap();
        let back: RotationPolicyConf = serde_json::from_str(&serialized).unwrap();
        assert_eq!(back, policy);
    }
}
