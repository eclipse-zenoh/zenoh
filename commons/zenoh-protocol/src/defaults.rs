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
use super::core::ZInt;

// Zenoh version
//  7 6 5 4 3 2 1 0
// +-+-+-+-+-+-+-+-+
// | v_maj | v_min |
// +-------+-------+
pub const VERSION: u8 = 0x07;

// The default sequence number resolution takes 4 bytes on the wire.
// Given the VLE encoding of ZInt, 4 bytes result in 28 useful bits.
// 2^28 = 268_435_456 => Max Seq Num = 268_435_455
pub const SEQ_NUM_RES: ZInt = 268_435_456;

/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65_535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65_535 bytes.
pub const BATCH_SIZE: u16 = u16::MAX;
