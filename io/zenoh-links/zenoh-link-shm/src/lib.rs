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

//! ⚠️ WARNING ⚠️
//!
//! This crate is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](../zenoh/index.html)

// this error is designed to explicitly forbid transport_shm feature for non-unix platforms, but our
// clippy with --all-targets flag triggers this on windows and I don't really know how to bypass this
// I think it would be easier to make windows implementation for transport_shm rather than fighting with this :)
// #[cfg(all(feature = "transport_shm", not(target_family = "unix")))]
// compile_error!("Feature transport_shm: platform not supported yet!");

#[cfg(all(feature = "transport_shm", unix))]
mod unix;
#[cfg(all(feature = "transport_shm", unix))]
pub use unix::*;
