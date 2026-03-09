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

//! [`MemRegistry`]: runtime registry of zero-copy memory transport backends.

use std::{collections::HashMap, sync::Arc};

use zenoh_buffers::ZSliceKind;

use crate::{
    caps::{MemBackendCaps, MemPeerCaps, NegotiatedMemCaps},
    transport::{ArcBackend, ZeroMemTransport},
};

/// Runtime registry of [`ZeroMemTransport`] backends.
///
/// Constructed once at session startup. Shared (via `Arc`) across all transport
/// connections spawned by the session.
///
/// # Usage
///
/// ```rust,ignore
/// let mut registry = MemRegistry::new();
/// registry.register(Arc::new(ShmBackend::new(shm_provider)));
/// #[cfg(feature = "cuda")]
/// registry.register(Arc::new(CudaIpcBackend::new()));
/// let registry = Arc::new(registry);
/// ```
#[derive(Default)]
pub struct MemRegistry {
    /// Map from ZSliceKind (u8) to the backend that owns it.
    by_kind: HashMap<u8, ArcBackend>,
    /// All registered backends in registration order (for capability advertisement).
    all: Vec<ArcBackend>,
}

impl MemRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a backend.  Panics if any of its `owned_kinds` is already claimed.
    pub fn register(&mut self, backend: Arc<dyn ZeroMemTransport>) {
        for &kind in backend.owned_kinds() {
            let prev = self.by_kind.insert(kind as u8, backend.clone());
            assert!(
                prev.is_none(),
                "ZSliceKind {} is already registered by another backend",
                kind as u8
            );
        }
        self.all.push(backend);
    }

    /// Look up the backend for a given ZSliceKind.
    pub fn backend_for(&self, kind: ZSliceKind) -> Option<&dyn ZeroMemTransport> {
        self.by_kind.get(&(kind as u8)).map(|b| b.as_ref())
    }

    /// Collect all backends' local capability advertisements for `ext_mem`.
    pub fn local_caps(&self) -> Vec<MemBackendCaps> {
        self.all.iter().map(|b| b.local_caps()).collect()
    }

    /// Build a [`NegotiatedMemCaps`] from the peer's advertised capabilities.
    ///
    /// Called once per connection after the handshake completes.
    pub fn negotiate(&self, peer: &MemPeerCaps) -> NegotiatedMemCaps {
        let mut result = NegotiatedMemCaps::new();
        for backend in &self.all {
            let peer_caps = peer.find(backend.backend_id());
            let outcome = backend.negotiate(peer_caps);
            for &kind in backend.owned_kinds() {
                result.set(kind, outcome.clone());
            }
        }
        result
    }

    pub fn is_empty(&self) -> bool {
        self.all.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use zenoh_buffers::ZSliceKind;

    use super::*;
    use crate::{
        backends::shm::ShmBackend,
        caps::{MemBackendCaps, MemPeerCaps, ShmCaps},
        DowngradePath, NegotiationResult,
    };

    fn make_shm_registry() -> MemRegistry {
        let mut reg = MemRegistry::new();
        reg.register(ShmBackend::new(42, vec![0]));
        reg
    }

    #[test]
    fn registry_shm_native_when_peer_has_shm() {
        let reg = make_shm_registry();
        let peer = MemPeerCaps::new(vec![MemBackendCaps::Shm(ShmCaps { segment_id: 99 })]);
        let neg = reg.negotiate(&peer);
        let entry = neg
            .get(ZSliceKind::ShmPtr)
            .expect("ShmPtr should be negotiated");
        assert!(matches!(entry.result, NegotiationResult::Native));
    }

    #[test]
    fn registry_shm_fallback_to_raw_when_peer_has_no_shm() {
        let reg = make_shm_registry();
        let peer = MemPeerCaps::new(vec![]); // peer advertises nothing
        let neg = reg.negotiate(&peer);
        let entry = neg
            .get(ZSliceKind::ShmPtr)
            .expect("ShmPtr should be negotiated");
        assert!(matches!(
            entry.result,
            NegotiationResult::Fallback(DowngradePath::ToRaw)
        ));
    }

    #[test]
    fn registry_local_caps_includes_shm() {
        let reg = make_shm_registry();
        let caps = reg.local_caps();
        assert!(caps.iter().any(|c| matches!(c, MemBackendCaps::Shm(_))));
    }

    #[test]
    fn registry_backend_for_unknown_kind_returns_none() {
        let reg = make_shm_registry();
        // Raw has no registered backend
        assert!(reg.backend_for(ZSliceKind::Raw).is_none());
    }

    #[test]
    #[should_panic]
    fn registry_panics_on_duplicate_kind() {
        let mut reg = MemRegistry::new();
        reg.register(ShmBackend::new(1, vec![0]));
        reg.register(ShmBackend::new(2, vec![0])); // second registration of ShmPtr → panic
    }
}
