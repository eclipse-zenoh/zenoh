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

use std::vec;

use zenoh_buffers::{reader::HasReader, writer::HasWriter};
use zenoh_codec::{RCodec, WCodec, Zenoh060};
use zenoh_compression::ZenohCompress;

#[test]
fn compress() {
    let input =  String::from("Heeeeeeeeeeeeeeeellllloollololoooooooooooooooooollooooooollllooooooooooooooooolllllllllooollllllloooooollllllllllloooooo Peoooooooopleeeeeeeee");

    // Encode with Zenoh060 codec
    let codec = Zenoh060::default();
    let mut buff: Vec<u8> = vec![];
    let mut writer = buff.writer();
    codec.write(&mut writer, &input).unwrap();

    // Compress with ZenohCompress
    let mut compression_buff: Box<[u8]> = vec![0; usize::pow(2, 16)].into_boxed_slice();
    let zenoh_compress = ZenohCompress;

    let mut compression: Vec<u8> = vec![];
    zenoh_compress
        .write(&mut compression.writer(), (&buff, &mut compression_buff))
        .unwrap();

    // Decompress with ZenohCompress
    let mut receiving_buff: Box<[u8]> = vec![0; usize::pow(2, 16)].into_boxed_slice();
    zenoh_compress
        .read((&compression, &mut receiving_buff))
        .unwrap();

    // Decode with Zenoh060
    let reconstitution: String = codec.read(&mut receiving_buff.reader()).unwrap();

    assert_eq!(input, reconstitution);
}
