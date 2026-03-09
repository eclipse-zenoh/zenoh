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
