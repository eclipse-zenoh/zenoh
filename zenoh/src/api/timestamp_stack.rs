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
use std::sync::Arc;

use zenoh_protocol::core::WhatAmI;

use crate::session::ZenohId;

/// Context passed to the user-defined timestamp callback.
///
/// This struct provides the callback with information about the current Zenoh node
/// and the interception point at which the timestamp is being generated.
///
/// The struct is `#[non_exhaustive]` to allow adding new fields in the future.
#[non_exhaustive]
#[zenoh_macros::unstable]
pub struct TsStackContext {
    /// The Zenoh ID of the current node.
    pub zid: ZenohId,
    /// The mode of the current node (router, peer, or client).
    pub whatami: WhatAmI,
    // TODO: should be a non-exhaustive enum
    /// The interception point identifier (e.g., `zenoh_protocol::network::timestamp_stack::interception_point::SEND`).
    pub interception_point: u8,
}

/// Type alias for the user-defined timestamp callback.
///
/// The callback receives a [`TsStackContext`] and returns the raw timestamp bytes
/// to be pushed onto the timestamp stack.
#[zenoh_macros::unstable]
pub type GetTimestampCallback = Arc<dyn Fn(TsStackContext) -> Vec<u8> + Send + Sync>;
