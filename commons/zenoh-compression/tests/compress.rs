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
/// Test for veryfying a random text is properly compressed and later decompressed using the
/// [ZenohCompress] codec.
fn zenohcompress_compression_test() {
    let input =  String::from("Condimentum lacinia quis vel eros. Elementum nibh tellus 
    molestie nunc non blandit. Tristique nulla aliquet enim tortor at auctor. Maecenas sed enim ut 
    sem viverra aliquet eget. Sit amet nisl suscipit adipiscing bibendum. Imperdiet sed euismod 
    nisi porta lorem. Fringilla ut morbi tincidunt augue interdum velit euismod in pellentesque.
    Rhoncus urna neque viverra justo nec ultrices dui. Sed viverra tellus in hac habitasse platea
    dictumst vestibulum rhoncus. Orci a scelerisque purus semper eget duis at. Magna etiam tempor
    orci eu lobortis elementum. Volutpat odio facilisis mauris sit amet. Dolor purus non enim 
    praesent elementum facilisis leo vel. Ante metus dictum at tempor commodo ullamcorper a. 
    Ornare massa eget egestas purus viverra accumsan in nisl. Est velit egestas dui id ornare arcu
    odio ut. Nunc vel risus commodo viverra. Convallis tellus id interdum velit laoreet id donec 
    ultrices tincidunt. Elementum pulvinar etiam non quam");

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
