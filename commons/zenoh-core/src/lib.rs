//
// Copyright (c) 2022 ZettaScale Technology
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
#![cfg_attr(not(features = "std"), no_std)]

#[cfg(feature = "std")]
mod std_only;
#[cfg(feature = "std")]
pub use std_only::*;

// Re-exports after moving ZError/ZResult to zenoh-result
pub use zenoh_result::{bail, to_zerror, zerror};
pub mod zresult {
    pub use zenoh_result::*;
}
pub use zresult::Error;
pub use zresult::ZResult as Result;

// +----------------+
// | `no_std` STUFF |
// +----------------+

#[cold]
pub const fn cold() {}
pub const fn likely(b: bool) -> bool {
    if !b {
        cold()
    }
    b
}
pub const fn unlikely(b: bool) -> bool {
    if b {
        cold()
    }
    b
}
