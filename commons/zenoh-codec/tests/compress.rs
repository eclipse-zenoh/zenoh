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

use zenoh_buffers::{reader::HasReader, writer::HasWriter, BBuf};
use zenoh_codec::{Compress, Decompress, RCodec, WCodec, Zenoh060, ZenohCompress};

#[test]
fn compress_bbuf() {
    let input: &[u8] = b"Heeeeeeeeeeeeeeeellllloollololoooooooooooooooooooooooollllooooooooooooooooolllllllllooollllllloooooollllllllllloooooo Peoooooooopleeeeeeeee";

    println!("{}", input.len());
    // panic!();
    let mut input_bbuf = BBuf::with_capacity(u16::MAX as usize);
    let codec = Zenoh060::default();
    let mut writer = input_bbuf.writer();
    codec.write(&mut writer, input).unwrap();

    let zenoh_compress = ZenohCompress;
    let compressed_bbuf = zenoh_compress.compress(&input_bbuf).unwrap();
    let decompressed_bbuf = zenoh_compress.decompress(&compressed_bbuf).unwrap();

    let mut reader = decompressed_bbuf.reader();
    let result: Vec<u8> = codec.read(&mut reader).unwrap();
    assert_eq!(input, result.as_slice());
}
