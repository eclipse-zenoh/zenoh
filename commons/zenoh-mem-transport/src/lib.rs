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

//! Pluggable zero-copy memory transport backends for Zenoh.
//!
//! # Overview
//!
//! SHM, CUDA IPC, RDMA, and UCX all follow the same pattern:
//!
//! ```text
//! Publisher:  allocate in shared space  →  get a handle (small, opaque bytes)
//! Wire:       send the handle only      →  no bulk data transfer
//! Subscriber: open the handle           →  zero-copy view of the same memory
//! ```
//!
//! This crate defines [`ZeroMemTransport`], the common trait for all backends,
//! and [`MemRegistry`], the runtime registry that the transport layer uses to
//! dispatch encoding/decoding and fallback conversions.
//!
//! # Capability negotiation
//!
//! During the Zenoh handshake (`InitSyn`/`InitAck`) each peer advertises which
//! backends it supports via the `ext_mem` extension (extension ID `0x8`).
//!
//! - SHM capability is already conveyed by the existing `ext_shm` (ID `0x2`)
//!   and its challenge-response mechanism; `ext_mem` carries additional backends
//!   (CUDA IPC, RDMA, …).
//! - After the handshake the transport calls [`MemRegistry::negotiate`] with the
//!   peer's [`MemPeerCaps`] to produce a [`NegotiatedMemCaps`].
//! - Every TX message consults `NegotiatedMemCaps` per [`ZSliceKind`] to decide
//!   whether to send natively or downgrade (GPU→host staging copy, etc.).
//!
//! ## `ext_mem` wire format
//!
//! The `ext_mem` bytes (carried inside `ZExtZBuf(0x8)`) are:
//!
//! ```text
//! version     : u8          — always 1
//! n_backends  : u8
//! for each backend:
//!     id      : u8          — MemBackendId discriminant
//!     len     : u16 (LE)    — byte length of the following caps
//!     caps    : bytes[len]
//!
//! ShmCaps    = { segment_id : u64 (LE) }
//! CudaIpcCaps = { n_devices : u8 ; device_ids : i32[n_devices] (LE) }
//! ```
//!
//! Unknown `id` values are skipped (forward-compatible).  A length mismatch or
//! version > 1 causes the entire extension to be ignored (no connection drop).
//!
//! # Adding a new backend
//!
//! 1. Implement [`ZeroMemTransport`] in a new crate (e.g. `zenoh-rdma`).
//! 2. Add a [`MemBackendId`] variant (requires a protocol version bump).
//! 3. Register with [`MemRegistry::register`] at session startup.
//! 4. No changes to `map_to_partner`, codec dispatch, or the handshake FSM.
//!
//! # Examples
//!
//! See the `registry_negotiate` example in the `examples/` directory for a
//! worked demonstration of registry setup, capability serialization, and
//! negotiation outcome querying.

pub mod backends;
pub mod caps;
pub mod registry;
pub mod transport;

pub use caps::{MemBackendCaps, MemBackendId, MemInitExt, MemPeerCaps, NegotiatedMemCaps};
pub use registry::MemRegistry;
pub use transport::{DowngradePath, NegotiationResult, ZeroMemTransport};
