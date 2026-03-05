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

//! FSM for the `ext_mem` handshake extension (CUDA IPC / zero-copy backends).
//!
//! ## Design
//!
//! `ext_mem` (extension ID `0x8`) carries a serialised [`MemInitExt`] blob
//! inside an `InitSyn` / `InitAck` message.  The blob lists the non-SHM
//! backends (CUDA IPC, RDMA, …) supported by the local peer.
//!
//! SHM capability is conveyed separately by the existing `ext_shm` (ID `0x2`)
//! challenge-response mechanism.  [`MemFsm::negotiate`] synthesises a unified
//! [`NegotiatedMemCaps`] by combining both sources:
//!
//! | peer signal         | ShmPtr result           | CudaPtr result                   |
//! |---------------------|-------------------------|----------------------------------|
//! | `ext_shm` present   | `Native`                | —                                |
//! | `ext_shm` absent    | `Fallback(ToRaw)`       | —                                |
//! | `ext_mem` CudaIpc   | —                       | `Native` (same-host) or fallback |
//! | `ext_mem` absent    | —                       | `Fallback(ToRaw)`                |
//!
//! Unlike `ext_shm`, there is **no Open-phase** for `ext_mem`.  The negotiation
//! result is available immediately after `InitAck` is processed.

use std::sync::Arc;

use zenoh_buffers::{reader::HasReader, ZBuf};
use zenoh_mem_transport::{
    caps::ShmCaps, MemBackendCaps, MemInitExt, MemPeerCaps, MemRegistry, NegotiatedMemCaps,
};
use zenoh_protocol::transport::init;

/// The mem extension FSM — used by both the accepting and opening sides.
pub(crate) struct MemFsm {
    registry: Arc<MemRegistry>,
}

impl MemFsm {
    pub(crate) fn new(registry: Arc<MemRegistry>) -> Self {
        Self { registry }
    }

    /// Serialise local capabilities into `ext_mem` for an outgoing InitSyn or InitAck.
    pub(crate) fn local_ext(&self) -> Option<init::ext::Mem> {
        let caps = self.registry.local_caps();
        // Only include non-SHM backends in ext_mem (SHM auth is in ext_shm).
        let cuda_caps: Vec<_> = caps
            .into_iter()
            .filter(|c| !matches!(c, MemBackendCaps::Shm(_)))
            .collect();
        if cuda_caps.is_empty() {
            return None;
        }
        let ext = MemInitExt::new(cuda_caps);
        let bytes: Vec<u8> = ext.to_bytes();
        let zbuf = ZBuf::from(bytes);
        Some(init::ext::Mem::new(zbuf))
    }

    /// Build [`NegotiatedMemCaps`] from:
    /// - `peer_ext_mem`: the remote peer's ext_mem (CUDA / other caps)
    /// - `peer_has_shm`: whether the peer sent ext_shm (meaning SHM is available)
    pub(crate) fn negotiate(
        &self,
        peer_ext_mem: Option<init::ext::Mem>,
        peer_has_shm: bool,
    ) -> NegotiatedMemCaps {
        let mut peer_backends: Vec<MemBackendCaps> = Vec::new();

        // If peer sent ext_shm, they have SHM capability.
        if peer_has_shm {
            peer_backends.push(MemBackendCaps::Shm(ShmCaps { segment_id: 0 }));
        }

        // Parse ext_mem for CUDA / other caps.
        if let Some(mem_ext) = peer_ext_mem {
            // Collect ZBuf into a contiguous byte vec for parsing.
            let mut reader = mem_ext.value.reader();
            let mut bytes: Vec<u8> = Vec::new();
            use zenoh_buffers::reader::Reader;
            while reader.can_read() {
                if let Ok(b) = reader.read_u8() {
                    bytes.push(b);
                } else {
                    break;
                }
            }
            if let Some(parsed) = MemInitExt::from_bytes(&bytes) {
                peer_backends.extend(parsed.backends);
            }
        }

        let peer_caps = MemPeerCaps::new(peer_backends);
        self.registry.negotiate(&peer_caps)
    }
}
