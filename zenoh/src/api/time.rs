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
use std::time::{SystemTime, UNIX_EPOCH};
use zenoh_protocol::core::{Timestamp, TimestampId};

/// Generates a [`Timestamp`] with [`TimestampId`] and current system time
/// The [`TimestampId`] can be taken from session id returned by [`SessionInfo::zid()`](crate::api::info::SessionInfo::zid).
pub fn new_timestamp<T: Into<TimestampId>>(id: T) -> Timestamp {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().into();
    Timestamp::new(now, id.into())
}
