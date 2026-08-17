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

//! ⚠️ WARNING ⚠️
//!
//! This module is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)

#[cfg(feature = "acl_postgres")]
mod acl_postgres;
#[cfg(feature = "acl_redis")]
mod acl_redis;

use std::{
    num::NonZeroUsize,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use lru::LruCache;
use serde::Deserialize;
use zenoh_config::{
    AclConfig, AclConfigPolicyEntry, AclConfigRule, AclConfigSubjects, AclIdentitySource,
    AclPolicyStoreBackend,
};
use zenoh_link::LinkAuthId;
use zenoh_result::{bail, zerror, ZResult};
use zenoh_transport::unicast::TransportUnicast;

use super::{authorization::PolicyEnforcer, InterceptorState, RefreshOutcome};

/// Access control lists as held in the policy store, with the same fields as the
/// `access_control` section of the configuration.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AclPolicyDocument {
    #[serde(default)]
    rules: Vec<AclConfigRule>,
    #[serde(default)]
    subjects: Vec<AclConfigSubjects>,
    #[serde(default)]
    policies: Vec<AclConfigPolicyEntry>,
}

/// Backend that can return the ACL document held for one identity.
pub(super) trait PolicyStore: Send + Sync {
    fn fetch(&self, identity: &str) -> ZResult<AclPolicyDocument>;
}

/// Identity of the peer behind a transport, read from the configured attribute.
///
/// `None` means the transport carries no such attribute, leaving it to be enforced with
/// the rules that apply to everyone. An error means the identity could not be
/// established, and the caller is expected to deny the transport.
fn identity_of(transport: &TransportUnicast, source: AclIdentitySource) -> ZResult<Option<String>> {
    let auth_ids = transport
        .get_auth_ids()
        .map_err(|e| zerror!("Cannot read the transport authentication ids: {}", e))?;
    Ok(match source {
        AclIdentitySource::Username => auth_ids.username().cloned(),
        AclIdentitySource::ZenohId => Some(auth_ids.zid().to_string()),
        AclIdentitySource::CertCommonName => {
            let mut names = auth_ids
                .link_auth_ids()
                .iter()
                .filter_map(|auth_id| match auth_id {
                    LinkAuthId::Tls(name) | LinkAuthId::Quic(name) => name.as_ref(),
                    _ => None,
                })
                .collect::<Vec<_>>();
            names.sort_unstable();
            names.dedup();
            match names.as_slice() {
                [] => None,
                [name] => Some((*name).clone()),
                _ => bail!(
                    "Transport presents {} different certificate common names, \
                     its identity is ambiguous",
                    names.len()
                ),
            }
        }
    })
}

/// Compiled policies of the identities this router has seen.
///
/// Holds one policy per identity rather than one for the whole fleet, so that a change
/// costs a single read and a single small compilation.
pub(crate) struct PolicyCache {
    /// The rules, subjects and policies applying to everyone, as declared in the
    /// configuration file.
    base: AclConfig,
    identity: AclIdentitySource,
    entry_ttl_ms: Option<u64>,
    cache_capacity: usize,
    store: Arc<dyn PolicyStore>,
    /// By identity, as read from each transport with [`identity_of`].
    entries: Mutex<LruCache<String, CacheEntry>>,
}

struct CacheEntry {
    policy: Arc<PolicyEnforcer>,
    fetched_at: Instant,
}

impl PolicyCache {
    /// Fails when `base` declares no policy store, or one that cannot hold anything.
    pub(crate) fn new(base: AclConfig) -> ZResult<Self> {
        let Some(conf) = base.policy_store.clone() else {
            bail!("Access control is not configured with a policy store");
        };
        let store: Arc<dyn PolicyStore> = match conf.backend {
            #[cfg(feature = "acl_redis")]
            AclPolicyStoreBackend::Redis(redis) => Arc::new(acl_redis::RedisStore::new(redis)?),
            #[cfg(not(feature = "acl_redis"))]
            AclPolicyStoreBackend::Redis(_) => {
                bail!("Access control is configured with a Redis policy store, but zenoh was built without the `acl_redis` feature")
            }
            #[cfg(feature = "acl_postgres")]
            AclPolicyStoreBackend::Postgres(postgres) => {
                Arc::new(acl_postgres::PostgresStore::new(postgres)?)
            }
            #[cfg(not(feature = "acl_postgres"))]
            AclPolicyStoreBackend::Postgres(_) => {
                bail!("Access control is configured with a Postgres policy store, but zenoh was built without the `acl_postgres` feature")
            }
        };
        Self::with_store(
            base,
            store,
            conf.identity,
            conf.entry_ttl_ms,
            conf.cache_capacity,
        )
    }

    fn with_store(
        base: AclConfig,
        store: Arc<dyn PolicyStore>,
        identity: AclIdentitySource,
        entry_ttl_ms: Option<u64>,
        cache_capacity: usize,
    ) -> ZResult<Self> {
        let capacity = NonZeroUsize::new(cache_capacity)
            .ok_or_else(|| zerror!("Access control cache_capacity must not be zero"))?;
        Ok(Self {
            base,
            identity,
            entry_ttl_ms,
            cache_capacity,
            store,
            entries: Mutex::new(LruCache::new(capacity)),
        })
    }

    /// Policy held for a transport, or `None` when it carries no identity and is
    /// therefore left to the rules applying to everyone.
    ///
    /// Uses [`Self::held`], so a stale entry is still the policy this transport was
    /// admitted with. Never reads the store, so that it can be called while the routing tables
    /// are locked. [`InterceptorState::prepare`] has already refused a transport whose
    /// policy could not be established, so a miss here is left to the configuration-file
    /// enforcer.
    pub(crate) fn held_for(&self, transport: &TransportUnicast) -> Option<Arc<PolicyEnforcer>> {
        let identity = identity_of(transport, self.identity).ok().flatten()?;
        self.held(&identity)
    }

    /// Returns a still-fresh policy, reading the store when none is held or the ttl has elapsed.
    fn fresh_or_fetch(&self, identity: &str) -> ZResult<Arc<PolicyEnforcer>> {
        if let Some(policy) = self.fresh(identity) {
            return Ok(policy);
        }
        // nothing prevents two connections of the same identity from reading
        // the store at the same time. Both get the same answer, so the cost is one wasted
        // read; a map of in-flight reads is the upgrade if it ever shows up.
        self.fetch_and_store(identity)
    }

    fn fetch_and_store(&self, identity: &str) -> ZResult<Arc<PolicyEnforcer>> {
        let document = self.store.fetch(identity)?;
        let policy = Arc::new(self.compile(document)?);
        self.insert(identity, policy.clone());
        Ok(policy)
    }

    fn insert(&self, identity: &str, policy: Arc<PolicyEnforcer>) {
        let mut entries = self.entries.lock().unwrap();
        if let Some((evicted, _)) = entries.push(
            identity.to_string(),
            CacheEntry {
                policy,
                fetched_at: Instant::now(),
            },
        ) {
            if evicted != identity {
                tracing::warn!(
                    "Access control cache is full (capacity {}), dropping identity '{}'",
                    self.cache_capacity,
                    evicted
                );
            }
        }
    }

    /// Policy held for `identity` if it is still within `entry_ttl_ms`.
    ///
    /// Past the ttl the entry is dropped so the next [`Self::fresh_or_fetch`] reads the store
    /// again. Does not apply the ttl when it is `None`.
    fn fresh(&self, identity: &str) -> Option<Arc<PolicyEnforcer>> {
        let mut entries = self.entries.lock().unwrap();
        let entry = entries.get(identity)?;
        match self.entry_ttl_ms {
            Some(ttl) if entry.fetched_at.elapsed() >= Duration::from_millis(ttl) => {
                entries.pop(identity);
                None
            }
            _ => Some(entry.policy.clone()),
        }
    }

    /// Last policy stored for an identity, even if it is past its ttl.
    ///
    /// Used while interceptors are built under the routing tables lock: a stale entry is
    /// still the policy this transport was admitted with, until [`InterceptorState::refresh`]
    /// replaces it.
    fn held(&self, identity: &str) -> Option<Arc<PolicyEnforcer>> {
        self.entries
            .lock()
            .unwrap()
            .get(identity)
            .map(|entry| entry.policy.clone())
    }

    /// Drops the policy held for an identity, so that it is read again when next needed.
    #[cfg(test)]
    fn invalidate(&self, identity: &str) {
        self.entries.lock().unwrap().pop(identity);
    }

    /// Compiles the rules applying to everyone together with those held for one identity.
    fn compile(&self, document: AclPolicyDocument) -> ZResult<PolicyEnforcer> {
        let mut enforcer = PolicyEnforcer::new();
        enforcer.init(&with_empty_lists(&merge(&self.base, document)))?;
        Ok(enforcer)
    }
}

impl InterceptorState for PolicyCache {
    /// Reads the policy for the identity behind a transport and holds it.
    ///
    /// Failure refuses the transport: there is then no session to enforce a fallback on.
    fn prepare(&self, transport: &TransportUnicast) -> ZResult<()> {
        match identity_of(transport, self.identity) {
            Ok(None) => Ok(()),
            Ok(Some(identity)) => self.fresh_or_fetch(&identity).map(|_| ()),
            Err(e) => Err(e),
        }
    }

    fn refresh(&self, identity: &str) -> RefreshOutcome {
        match self.fetch_and_store(identity) {
            Ok(_) => RefreshOutcome::Updated,
            Err(e) => {
                tracing::error!(
                    "Cannot read the access control policy for identity '{}': {}",
                    identity,
                    e
                );
                // Drop the held policy so a reconnect cannot reuse it while the store is still
                // unreachable. The gateway closes matching transports.
                self.entries.lock().unwrap().pop(identity);
                RefreshOutcome::Failed
            }
        }
    }

    fn identity_of(&self, transport: &TransportUnicast) -> Option<String> {
        identity_of(transport, self.identity).ok().flatten()
    }

    fn stale_identities(&self) -> Vec<String> {
        let Some(ttl) = self.entry_ttl_ms.map(Duration::from_millis) else {
            return Vec::new();
        };
        self.entries
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, entry)| entry.fetched_at.elapsed() >= ttl)
            .map(|(identity, _)| identity.clone())
            .collect()
    }
}

/// The same configuration with its three lists always present.
///
/// The policy store holds the rules for each identity, so the configuration file may
/// declare none at all, and an identity the store holds nothing for must be enforced with
/// `default_permission`.
/// Both are cases `PolicyEnforcer::init` would otherwise refuse outright.
pub(crate) fn with_empty_lists(config: &AclConfig) -> AclConfig {
    let mut config = config.clone();
    config.rules.get_or_insert_with(Vec::new);
    config.subjects.get_or_insert_with(Vec::new);
    config.policies.get_or_insert_with(Vec::new);
    config
}

pub(super) fn parse(document: &str) -> ZResult<AclPolicyDocument> {
    json5::from_str(document).map_err(|e| {
        zerror!(
            "Invalid access control document read from the policy store: {}",
            e
        )
        .into()
    })
}

/// Appends the lists read from the policy store to the ones declared in the configuration file.
///
/// Ids are deliberately not deduplicated: a collision between the two sources makes the
/// resulting policy ambiguous, and is reported when it is compiled.
fn merge(config: &AclConfig, document: AclPolicyDocument) -> AclConfig {
    fn append<T>(base: &mut Option<Vec<T>>, mut extra: Vec<T>) {
        if extra.is_empty() {
            return;
        }
        base.get_or_insert_with(Vec::new).append(&mut extra);
    }

    let mut merged = config.clone();
    append(&mut merged.rules, document.rules);
    append(&mut merged.subjects, document.subjects);
    append(&mut merged.policies, document.policies);
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    const STORE_DOCUMENT: &str = r#"{
        "rules": [
            {"id": "store-rule", "key_exprs": ["a/**"], "messages": ["put"], "permission": "allow"}
        ],
        "subjects": [{"id": "store-subject", "usernames": ["alice"]}],
        "policies": [{"rules": ["store-rule"], "subjects": ["store-subject"]}]
    }"#;

    const OTHER_DOCUMENT: &str = r#"{
        "rules": [
            {"id": "other-rule", "key_exprs": ["b/**"], "messages": ["put"], "permission": "deny"}
        ],
        "subjects": [{"id": "other-subject", "usernames": ["bob"]}],
        "policies": [{"rules": ["other-rule"], "subjects": ["other-subject"]}]
    }"#;

    struct NullStore;

    impl PolicyStore for NullStore {
        fn fetch(&self, _: &str) -> ZResult<AclPolicyDocument> {
            Ok(AclPolicyDocument::default())
        }
    }

    struct ScriptedStore {
        answers: Mutex<std::collections::VecDeque<ZResult<AclPolicyDocument>>>,
    }

    impl PolicyStore for ScriptedStore {
        fn fetch(&self, _: &str) -> ZResult<AclPolicyDocument> {
            self.answers
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected fetch")
        }
    }

    fn cache_with(entry_ttl_ms: Option<u64>, cache_capacity: usize) -> PolicyCache {
        PolicyCache::with_store(
            AclConfig {
                enabled: true,
                ..AclConfig::default()
            },
            Arc::new(NullStore),
            AclIdentitySource::Username,
            entry_ttl_ms,
            cache_capacity,
        )
        .unwrap()
    }

    fn cache_policy(cache: &PolicyCache, identity: &str, document: &str) {
        let policy = Arc::new(cache.compile(parse(document).unwrap()).unwrap());
        cache.insert(identity, policy);
    }

    #[test]
    fn store_document_is_appended_to_the_configured_lists() {
        // The store alone can provide the whole policy.
        let from_store = merge(&AclConfig::default(), parse(STORE_DOCUMENT).unwrap());
        assert_eq!(from_store.rules.as_ref().unwrap().len(), 1);
        assert_eq!(from_store.subjects.as_ref().unwrap().len(), 1);
        assert_eq!(from_store.policies.as_ref().unwrap().len(), 1);

        // What it provides is added to what the configuration file already declares,
        // rather than replacing it.
        let merged = merge(&from_store, parse(OTHER_DOCUMENT).unwrap());
        let rules = merged.rules.as_ref().unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].id, "store-rule");
        assert_eq!(rules[1].id, "other-rule");
        assert_eq!(merged.subjects.as_ref().unwrap().len(), 2);
        assert_eq!(merged.policies.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn a_colliding_id_between_file_and_store_is_rejected() {
        // Merge does not deduplicate; a shared id would otherwise make the compiled
        // policy ambiguous.
        let cache = PolicyCache::with_store(
            merge(
                &AclConfig {
                    enabled: true,
                    ..AclConfig::default()
                },
                parse(STORE_DOCUMENT).unwrap(),
            ),
            Arc::new(NullStore),
            AclIdentitySource::Username,
            None,
            8,
        )
        .unwrap();
        assert!(cache.compile(parse(STORE_DOCUMENT).unwrap()).is_err());
    }

    #[test]
    fn empty_document_leaves_the_configuration_untouched() {
        let merged = merge(&AclConfig::default(), parse("{}").unwrap());
        assert!(merged.rules.is_none());
        assert!(merged.subjects.is_none());
        assert!(merged.policies.is_none());
    }

    #[test]
    fn an_identity_the_store_holds_nothing_for_still_compiles() {
        // Without this, such an identity would be denied by a compilation error rather than
        // enforced with `default_permission`.
        let cache = cache_with(Some(300_000), 8);
        let policy = cache.compile(parse("{}").unwrap()).unwrap();
        assert!(policy.acl_enabled);
        assert!(policy.policy_map.is_empty());
    }

    #[test]
    fn fresh_drops_a_policy_past_its_ttl() {
        let cache = cache_with(Some(20), 8);
        cache_policy(&cache, "alice", STORE_DOCUMENT);
        assert!(cache.fresh("alice").is_some());
        assert!(cache.held("alice").is_some());

        std::thread::sleep(Duration::from_millis(30));
        // `held` ignores the ttl; `fresh` applies it (and drops the entry).
        assert!(cache.held("alice").is_some());
        assert!(cache.fresh("alice").is_none());
        assert!(cache.held("alice").is_none());

        // A cache without a ttl trusts what it holds until something invalidates it.
        let cache = cache_with(None, 8);
        cache_policy(&cache, "alice", STORE_DOCUMENT);
        std::thread::sleep(Duration::from_millis(30));
        assert!(cache.fresh("alice").is_some());
        assert!(cache.held("alice").is_some());

        cache.invalidate("alice");
        assert!(cache.fresh("alice").is_none());
        assert!(cache.held("alice").is_none());
    }

    #[test]
    fn policies_past_their_ttl_are_reported_as_stale() {
        let cache = cache_with(Some(20), 8);
        cache_policy(&cache, "alice", STORE_DOCUMENT);
        assert!(cache.stale_identities().is_empty());

        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(cache.stale_identities(), vec!["alice".to_string()]);

        // Without a ttl nothing goes stale on its own, leaving an explicit refresh as the
        // only way a change reaches a connected identity.
        let cache = cache_with(None, 8);
        cache_policy(&cache, "alice", STORE_DOCUMENT);
        std::thread::sleep(Duration::from_millis(30));
        assert!(cache.stale_identities().is_empty());
    }

    #[test]
    fn holding_more_identities_than_capacity_drops_the_least_recently_used() {
        let cache = cache_with(None, 2);
        cache_policy(&cache, "alice", STORE_DOCUMENT);
        cache_policy(&cache, "bob", STORE_DOCUMENT);
        // Reading alice makes bob the next to go.
        assert!(cache.held("alice").is_some());
        cache_policy(&cache, "carol", STORE_DOCUMENT);

        assert!(cache.held("bob").is_none());
        assert!(cache.held("alice").is_some());
        assert!(cache.held("carol").is_some());
    }

    #[test]
    fn a_fetched_document_is_compiled_and_a_failed_refresh_drops_it() {
        let cache = PolicyCache::with_store(
            AclConfig {
                enabled: true,
                ..AclConfig::default()
            },
            Arc::new(ScriptedStore {
                answers: Mutex::new(std::collections::VecDeque::from([
                    Ok(parse(STORE_DOCUMENT).unwrap()),
                    Err(zerror!("store unreachable").into()),
                ])),
            }),
            AclIdentitySource::Username,
            None,
            8,
        )
        .unwrap();

        let policy = cache.fresh_or_fetch("alice").unwrap();
        assert!(!policy.policy_map.is_empty());
        assert!(cache.held("alice").is_some());

        // A later read that cannot reach the store must not leave the last policy in
        // place for a reconnect.
        assert!(matches!(cache.refresh("alice"), RefreshOutcome::Failed));
        assert!(cache.held("alice").is_none());
    }

    #[test]
    fn malformed_document_is_rejected() {
        // A misspelled field would otherwise silently drop rules the operator believes
        // are enforced.
        assert!(parse(r#"{"rulez": []}"#).is_err());
        assert!(parse("not json").is_err());
        assert!(parse(r#"{"rules": [{"id": "no-key-exprs"}]}"#).is_err());
    }
}
