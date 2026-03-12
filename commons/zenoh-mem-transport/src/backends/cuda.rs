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

//! [`CudaIpcBackend`]: ZeroMemTransport implementation for CUDA IPC.
//!
//! Handles `ZSliceKind::CudaPtr` and `ZSliceKind::CudaTensor`.
//!
//! ## Downgrade paths
//!
//! When the remote peer does not support CUDA IPC (e.g. a non-CUDA subscriber,
//! or a subscriber on a different host), the backend can fall back:
//!
//! - [`DowngradePath::ToRaw`]: allocates a host staging buffer and copies GPU→host
//!   via `cudaMemcpy DeviceToHost`, then sends as `ZSliceKind::Raw`.
//! - [`DowngradePath::ToShm`]: reserved for future use (GPU→SHM mapping).

use std::sync::Arc;

use zenoh_buffers::{ZSlice, ZSliceBuffer, ZSliceKind};
use zenoh_cuda::CudaBufInner;
use zenoh_result::{zerror, ZResult};

use crate::{
    caps::{CudaIpcCaps, MemBackendCaps, MemBackendId},
    transport::{DowngradePath, HandleReader, HandleWriter, NegotiationResult, ZeroMemTransport},
};

/// [`ZeroMemTransport`] backend for CUDA IPC (same-host zero-copy GPU transport).
pub struct CudaIpcBackend {
    /// Device IDs this peer can share from (empty = current device only).
    local_device_ids: Vec<i32>,
}

impl CudaIpcBackend {
    /// Create a backend advertising the given device IDs.
    pub fn new(device_ids: Vec<i32>) -> Arc<Self> {
        Arc::new(Self {
            local_device_ids: device_ids,
        })
    }
}

impl ZeroMemTransport for CudaIpcBackend {
    fn owned_kinds(&self) -> &[ZSliceKind] {
        &[ZSliceKind::CudaPtr, ZSliceKind::CudaTensor]
    }

    fn backend_id(&self) -> MemBackendId {
        MemBackendId::CudaIpc
    }

    fn local_caps(&self) -> MemBackendCaps {
        MemBackendCaps::CudaIpc(CudaIpcCaps {
            device_ids: self.local_device_ids.clone(),
        })
    }

    fn negotiate(&self, peer: Option<&MemBackendCaps>) -> NegotiationResult {
        match peer {
            Some(MemBackendCaps::CudaIpc(_)) => NegotiationResult::Native,
            Some(MemBackendCaps::Shm(_)) => NegotiationResult::Fallback(DowngradePath::ToShm),
            _ => NegotiationResult::Fallback(DowngradePath::ToRaw),
        }
    }

    fn write_handle(&self, zs: &ZSlice, writer: &mut dyn HandleWriter) -> ZResult<()> {
        // Encoding is still handled by the codec in zenoh-codec/src/core/zbuf.rs
        // for Phase 1/2. This will be fully migrated in Phase 3.
        let _ = (zs, writer);
        Err(
            zerror!("CudaIpcBackend::write_handle: codec handles encoding (Phase 3 pending)")
                .into(),
        )
    }

    fn read_handle(&self, reader: &mut dyn HandleReader, kind: ZSliceKind) -> ZResult<ZSlice> {
        let _ = (reader, kind);
        Err(zerror!("CudaIpcBackend::read_handle: codec handles decoding (Phase 3 pending)").into())
    }

    fn downgrade(&self, zs: &ZSlice, path: DowngradePath) -> Option<ZSlice> {
        let cuda = zs.downcast_ref::<CudaBufInner>()?;
        match path {
            DowngradePath::ToRaw => {
                // Allocate a host staging buffer and cudaMemcpy device→host.
                let mut staging = vec![0u8; cuda.cuda_len];
                if let Err(e) = cuda.copy_to_host(&mut staging) {
                    tracing::error!("CudaIpcBackend downgrade ToRaw: cudaMemcpy failed: {e}");
                    return None;
                }
                // Wrap in Arc<Vec<u8>> which implements ZSliceBuffer.
                let arc: Arc<dyn ZSliceBuffer> = Arc::new(staging);
                let len = arc.as_slice().len();
                ZSlice::new(arc, 0, len).ok()
            }
            DowngradePath::ToShm => {
                // TODO Phase 4: copy into an SHM-mapped region.
                // Requires a reference to the session's ShmProvider.
                // For now, fall back to raw bytes.
                tracing::warn!(
                    "CudaIpcBackend downgrade ToShm: SHM staging not yet implemented, using raw"
                );
                self.downgrade(zs, DowngradePath::ToRaw)
            }
        }
    }
}

#[cfg(all(test, feature = "cuda"))]
mod tests {
    use std::sync::Arc;

    use zenoh_buffers::{ZSlice, ZSliceBuffer, ZSliceKind};
    use zenoh_cuda::CudaBufInner;

    use super::{CudaIpcBackend, DowngradePath, ZeroMemTransport};

    // ── cross-capability: CUDA device memory → raw bytes via cudaMemcpy ──────
    //
    // Simulates the TX path when the peer does not support CUDA IPC: the
    // publisher's CUDA ZSlice is downgraded to raw host bytes before sending.
    //
    // cudaMalloc returns zero-initialised memory, so the expected bytes are all
    // zero.  We use this to avoid needing a cudaMemcpy HostToDevice in the test.
    #[test]
    #[ignore = "requires CUDA device"]
    fn test_cuda_downgrade_to_raw() {
        const LEN: usize = 128;

        // Allocate zeroed device memory and wrap as a ZSlice.
        let buf = CudaBufInner::alloc_device(LEN, 0).expect("cudaMalloc");
        let arc: Arc<dyn ZSliceBuffer> = Arc::new(buf);
        let zs = ZSlice::new(arc.clone(), 0, LEN).expect("ZSlice::new");
        // Confirm the kind is CudaPtr (set by ZBuf::from_cuda; here we set it
        // directly since we're below the ZBuf layer).
        let mut zs = zs;
        zs.kind = ZSliceKind::CudaPtr;

        let backend = CudaIpcBackend::new(vec![]);
        let raw = backend
            .downgrade(&zs, DowngradePath::ToRaw)
            .expect("downgrade should succeed for device memory");

        assert!(
            raw.kind == ZSliceKind::Raw,
            "downgraded slice must be ZSliceKind::Raw"
        );
        assert_eq!(raw.len(), LEN, "byte count must be preserved");
        assert!(
            raw.as_slice().iter().all(|&b| b == 0),
            "cudaMalloc-zeroed device memory must round-trip as zero bytes"
        );
    }

    // ── negotiate: no peer caps → Fallback(ToRaw) ─────────────────────────────
    //
    // Tests the capability negotiation logic without a GPU: when the peer
    // advertises no CUDA caps, the backend must request a ToRaw downgrade.
    // This is a pure-logic test (no CUDA device needed).
    #[test]
    fn test_negotiate_no_peer_falls_back_to_raw() {
        use crate::transport::NegotiationResult;

        let backend = CudaIpcBackend::new(vec![]);
        let result = backend.negotiate(None);
        assert!(
            matches!(result, NegotiationResult::Fallback(DowngradePath::ToRaw)),
            "no peer caps should produce Fallback(ToRaw), got {result:?}"
        );
    }

    // ── negotiate: matching CUDA peer → Native ────────────────────────────────
    #[test]
    fn test_negotiate_cuda_peer_native() {
        use crate::{
            caps::{CudaIpcCaps, MemBackendCaps},
            transport::NegotiationResult,
        };

        let backend = CudaIpcBackend::new(vec![0]);
        let peer = MemBackendCaps::CudaIpc(CudaIpcCaps {
            device_ids: vec![0],
        });
        let result = backend.negotiate(Some(&peer));
        assert!(
            matches!(result, NegotiationResult::Native),
            "matching CUDA peer should be Native, got {result:?}"
        );
    }
}
