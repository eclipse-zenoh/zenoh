//
// Copyright (c) 2025 ZettaScale Technology
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
//! This module is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)

pub(crate) mod batch_arena;
pub(crate) mod page_arena;
pub mod reader;
pub mod writer;

pub const BUF_SIZE: usize = 65537;
//pub const BUF_COUNT: usize = 1 * 1024 * 1024 / BUF_SIZE;

//const BUF_SIZE: usize = 16;
pub const BUF_COUNT: usize = 4;
