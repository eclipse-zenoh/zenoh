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

//! # Example: capability negotiation with `MemRegistry`
//!
//! Shows how a session startup routine would:
//!
//! 1. Register SHM (and optionally CUDA) backends in a [`MemRegistry`].
//! 2. Build the local `ext_mem` bytes to advertise in `InitSyn`.
//! 3. Parse a peer's `ext_mem` bytes and call [`MemRegistry::negotiate`] to
//!    obtain a [`NegotiatedMemCaps`] for the connection.
//! 4. Query the outcome per [`ZSliceKind`] at TX time.
//!
//! Run with:
//!
//! ```bash
//! cargo run --example registry_negotiate --features shared-memory
//! ```

use std::sync::Arc;

use zenoh_buffers::ZSliceKind;
use zenoh_mem_transport::{
    backends::shm::ShmBackend,
    caps::{MemBackendCaps, MemInitExt, MemPeerCaps, ShmCaps},
    DowngradePath, MemRegistry, NegotiationResult,
};

fn main() {
    // ── 1. Build a local registry ────────────────────────────────────────────
    //
    // At session startup, register the backends you want to use.
    // The registry is shared (Arc) across all transport connections.

    let mut registry = MemRegistry::new();

    // SHM backend: segment ID 1, supports POSIX protocol (ID = 0).
    registry.register(ShmBackend::new(1, vec![0]));

    // (CUDA backend would be registered here when the `cuda` feature is active.)
    //   #[cfg(feature = "cuda")]
    //   registry.register(CudaIpcBackend::new(vec![0, 1]));

    let registry = Arc::new(registry);

    println!("Local backends: {} registered", registry.local_caps().len());

    // ── 2. Serialize local caps into ext_mem bytes ───────────────────────────
    //
    // This is what MemFsm::local_ext() does inside zenoh-transport.

    let local_ext = MemInitExt::new(registry.local_caps());
    let bytes = local_ext.to_bytes();
    println!("ext_mem bytes ({} bytes): {bytes:?}", bytes.len());

    // ── 3. Simulate receiving a peer's ext_mem (both have SHM) ───────────────

    let peer_ext_bytes = {
        let peer_ext = MemInitExt::new(vec![MemBackendCaps::Shm(ShmCaps { segment_id: 99 })]);
        peer_ext.to_bytes()
    };

    let parsed_peer = MemInitExt::from_bytes(&peer_ext_bytes).expect("peer's ext_mem should parse");

    let peer_caps = MemPeerCaps::new(parsed_peer.backends);
    let negotiated = registry.negotiate(&peer_caps);

    // ── 4. Query negotiation outcome at TX time ───────────────────────────────

    match negotiated.get(ZSliceKind::ShmPtr) {
        Some(entry) => match &entry.result {
            NegotiationResult::Native => println!("ShmPtr: Native zero-copy"),
            NegotiationResult::Fallback(DowngradePath::ToRaw) => {
                println!("ShmPtr: Fallback — copy to raw bytes")
            }
            NegotiationResult::Fallback(DowngradePath::ToShm) => {
                println!("ShmPtr: Fallback — copy into SHM region")
            }
            NegotiationResult::Reject => println!("ShmPtr: Rejected"),
        },
        None => println!("ShmPtr: No backend registered"),
    }

    // ── 5. Peer with no memory backends (raw-only node) ──────────────────────

    let raw_peer = MemPeerCaps::new(vec![]);
    let negotiated_raw = registry.negotiate(&raw_peer);
    match negotiated_raw.get(ZSliceKind::ShmPtr) {
        Some(entry) => match &entry.result {
            NegotiationResult::Fallback(DowngradePath::ToRaw) => {
                println!("Raw-only peer: ShmPtr → fallback to raw (expected)")
            }
            other => println!("Unexpected result for raw peer: {other:?}"),
        },
        None => println!("ShmPtr: No backend registered"),
    }
}
