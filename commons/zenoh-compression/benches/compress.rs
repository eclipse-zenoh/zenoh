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

use std::{fmt, fs, vec};

use criterion::{measurement::WallTime, BenchmarkGroup, BenchmarkId, Criterion};
use zenoh_buffers::writer::{DidntWrite, HasWriter};
use zenoh_codec::*;
use zenoh_compression::ZenohCompress;

/// Low entropy source file: a series of literary quotes repeated a number of times throughout
/// the file, the compression ratio should be low.
static LOW_ENTROPY_SOURCE_FILE: &str = "quotes.txt";

/// Middle entropy source file: a lorem ipsum sample file with random pseudo-latin words.
static MIDDLE_ENTROPY_SOURCE_FILE: &str = "lipsum.txt";

/// High entropy source file: a series of non repeated strings with random characters.
static HIGH_ENTROPY_SOURCE_FILE: &str = "random.txt";

#[derive(Clone, Copy, Debug)]
enum EntropyLevel {
    LOW,
    MIDDLE,
    HIGH,
}

impl EntropyLevel {
    fn get_source(&self) -> &str {
        match &self {
            EntropyLevel::LOW => LOW_ENTROPY_SOURCE_FILE,
            EntropyLevel::MIDDLE => MIDDLE_ENTROPY_SOURCE_FILE,
            EntropyLevel::HIGH => HIGH_ENTROPY_SOURCE_FILE,
        }
    }
}

impl fmt::Display for EntropyLevel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
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

fn load_test_batch_from_source_file(entropy: EntropyLevel, size: usize) -> Vec<u8> {
    let content = fs::read_to_string(entropy.get_source()).unwrap();
    let batch = &content[..size];
    batch.as_bytes().to_vec()
}

fn encode_simple(buff: &mut Vec<u8>, codec: &Zenoh060, batch: &Vec<u8>) {
    // Encoding with zenoh codec
    let mut writer = buff.writer();
    codec.write(&mut writer, batch.as_slice()).unwrap();
}

fn encode_with_compression(
    buff: &mut Vec<u8>,
    mut compression_buff: &mut Box<[u8]>,
    codec: &Zenoh060,
    zenoh_compress: &ZenohCompress,
    batch: &Vec<u8>,
) -> Result<usize, DidntWrite> {
    // Encoding with zenoh codec
    let mut writer = buff.writer();
    codec.write(&mut writer, batch.as_slice()).unwrap();

    // Compressing encoded output from buff into compression
    let mut compression: Vec<u8> = vec![];
    let compression_result = zenoh_compress.write(
        &mut compression.writer(),
        (batch.as_slice(), &mut compression_buff),
    );

    // Encode compression with zenoh codec
    buff.clear();
    let mut writer = buff.writer();
    codec.write(&mut writer, compression_buff.as_ref()).unwrap();
    compression_result
}

fn bench_encoding_with_compression(
    function_name: &str,
    group: &mut BenchmarkGroup<WallTime>,
    batch_size: &usize,
    codec: &Zenoh060,
    zenoh_compress: ZenohCompress,
    dummy_batch: &Vec<u8>,
) {
    group.bench_with_input(
        BenchmarkId::new(function_name, batch_size),
        batch_size,
        |b, _| {
            let mut buff = vec![];
            let mut compression_buff: Box<[u8]> = vec![0; usize::pow(2, 16)].into_boxed_slice();
            b.iter(|| {
                buff.clear();
                encode_with_compression(
                    &mut buff,
                    &mut compression_buff,
                    codec,
                    &zenoh_compress,
                    dummy_batch,
                )
            });
        },
    );
}

fn bench_simple_encoding(
    function_name: &str,
    group: &mut BenchmarkGroup<WallTime>,
    batch_size: &usize,
    codec: &Zenoh060,
    batch: &Vec<u8>,
) {
    group.bench_with_input(
        BenchmarkId::new(function_name, batch_size),
        batch_size,
        |b, _| {
            let mut buff = vec![];
            b.iter(|| {
                buff.clear();
                encode_simple(&mut buff, codec, batch);
            });
        },
    );
}

fn compression_bench(c: &mut Criterion) {
    let codec = Zenoh060::default();
    let zenoh_compress = ZenohCompress::default();
    let mut group = c.benchmark_group("Compression");
    for batch_size in BATCH_SIZES.into_iter() {
        for entropy in [EntropyLevel::LOW, EntropyLevel::MIDDLE, EntropyLevel::HIGH] {
            let batch = load_test_batch_from_source_file(entropy, *batch_size);
            group.throughput(criterion::Throughput::Bytes(*batch_size as u64));
            bench_simple_encoding(
                &format!("Simple encoding - {:?} entropy", entropy),
                &mut group,
                batch_size,
                &codec,
                &batch,
            );
            bench_encoding_with_compression(
                &format!("Compression - {:?} entropy", entropy),
                &mut group,
                batch_size,
                &codec,
                zenoh_compress,
                &batch,
            );
        }
    }
    group.finish();
}

// Dummy bench to generate a CSV file with custom stats, aiming to determine the amount of bytes
// we save with the compression algorithm compared to a normally encoded batch.
fn get_stats(_: &mut Criterion) {
    let codec = Zenoh060::default();
    let zenoh_compress = ZenohCompress::default();
    let metrics = std::fs::File::create("metrics.csv").unwrap();
    let mut writer = csv::Writer::from_writer(metrics);
    writer
        .write_record(&["size", "is_compressed", "entropy", "output_size"])
        .unwrap();
    for batch_size in BATCH_SIZES.into_iter() {
        for entropy in [EntropyLevel::LOW, EntropyLevel::MIDDLE, EntropyLevel::HIGH] {
            let batch = load_test_batch_from_source_file(entropy, *batch_size);
            let mut buffer: Vec<u8> = vec![];
            encode_simple(&mut buffer, &codec, &batch);
            writer
                .write_record(&[
                    batch_size.to_string(),
                    /*compression=*/ false.to_string(),
                    /*entropy=*/ entropy.to_string(),
                    /*final_batch_size=*/ buffer.len().to_string(),
                ])
                .unwrap();

            buffer.clear();
            let mut compression_buffer: Box<[u8]> = vec![0; usize::pow(2, 17)].into_boxed_slice();
            let compression_result = encode_with_compression(
                &mut buffer,
                &mut compression_buffer,
                &codec,
                &zenoh_compress,
                &batch,
            );
            writer
                .write_record(&[
                    batch_size.to_string(),
                    /*compression=*/ true.to_string(),
                    /*entropy=*/ entropy.to_string(),
                    /*final_batch_size=*/ compression_result.unwrap().to_string(),
                ])
                .unwrap();
        }
    }
    writer.flush().unwrap();
}

// Run benches with cargo bench --bench compress -- --plotting-backend gnuplot
// criterion_group!(benches, compression_bench);
criterion_group!(benches, compression_bench, get_stats);
criterion_main!(benches);
