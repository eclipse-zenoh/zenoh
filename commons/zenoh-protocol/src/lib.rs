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
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

pub mod common;
pub mod core;
pub mod network;
pub mod scouting;
pub mod transport;
pub mod zenoh;

// Zenoh version
pub const VERSION: u8 = 0x09;

// Zenoh protocol uses the following conventions for message definition and representation.
//
//
// # Single byte field
//
// A fixed size field of 8 bits.
//
// ```text
//  7 6 5 4 3 2 1 0
// +-+-+-+-+-+-+-+-+
// |      u8       |
// +---------------+
// ```
//
//
// # Variable length field
//
// The field size depends on the element definition and/or actual encoding. An example of variable
// length element is an array of bytes (e.g., a payload or a string).
//
// ```text
//  7 6 5 4 3 2 1 0
// +-+-+-+-+-+-+-+-+
// ~    element    ~
// +---------------+
// ```
//
//
// # u64 field
//
// A u64 is a specialized variable length field that is used to encode an unsigned integer.
//
// ```text
//  7 6 5 4 3 2 1 0
// +-+-+-+-+-+-+-+-+
// %     u64      %
// +---------------+
// ```
//
//
// # Array field
//
// An array contains a fixed number of elements whose number is known a priori or indicated by
// another field. Each element can be either a single byte field or a variable length field.
//
// ```text
//  7 6 5 4 3 2 1 0
// +-+-+-+-+-+-+-+-+
// ~   [element]   ~
// +---------------+
// ```
//
//
// # Vector field
//
// A vector contains a variable number of elements and is represented as follows:
//
// ```text
//  7 6 5 4 3 2 1 0
// +-+-+-+-+-+-+-+-+
// ~   <element>   ~
// +---------------+
// ```
//
// A vector field is always expanded as follows:
//
//  ```text
//  7 6 5 4 3 2 1 0
// +-+-+-+-+-+-+-+-+
// %      num      %
// +---------------+
// ~   [element]   ~
// +---------------+
// ```
//
