//
// Copyright (c) 2024 ZettaScale Technology
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

//! This module exposes the [ReplicationService] structure needed by the storage manager to
//! replicate the content of storage across a Zenoh network.
//!
//! This structure, and thus the replication, only works for storage that have the [History::Latest]
//! capability.
//!
//! From a high-level, the replication works by generating a concise view of the state of the
//! storage at regular time intervals. To do so, the time is divided in 'intervals' (themselves
//! divided into 'sub-intervals') and each publication mapped to one such group. Then a fingerprint
//! of each group is computed and these fingerprints are sent over the network to other storage for
//! comparison.
//!
//! [History::Latest]: zenoh_backend_traits::History::Latest

mod classification;
mod configuration;
mod core;
mod digest;
mod log;
mod service;

pub(crate) use log::{Action, Event, LogLatest, LogLatestKey};
pub(crate) use service::ReplicationService;
