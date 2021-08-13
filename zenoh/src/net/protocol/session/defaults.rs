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
use super::core::{whatami, WhatAmI, ZInt};

// Zenoh version
//  7 6 5 4 3 2 1 0
// +-+-+-+-+-+-+-+-+
// | v_maj | v_min |
// +-------+-------+
pub const ZN_VERSION: u8 = 0x05;

// Zenoh default whatami
pub const ZN_DEFAULT_WHATAMI: WhatAmI = whatami::PEER;

// The default sequence number resolution takes 4 bytes on the wire.
// Given the VLE encoding of ZInt, 4 bytes result in 28 useful bits.
// 2^28 = 268_435_456 => Max Seq Num = 268_435_455
pub const ZN_DEFAULT_SEQ_NUM_RESOLUTION: ZInt = 268_435_456;

// The default batch size in bytes for the transport
// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
//       in bytes of the message, resulting in the maximum length of a message being 65_535 bytes.
//       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
//       the boundary of the serialized messages. The length is encoded as little-endian.
//       In any case, the length of a message must not exceed 65_535 bytes.
//
// This results in a maximum batch size of (2 + 65_535) 65_537 bytes. In case a transport link has
// an MTU smaller than the ZN_BATCH_SIZE, the batch size will be automatically set to the
// transport link MTU to avoid any transmission problems on the network.
pub const ZN_DEFAULT_BATCH_SIZE: usize = 65_537;

zconfigurable! {
    // Default link lease in milliseconds: 10 seconds
    pub static ref ZN_LINK_LEASE: ZInt = 10_000;

    // Default interval for keep alive messages in milliseconds: 2.5 seconds
    // NOTE: In order to consider eventual packet loss and transmission latency and jitter,
    //       set the actual keep_alive timeout to one fourth of the agreed session lease.
    //       This is in-line with the ITU-T G.8013/Y.1731 specification on continous connectivity
    //       check which considers a link as failed when no messages are received in 3.5 times the
    //       target interval. For simplicity, we compute the keep_alive interval as 1/4 of the
    //       session lease.
    pub static ref ZN_LINK_KEEP_ALIVE: ZInt = 2_500;

    // Default timeout when opening a session in milliseconds
    pub static ref ZN_OPEN_TIMEOUT: ZInt = 10_000;

    // Default maximum number of pending sessions being opened with the host
    pub static ref ZN_OPEN_INCOMING_PENDING: usize = 1_024;

    // Parameters of the link transmission queue
    // - The size of each queue relates to the number of batches a given queue can contain.
    // - The amount of memory being allocated for each queue is then QUEUE_SIZE_XXX * ZN_BATCH_SIZE.
    //   In case the transport link MTU is smaller than the ZN_BATCH_SIZE, then amount of memory being
    //   allocated for each queue is QUEUE_SIZE_XXX * LINK_MTU.
    pub static ref ZN_QUEUE_SIZE_CONTROL: usize = 1;
    pub static ref ZN_QUEUE_SIZE_REAL_TIME: usize = 4;
    pub static ref ZN_QUEUE_SIZE_INTERACTIVE_HIGH: usize = 4;
    pub static ref ZN_QUEUE_SIZE_INTERACTIVE_LOW: usize = 4;
    pub static ref ZN_QUEUE_SIZE_DATA_HIGH: usize = 4;
    pub static ref ZN_QUEUE_SIZE_DATA: usize = 4;
    pub static ref ZN_QUEUE_SIZE_DATA_LOW: usize = 4;
    pub static ref ZN_QUEUE_SIZE_BACKGROUND: usize = 4;

    // The default backoff time in nanoseconds to allow the batching to potentially progress
    pub static ref ZN_QUEUE_PULL_BACKOFF: u64 = 100;

    // The total size of buffers allocated at RX side per link. Default 16MB.
    pub static ref ZN_RX_BUFF_SIZE: usize = 16_777_216;
}
