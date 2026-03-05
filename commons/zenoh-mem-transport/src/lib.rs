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
//! During the Zenoh handshake (`InitSyn`/`InitAck`/`OpenSyn`/`OpenAck`) each
//! peer advertises a [`MemPeerCaps`] listing which backends it supports.  After
//! the handshake the transport stores a [`NegotiatedMemCaps`] that records, for
//! each [`ZSliceKind`], whether to use the backend natively or to fall back to
//! a cheaper representation the peer can receive.
//!
//! # Adding a new backend
//!
//! 1. Implement [`ZeroMemTransport`] in a new crate (e.g. `zenoh-rdma`).
//! 2. Add a `MemBackendId` variant (protocol version bump).
//! 3. Register with [`MemRegistry::register`] at session startup.
//! 4. No changes to `map_to_partner`, codec dispatch, or the handshake FSM.

pub mod backends;
pub mod caps;
pub mod registry;
pub mod transport;

pub use caps::{MemBackendCaps, MemBackendId, MemInitExt, MemPeerCaps, NegotiatedMemCaps};
pub use registry::MemRegistry;
pub use transport::{DowngradePath, NegotiationResult, ZeroMemTransport};
