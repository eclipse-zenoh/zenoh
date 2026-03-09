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

//! Capability types for the `ext_mem` handshake extension.

use zenoh_buffers::ZSliceKind;

/// Wire identifier for each known backend.
///
/// Adding a new backend requires a protocol version bump and a new variant here.
/// Closed enum by design: the wire format must be stable and exhaustive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MemBackendId {
    Shm = 0,
    CudaIpc = 1,
    Rdma = 2, // reserved — not yet implemented
}

impl MemBackendId {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Shm),
            1 => Some(Self::CudaIpc),
            2 => Some(Self::Rdma),
            _ => None,
        }
    }
}

/// Backend-specific capability data advertised during the handshake.
#[derive(Debug, Clone)]
pub enum MemBackendCaps {
    /// POSIX SHM: the auth segment ID (u64) for the challenge-response handshake.
    Shm(ShmCaps),
    /// CUDA IPC: list of device ordinals the peer can share from.
    CudaIpc(CudaIpcCaps),
    /// RDMA: reserved placeholder.
    Rdma(RdmaCaps),
}

impl MemBackendCaps {
    pub fn id(&self) -> MemBackendId {
        match self {
            Self::Shm(_) => MemBackendId::Shm,
            Self::CudaIpc(_) => MemBackendId::CudaIpc,
            Self::Rdma(_) => MemBackendId::Rdma,
        }
    }
}

/// SHM capability: the initiator's auth segment ID used for challenge-response.
#[derive(Debug, Clone)]
pub struct ShmCaps {
    /// AuthSegment ID sent in InitSyn (equivalent to the old `ext_shm` payload).
    pub segment_id: u64,
}

/// CUDA IPC capability: device ordinals available on this peer.
#[derive(Debug, Clone)]
pub struct CudaIpcCaps {
    pub device_ids: Vec<i32>,
}

/// RDMA capability: reserved for future use.
#[derive(Debug, Clone)]
pub struct RdmaCaps {
    _reserved: (),
}

/// Capabilities advertised by a remote peer in `ext_mem`.
///
/// Built from the decoded `InitSyn`/`InitAck` extension and stored for the
/// lifetime of the transport connection.
#[derive(Debug, Clone, Default)]
pub struct MemPeerCaps {
    backends: Vec<MemBackendCaps>,
}

impl MemPeerCaps {
    pub fn new(backends: Vec<MemBackendCaps>) -> Self {
        Self { backends }
    }

    /// Find this peer's capability for a given backend, if advertised.
    pub fn find(&self, id: MemBackendId) -> Option<&MemBackendCaps> {
        self.backends.iter().find(|c| c.id() == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &MemBackendCaps> {
        self.backends.iter()
    }
}

/// Result of handshake negotiation stored per transport connection.
///
/// Records, for each [`ZSliceKind`], what was agreed during the handshake.
/// Used in `map_to_partner` on every TX message.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NegotiatedMemCaps {
    // Indexed by ZSliceKind discriminant (u8).
    // None = no backend registered for this kind (treat as Raw).
    entries: Vec<Option<NegotiatedEntry>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NegotiatedEntry {
    pub result: super::NegotiationResult,
}

impl NegotiatedMemCaps {
    pub fn new() -> Self {
        Self {
            entries: vec![None; 8], // headroom for up to 8 ZSliceKind values
        }
    }

    pub fn set(&mut self, kind: ZSliceKind, result: super::NegotiationResult) {
        let idx = kind as usize;
        if idx >= self.entries.len() {
            self.entries.resize(idx + 1, None);
        }
        self.entries[idx] = Some(NegotiatedEntry { result });
    }

    pub fn get(&self, kind: ZSliceKind) -> Option<&NegotiatedEntry> {
        self.entries.get(kind as usize)?.as_ref()
    }
}

/// Wire format for the `ext_mem` handshake extension.
///
/// Serialized as bytes inside a `ZExtZBuf(0x8)`.
/// Only carries non-SHM backends (CUDA IPC, RDMA, …).
/// SHM capability is conveyed separately via the existing `ext_shm` extension.
pub struct MemInitExt {
    pub backends: Vec<MemBackendCaps>,
}

const WIRE_VERSION: u8 = 1;

impl MemInitExt {
    pub fn new(backends: Vec<MemBackendCaps>) -> Self {
        Self { backends }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.push(WIRE_VERSION);
        out.push(self.backends.len() as u8);
        for backend in &self.backends {
            match backend {
                MemBackendCaps::CudaIpc(caps) => {
                    out.push(MemBackendId::CudaIpc as u8);
                    let n = caps.device_ids.len().min(255) as u8;
                    let caps_len = 1u16 + n as u16 * 4;
                    out.extend_from_slice(&caps_len.to_le_bytes());
                    out.push(n);
                    for &id in caps.device_ids.iter().take(n as usize) {
                        out.extend_from_slice(&id.to_le_bytes());
                    }
                }
                MemBackendCaps::Rdma(_) => {
                    out.push(MemBackendId::Rdma as u8);
                    out.extend_from_slice(&0u16.to_le_bytes());
                }
                MemBackendCaps::Shm(caps) => {
                    out.push(MemBackendId::Shm as u8);
                    let caps_len = 8u16;
                    out.extend_from_slice(&caps_len.to_le_bytes());
                    out.extend_from_slice(&caps.segment_id.to_le_bytes());
                }
            }
        }
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() {
            return None;
        }
        let version = bytes[0];
        if version != WIRE_VERSION {
            return None;
        }
        if bytes.len() < 2 {
            return None;
        }
        let n = bytes[1] as usize;
        let mut pos = 2;
        let mut backends = Vec::with_capacity(n);
        for _ in 0..n {
            if pos + 3 > bytes.len() {
                return None;
            }
            let id = bytes[pos];
            let len = u16::from_le_bytes([bytes[pos + 1], bytes[pos + 2]]) as usize;
            pos += 3;
            if pos + len > bytes.len() {
                return None;
            }
            let cap_bytes = &bytes[pos..pos + len];
            pos += len;
            match id {
                1 => {
                    // CudaIpc
                    if cap_bytes.is_empty() {
                        return None;
                    }
                    let n_dev = cap_bytes[0] as usize;
                    if 1 + n_dev * 4 > cap_bytes.len() {
                        return None;
                    }
                    let mut device_ids = Vec::with_capacity(n_dev);
                    for i in 0..n_dev {
                        let off = 1 + i * 4;
                        let v = i32::from_le_bytes([
                            cap_bytes[off],
                            cap_bytes[off + 1],
                            cap_bytes[off + 2],
                            cap_bytes[off + 3],
                        ]);
                        device_ids.push(v);
                    }
                    backends.push(MemBackendCaps::CudaIpc(CudaIpcCaps { device_ids }));
                }
                0 => {
                    // Shm
                    if cap_bytes.len() < 8 {
                        return None;
                    }
                    let segment_id = u64::from_le_bytes(cap_bytes[..8].try_into().ok()?);
                    backends.push(MemBackendCaps::Shm(ShmCaps { segment_id }));
                }
                _ => {
                    // Unknown backend — skip
                }
            }
        }
        Some(Self { backends })
    }
}

#[cfg(test)]
mod tests {
    use crate::{DowngradePath, NegotiationResult};

    use super::*;

    // ── MemInitExt wire format ────────────────────────────────────────────────

    #[test]
    fn mem_init_ext_empty_roundtrip() {
        let ext = MemInitExt::new(vec![]);
        let bytes = ext.to_bytes();
        // version byte + n_backends byte
        assert_eq!(bytes.len(), 2);
        let decoded = MemInitExt::from_bytes(&bytes).expect("decode failed");
        assert!(decoded.backends.is_empty());
    }

    #[test]
    fn mem_init_ext_shm_roundtrip() {
        let ext = MemInitExt::new(vec![MemBackendCaps::Shm(ShmCaps {
            segment_id: 0xDEAD_BEEF_1234_5678,
        })]);
        let bytes = ext.to_bytes();
        let decoded = MemInitExt::from_bytes(&bytes).expect("decode failed");
        assert_eq!(decoded.backends.len(), 1);
        match &decoded.backends[0] {
            MemBackendCaps::Shm(c) => assert_eq!(c.segment_id, 0xDEAD_BEEF_1234_5678),
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn mem_init_ext_cuda_roundtrip_no_devices() {
        let ext = MemInitExt::new(vec![MemBackendCaps::CudaIpc(CudaIpcCaps {
            device_ids: vec![],
        })]);
        let bytes = ext.to_bytes();
        let decoded = MemInitExt::from_bytes(&bytes).expect("decode failed");
        assert_eq!(decoded.backends.len(), 1);
        match &decoded.backends[0] {
            MemBackendCaps::CudaIpc(c) => assert!(c.device_ids.is_empty()),
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn mem_init_ext_cuda_roundtrip_multiple_devices() {
        let ext = MemInitExt::new(vec![MemBackendCaps::CudaIpc(CudaIpcCaps {
            device_ids: vec![0, 1, 2, -1],
        })]);
        let bytes = ext.to_bytes();
        let decoded = MemInitExt::from_bytes(&bytes).expect("decode failed");
        match &decoded.backends[0] {
            MemBackendCaps::CudaIpc(c) => assert_eq!(c.device_ids, [0, 1, 2, -1]),
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn mem_init_ext_multiple_backends_roundtrip() {
        let ext = MemInitExt::new(vec![
            MemBackendCaps::Shm(ShmCaps { segment_id: 42 }),
            MemBackendCaps::CudaIpc(CudaIpcCaps {
                device_ids: vec![0, 1],
            }),
        ]);
        let bytes = ext.to_bytes();
        let decoded = MemInitExt::from_bytes(&bytes).expect("decode failed");
        assert_eq!(decoded.backends.len(), 2);
        match &decoded.backends[0] {
            MemBackendCaps::Shm(c) => assert_eq!(c.segment_id, 42),
            other => panic!("expected Shm, got {other:?}"),
        }
        match &decoded.backends[1] {
            MemBackendCaps::CudaIpc(c) => assert_eq!(c.device_ids, [0, 1]),
            other => panic!("expected CudaIpc, got {other:?}"),
        }
    }

    #[test]
    fn mem_init_ext_rejects_bad_version() {
        let mut bytes = MemInitExt::new(vec![]).to_bytes();
        bytes[0] = 99; // corrupt version
        assert!(MemInitExt::from_bytes(&bytes).is_none());
    }

    #[test]
    fn mem_init_ext_rejects_truncated() {
        // Truncate after version+n_backends but before first backend
        let ext = MemInitExt::new(vec![MemBackendCaps::CudaIpc(CudaIpcCaps {
            device_ids: vec![0],
        })]);
        let bytes = ext.to_bytes();
        assert!(MemInitExt::from_bytes(&bytes[..3]).is_none()); // header only, no caps
    }

    #[test]
    fn mem_init_ext_skips_unknown_backend_id() {
        // Manually craft a payload with an unknown backend id (0x42)
        let bytes = vec![
            1u8,  // version
            2u8,  // n_backends = 2
            0x42, // unknown id
            3, 0, // len = 3
            0, 0, 0,   // 3 bytes of garbage
            1u8, // CudaIpc id
            1, 0,   // len = 1
            0u8, // n_devices = 0
        ];
        let decoded = MemInitExt::from_bytes(&bytes).expect("should succeed, skipping unknown");
        // Only the known CudaIpc backend should appear
        assert_eq!(decoded.backends.len(), 1);
        assert!(matches!(decoded.backends[0], MemBackendCaps::CudaIpc(_)));
    }

    // ── NegotiatedMemCaps ─────────────────────────────────────────────────────

    #[test]
    fn negotiated_mem_caps_set_and_get() {
        use zenoh_buffers::ZSliceKind;
        let mut caps = NegotiatedMemCaps::new();
        caps.set(ZSliceKind::ShmPtr, NegotiationResult::Native);
        let entry = caps.get(ZSliceKind::ShmPtr).expect("should be present");
        assert!(matches!(entry.result, NegotiationResult::Native));
        assert!(caps.get(ZSliceKind::Raw).is_none());
    }

    #[test]
    fn negotiated_mem_caps_fallback() {
        use zenoh_buffers::ZSliceKind;
        let mut caps = NegotiatedMemCaps::new();
        caps.set(
            ZSliceKind::ShmPtr,
            NegotiationResult::Fallback(DowngradePath::ToRaw),
        );
        let entry = caps.get(ZSliceKind::ShmPtr).expect("should be present");
        assert!(matches!(
            entry.result,
            NegotiationResult::Fallback(DowngradePath::ToRaw)
        ));
    }

    // ── MemPeerCaps ───────────────────────────────────────────────────────────

    #[test]
    fn mem_peer_caps_find() {
        let peer = MemPeerCaps::new(vec![
            MemBackendCaps::Shm(ShmCaps { segment_id: 1 }),
            MemBackendCaps::CudaIpc(CudaIpcCaps {
                device_ids: vec![0],
            }),
        ]);
        assert!(peer.find(MemBackendId::Shm).is_some());
        assert!(peer.find(MemBackendId::CudaIpc).is_some());
        assert!(peer.find(MemBackendId::Rdma).is_none());
    }
}
