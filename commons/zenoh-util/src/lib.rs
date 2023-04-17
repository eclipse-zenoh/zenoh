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
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;
#[cfg_attr(feature = "std", macro_use)]
extern crate lazy_static;

#[deprecated = "This module is now a separate crate. Use the `zenoh_core` crate directly for shorter compile-times. You may disable this re-export by disabling `zenoh-util`'s default features."]
pub use zenoh_core as core;

#[cfg(feature = "std")]
mod std_only;

#[cfg(feature = "std")]
pub use std_only::*;
