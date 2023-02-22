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
#[macro_use]
extern crate criterion;

use criterion::{BenchmarkId, Criterion};
use rand::SeedableRng;
use zenoh_buffers::writer::HasWriter;
use zenoh_codec::*;
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

fn generate_dummy_batch(entropy: EntropyLevel, size: usize) -> Vec<u8> {
    let mut segment_32: Vec<u8> = Vec::from("AAAAAAAABBBBBBBBCCCCCCCCDDDDDDDD");
    let mut dummy_batch: Vec<u8> = vec![];
    for _i in 0..(size / segment_32.len()) {
        dummy_batch.append(&mut segment_32);
    }
    match entropy {
        EntropyLevel::LOW => dummy_batch,
        EntropyLevel::HIGH => {
            let mut prng = PseudoRng::from_entropy();
            let key = [0_u8; BlockCipher::BLOCK_SIZE];
            let cipher = BlockCipher::new(key);
            let encrypted_batch = cipher.encrypt(dummy_batch, &mut prng);
            encrypted_batch
        }
    }
}

fn bench_simple_encoding(buff: &mut Vec<u8>, codec: &Zenoh060, batch: &Vec<u8>) {
    buff.clear();

    // Encoding with zenoh codec
    let mut writer = buff.writer();
    codec.write(&mut writer, batch.as_slice()).unwrap();
}

fn bench_compression(
    buff: &mut Vec<u8>,
    mut compression_buff: &mut Box<[u8]>,
    codec: &Zenoh060,
    zenoh_compress: &ZenohCompress,
    batch: &Vec<u8>,
) {
    buff.clear();

    // Encoding with zenoh codec
    let mut writer = buff.writer();
    codec.write(&mut writer, batch.as_slice()).unwrap();

    // Compressing encoded output from buff into compression
    let mut compression: Vec<u8> = vec![];
    _ = zenoh_compress.write(
        &mut compression.writer(),
        (batch.as_slice(), &mut compression_buff),
    );

    // Encode compression with zenoh codec
    buff.clear();
    let mut writer = buff.writer();
    codec.write(&mut writer, compression_buff.as_ref()).unwrap();
}

fn low_entropy_simple_encoding(c: &mut Criterion) {
    let mut buff = vec![];

    let codec = Zenoh060::default();
    let entropy = EntropyLevel::LOW;
    let mut group = c.benchmark_group("Low entropy simple encoding");
    for batch_size in BATCH_SIZES.into_iter() {
        let dummy_batch = generate_dummy_batch(entropy, *batch_size);
        group.throughput(criterion::Throughput::Bytes(*batch_size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            &batch_size,
            |b, _| {
                b.iter(|| bench_simple_encoding(&mut buff, &codec, &dummy_batch));
            },
        );
    }
    group.finish();
}

fn low_entropy_encoding_with_compression(c: &mut Criterion) {
    let mut buff = vec![];
    let mut compression_buff: Box<[u8]> = vec![0; usize::pow(2, 16)].into_boxed_slice();

    let codec = Zenoh060::default();
    let zenoh_compress = ZenohCompress::default();
    let entropy = EntropyLevel::LOW;
    let mut group = c.benchmark_group("Low entropy encoding with compression.");

    for batch_size in BATCH_SIZES.into_iter() {
        let dummy_batch = generate_dummy_batch(entropy, *batch_size);
        group.throughput(criterion::Throughput::Bytes(*batch_size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            &batch_size,
            |b, _| {
                b.iter(|| {
                    bench_compression(
                        &mut buff,
                        &mut compression_buff,
                        &codec,
                        &zenoh_compress,
                        &dummy_batch,
                    )
                });
            },
        );
    }
    group.finish();
}

fn high_entropy_simple_encoding(c: &mut Criterion) {
    let mut buff = vec![];
    let codec = Zenoh060::default();
    let entropy = EntropyLevel::HIGH;
    let mut group = c.benchmark_group("High entropy simple encoding");

    for batch_size in BATCH_SIZES.into_iter() {
        let dummy_batch = generate_dummy_batch(entropy, *batch_size);
        group.throughput(criterion::Throughput::Bytes(*batch_size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            &batch_size,
            |b, _| {
                b.iter(|| bench_simple_encoding(&mut buff, &codec, &dummy_batch));
            },
        );
    }
    group.finish();
}

fn high_entropy_encoding_with_compression(c: &mut Criterion) {
    let mut buff = vec![];
    let mut compression_buff: Box<[u8]> = vec![0; usize::pow(2, 16)].into_boxed_slice();
    let codec = Zenoh060::default();
    let zenoh_compress = ZenohCompress::default();
    let entropy = EntropyLevel::HIGH;
    let mut group = c.benchmark_group("High entropy encoding with compression.");
    for batch_size in BATCH_SIZES.into_iter() {
        let dummy_batch = generate_dummy_batch(entropy, *batch_size);
        group.throughput(criterion::Throughput::Bytes(*batch_size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            &batch_size,
            |b, _| {
                b.iter(|| {
                    bench_compression(
                        &mut buff,
                        &mut compression_buff,
                        &codec,
                        &zenoh_compress,
                        &dummy_batch,
                    )
                });
            },
        );
    }
    group.finish();
}

// - Cuantos batches podemos comprimir por segundo
// - Cuantos bytes enviados ganamos en comparacion con enviar sin comprimir
// - Entropia (ver block cipher)

// Run benches with cargo bench --bench compress -- --plotting-backend gnuplot
criterion_group!(
    benches,
    low_entropy_simple_encoding,
    low_entropy_encoding_with_compression,
    high_entropy_simple_encoding,
    high_entropy_encoding_with_compression
);
criterion_main!(benches);
