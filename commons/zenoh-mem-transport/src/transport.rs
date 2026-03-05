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

use std::sync::Arc;

use zenoh_buffers::{ZSlice, ZSliceKind};
use zenoh_result::ZResult;

use crate::caps::{MemBackendCaps, MemBackendId};

/// Object-safe write interface: write raw bytes into a buffer.
pub trait HandleWriter: Send {
    fn write_bytes(&mut self, data: &[u8]) -> ZResult<()>;
}

/// Object-safe read interface: read raw bytes from a buffer.
pub trait HandleReader: Send {
    fn read_bytes(&mut self, buf: &mut [u8]) -> ZResult<()>;
}

/// How to downgrade a ZSlice when the peer cannot receive it natively.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DowngradePath {
    /// Copy into a SHM region the peer can map (peer supports SHM but not this backend).
    ToShm,
    /// Serialize to raw bytes — full CPU copy, peer has no zero-copy support.
    ToRaw,
}

/// Outcome of capability negotiation for a single backend/ZSliceKind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NegotiationResult {
    /// Both peers support this backend natively — send the handle as-is.
    Native,
    /// Peer cannot receive this backend. Use the given [`DowngradePath`] on TX.
    Fallback(DowngradePath),
    /// Backends are mutually incompatible and no safe fallback is available.
    /// The ZSlice will be dropped rather than sent.
    Reject,
}

/// A pluggable zero-copy memory transport backend.
///
/// Implementors: [`ShmBackend`](crate::backends::shm::ShmBackend),
/// [`CudaIpcBackend`](crate::backends::cuda::CudaIpcBackend).
///
/// # Contract
///
/// - [`owned_kinds`](ZeroMemTransport::owned_kinds) must return a non-empty slice.
///   Every `ZSliceKind` must be owned by at most one registered backend.
/// - [`write_handle`] and [`read_handle`] are called symmetrically: whatever
///   bytes `write_handle` emits (after the kind byte written by the codec),
///   `read_handle` must consume exactly.
/// - [`downgrade`] may block (e.g. `cudaMemcpy`) but must not panic.
pub trait ZeroMemTransport: Send + Sync + 'static {
    /// The [`ZSliceKind`] discriminant(s) this backend owns.
    fn owned_kinds(&self) -> &[ZSliceKind];

    /// The backend identifier used in `ext_mem` capability advertisements.
    fn backend_id(&self) -> MemBackendId;

    /// Capability data sent in our `InitSyn` / `InitAck`.
    fn local_caps(&self) -> MemBackendCaps;

    /// Determine whether this backend can be used with the given peer capability.
    ///
    /// Called once per transport connection after handshake, per owned kind.
    fn negotiate(&self, peer: Option<&MemBackendCaps>) -> NegotiationResult;

    /// TX: encode the handle portion of a ZSlice this backend owns.
    ///
    /// The kind byte has already been written by the caller; this method writes
    /// only the backend-specific payload (e.g. IPC handle + len + device_id).
    fn write_handle(&self, zs: &ZSlice, writer: &mut dyn HandleWriter) -> ZResult<()>;

    /// RX: decode a handle from wire and produce a ZSlice.
    ///
    /// `kind` identifies which of `owned_kinds` was on the wire (some backends
    /// own multiple kinds with different metadata, e.g. `CudaPtr` vs `CudaTensor`).
    fn read_handle(&self, reader: &mut dyn HandleReader, kind: ZSliceKind) -> ZResult<ZSlice>;

    /// TX fallback: convert a ZSlice owned by this backend to one the peer *can* receive.
    ///
    /// Returns `None` if downgrade is impossible (e.g. pure device memory and no
    /// CUDA runtime available for staging copy).
    fn downgrade(&self, zs: &ZSlice, path: DowngradePath) -> Option<ZSlice>;
}

/// Type-erased backend, reference-counted for sharing across the transport.
pub type ArcBackend = Arc<dyn ZeroMemTransport>;
