//
// Copyright (c) 2026 ZettaScale Technology
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
use zenoh_buffers::{reader::HasReader, writer::HasWriter};
use zenoh_codec::{RCodec, WCodec, Zenoh080, Zenoh080Bounded};
use zenoh_protocol::{
    core::{Locator, WhatAmI, ZenohIdProto},
    scouting::{HelloProto, ScoutingMessage},
};

fn fixed_zid() -> ZenohIdProto {
    ZenohIdProto::try_from([0x14, 0x20, 0x30, 0xff].as_slice()).expect("fixed zid must be valid")
}

#[test]
fn bounded_string_rejects_declared_length_larger_than_input() {
    // This is a length-prefixed string with a declared length of 2, but only 1
    // byte follows. The bounded byte/string decoder must reject the truncated
    // payload instead of trusting the declared length for allocation.
    let bytes = [2, b'a'];

    let codec = Zenoh080Bounded::<usize>::new();
    let mut reader = bytes.as_slice().reader();
    let decoded: Result<String, _> = codec.read(&mut reader);
    assert!(decoded.is_err());
}

#[test]
fn hello_rejects_locator_count_larger_than_remaining_input() {
    // Start from a valid HELLO with one locator, then corrupt only the
    // locator-count prefix into an absurdly large varint. The decoder should
    // fail cleanly once it runs out of bytes for the declared locator list,
    // without preallocating from that attacker-controlled count.
    let hello = HelloProto {
        version: 1,
        whatami: WhatAmI::Client,
        zid: fixed_zid(),
        locators: vec![Locator::new("tcp", "127.0.0.1:7447", "").expect("locator must be valid")],
    };

    let codec = Zenoh080::new();
    let mut bytes = vec![];
    let mut writer = bytes.writer();
    codec
        .write(&mut writer, &ScoutingMessage::from(hello))
        .unwrap();

    // HELLO layout here is:
    //   [0]    header
    //   [1]    version
    //   [2]    flags
    //   [3..7] zid (4 bytes)
    //   [7]    locator_count
    //   [8]    locator_len
    //   [9..]  locator_bytes
    // Replace the valid one-byte locator_count with a huge varint and leave the
    // remaining payload unchanged, so the declared count is impossible.
    bytes.splice(7..8, [255, 255, 255, 255, 255, 255, 255, 255, 0]);

    let mut reader = bytes.as_slice().reader();
    let decoded: Result<ScoutingMessage, _> = codec.read(&mut reader);
    assert!(decoded.is_err());
}
