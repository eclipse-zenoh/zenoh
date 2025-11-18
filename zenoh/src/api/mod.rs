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

pub(crate) type Id = u32;

pub(crate) mod admin;
pub(crate) mod builders;
pub(crate) mod bytes;
#[cfg(feature = "unstable")]
pub(crate) mod cancellation;
pub(crate) mod config;
pub(crate) mod encoding;
pub(crate) mod handlers;
pub(crate) mod info;
pub(crate) mod key_expr;
pub(crate) mod liveliness;
#[cfg(feature = "plugins")]
pub(crate) mod loader;
pub(crate) mod matching;
#[cfg(feature = "plugins")]
pub(crate) mod plugins;
pub(crate) mod publisher;
pub(crate) mod querier;
pub(crate) mod query;
pub(crate) mod queryable;
pub(crate) mod sample;
pub(crate) mod scouting;
pub(crate) mod selector;
pub(crate) mod session;
pub(crate) mod subscriber;
