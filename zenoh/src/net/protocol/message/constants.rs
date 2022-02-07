//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use super::core::ZInt;
use crate::net::protocol::core::SeqNumBytes;

pub mod defaults {
    use super::SeqNumBytes;

    // The default sequence number resolution takes 4 bytes on the wire.
    // Given the VLE encoding of ZInt, 4 bytes result in 28 useful bits.
    // 2^28 = 268_435_456 => Max Seq Num = 268_435_455
    pub const SEQ_NUM_RES: SeqNumBytes = SeqNumBytes::Four;

    /// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
    ///       in bytes of the message, resulting in the maximum length of a message being 65_535 bytes.
    ///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
    ///       the boundary of the serialized messages. The length is encoded as little-endian.
    ///       In any case, the length of a message must not exceed 65_535 bytes.
    pub const BATCH_SIZE: u16 = u16::MAX;
}

pub mod data_kind {
    use super::ZInt;

    pub const PUT: ZInt = 0;
    pub const PATCH: ZInt = 1;
    pub const DELETE: ZInt = 2;

    pub const DEFAULT: ZInt = PUT;

    pub fn to_string(i: ZInt) -> String {
        match i {
            0 => "PUT".to_string(),
            1 => "PATCH".to_string(),
            2 => "DELETE".to_string(),
            i => i.to_string(),
        }
    }
}
