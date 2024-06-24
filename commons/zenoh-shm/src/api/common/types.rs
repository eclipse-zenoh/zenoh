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

/// Unique protocol identifier.
/// Here is a contract: it is up to user to make sure that incompatible ShmClient
/// and ShmProviderBackend implementations will never use the same ProtocolID
#[zenoh_macros::unstable_doc]
pub type ProtocolID = u32;

/// Unique segment identifier
#[zenoh_macros::unstable_doc]
pub type SegmentID = u32;

/// Chunk id within it's segment
#[zenoh_macros::unstable_doc]
pub type ChunkID = u32;
