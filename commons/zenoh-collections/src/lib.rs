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
pub mod single_or_vec;
pub use single_or_vec::*;

#[cfg(feature = "std")]
pub(crate) mod ring_buffer;
#[cfg(feature = "std")]
pub(crate) use ring_buffer::*;

#[cfg(feature = "std")]
pub(crate) mod stack_buffer;
#[cfg(feature = "std")]
pub(crate) use stack_buffer::*;

#[cfg(feature = "std")]
pub mod fifo_queue;
#[cfg(feature = "std")]
pub use fifo_queue::*;

#[cfg(feature = "std")]
pub mod lifo_queue;
#[cfg(feature = "std")]
pub use lifo_queue::*;

#[cfg(feature = "std")]
pub mod object_pool;
#[cfg(feature = "std")]
pub use object_pool::*;

#[cfg(feature = "std")]
pub mod mvar;
#[cfg(feature = "std")]
pub use mvar::*;
