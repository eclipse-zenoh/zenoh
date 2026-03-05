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

//! [`ShmBackend`]: ZeroMemTransport implementation for POSIX shared memory.
//!
//! Wraps the existing SHM auth/protocol machinery.  The challenge-response
//! handshake (OpenSyn/OpenAck) is still driven by the establishment FSM in
//! `zenoh-transport`; this backend only participates in capability advertisement
//! (`local_caps`) and per-kind negotiation (`negotiate`).

use std::sync::Arc;

use zenoh_buffers::{ZSlice, ZSliceKind};
use zenoh_result::{zerror, ZResult};
use zenoh_shm::api::common::types::ProtocolID;

use crate::{
    caps::{MemBackendCaps, MemBackendId, ShmCaps},
    transport::{DowngradePath, HandleReader, HandleWriter, NegotiationResult, ZeroMemTransport},
};

/// [`ZeroMemTransport`] backend for POSIX shared memory.
pub struct ShmBackend {
    /// Our local AuthSegment ID, advertised in `ext_mem` during InitSyn.
    pub segment_id: u64,
    /// Protocol IDs the local SHM provider supports (e.g. POSIX = 0).
    pub supported_protocols: Vec<ProtocolID>,
}

impl ShmBackend {
    pub fn new(segment_id: u64, supported_protocols: Vec<ProtocolID>) -> Arc<Self> {
        Arc::new(Self {
            segment_id,
            supported_protocols,
        })
    }
}

impl ZeroMemTransport for ShmBackend {
    fn owned_kinds(&self) -> &[ZSliceKind] {
        &[ZSliceKind::ShmPtr]
    }

    fn backend_id(&self) -> MemBackendId {
        MemBackendId::Shm
    }

    fn local_caps(&self) -> MemBackendCaps {
        MemBackendCaps::Shm(ShmCaps {
            segment_id: self.segment_id,
        })
    }

    fn negotiate(&self, peer: Option<&MemBackendCaps>) -> NegotiationResult {
        match peer {
            Some(MemBackendCaps::Shm(_)) => NegotiationResult::Native,
            _ => NegotiationResult::Fallback(DowngradePath::ToRaw),
        }
    }

    fn write_handle(&self, zs: &ZSlice, writer: &mut dyn HandleWriter) -> ZResult<()> {
        // The SHM wire format is: the ShmBufInfo encoded as a ZSlice of bytes.
        // The codec in zenoh-codec/src/core/zbuf.rs already handles this for ShmPtr.
        // This method is a no-op placeholder — the actual encoding lives in the codec
        // for now and will be migrated here in Phase 3.
        let _ = (zs, writer);
        Err(zerror!("ShmBackend::write_handle: use codec directly (Phase 3 migration pending)").into())
    }

    fn read_handle(&self, reader: &mut dyn HandleReader, kind: ZSliceKind) -> ZResult<ZSlice> {
        let _ = (reader, kind);
        Err(zerror!("ShmBackend::read_handle: use codec directly (Phase 3 migration pending)").into())
    }

    fn downgrade(&self, _zs: &ZSlice, _path: DowngradePath) -> Option<ZSlice> {
        // SHM is already a fallback target; it does not further downgrade.
        // (Raw bytes are handled by the codec as ZSliceKind::Raw, not by a backend.)
        None
    }
}
