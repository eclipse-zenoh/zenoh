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

use rand::SeedableRng;
use zenoh_buffers::{reader::HasReader, writer::HasWriter};
use zenoh_codec::{RCodec, WCodec, Zenoh060};
use zenoh_compression::ZenohCompress;
use zenoh_crypto::{BlockCipher, PseudoRng};

#[derive(Clone, Copy, Debug)]
enum EntropyLevel {
    LOW,
    HIGH,
}

static KB: usize = 1024;
static BATCH_SIZES: &'static [usize] = &[
    KB,
    2 * KB,
    4 * KB,
    8 * KB,
    16 * KB,
    24 * KB,
    32 * KB,
    40 * KB,
    48 * KB,
    56 * KB,
    64 * KB,
];

fn generate_test_batch(entropy: EntropyLevel, size: usize) -> Vec<u8> {
    let segment_32: Vec<u8> = Vec::from("AAAAAAAABBBBBBBBCCCCCCCCDDDDDDDD");
    let mut test_batch: Vec<u8> = vec![];
    for _i in 0..(size / segment_32.len()) {
        test_batch.append(&mut segment_32.clone());
    }
    match entropy {
        EntropyLevel::LOW => test_batch,
        EntropyLevel::HIGH => {
            let mut prng = PseudoRng::from_entropy();
            let key = [0_u8; BlockCipher::BLOCK_SIZE];
            let cipher = BlockCipher::new(key);
            let encrypted_batch = cipher.encrypt(test_batch, &mut prng);
            encrypted_batch
        }
    }
}

fn encode_with_compression(batch: &Vec<u8>) -> Vec<u8> {
    let codec = Zenoh060::default();
    let zenoh_compress = ZenohCompress::default();

    // Encoding with zenoh codec
    let mut buff: Vec<u8> = vec![];
    let mut writer = buff.writer();
    codec.write(&mut writer, batch.as_slice()).unwrap();

    // Compressing encoded output from buff into compression
    let mut compression: Vec<u8> = vec![];
    let mut compression_buff: Box<[u8]> = ZenohCompress::allocate_buffer();
    let compressed_bytes = zenoh_compress
        .write(
            &mut compression.writer(),
            (batch.as_slice(), &mut compression_buff),
        )
        .unwrap();

    // Encode compression with zenoh codec
    buff.clear();
    let mut writer = buff.writer();
    codec
        .write(&mut writer, &compression_buff[0..compressed_bytes])
        .unwrap();
    let ratio = buff.len() as f32 / batch.len() as f32;
    println!("Buff len: {} | Compression ratio: {:.3}", buff.len(), ratio);
    buff
}

fn decode_compression(compressed_buff: &Vec<u8>) -> Vec<u8> {
    let codec = Zenoh060::default();
    let zenoh_compress = ZenohCompress::default();
    let mut decompression_buff: Box<[u8]> = vec![0; usize::pow(2, 16)].into_boxed_slice();

    let buff: Vec<u8> = codec.read(&mut compressed_buff.reader()).unwrap();
    let result = zenoh_compress
        .read((&buff, &mut decompression_buff))
        .unwrap();
    Vec::from(&decompression_buff[0..result])
}

const COMPRESSION65: &[u8] = include_bytes!("64KbFile.txt");

#[test]
fn compression_tests() {
    for batch_size in BATCH_SIZES.iter() {
        let batch = generate_test_batch(EntropyLevel::HIGH, *batch_size);
        let compression = encode_with_compression(&batch);
        let decompression = decode_compression(&compression);
        assert_eq!(batch, decompression);
    }
}

#[test]
fn compression_test_from_file() {
    let compression = encode_with_compression(&COMPRESSION65.to_vec());
    let decompression = decode_compression(&compression);
    assert_eq!(COMPRESSION65, decompression);
}

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
