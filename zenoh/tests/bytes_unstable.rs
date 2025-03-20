//
// Copyright (c) 2025 ZettaScale Technology
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
#![cfg(feature = "unstable")]
use zenoh::bytes::ZBytes;
use zenoh_buffers::{ZBuf, ZSlice};

#[test]
#[should_panic]
fn test_zbytes_chunks_with_size_zero() {
    let zbytes = ZBytes::from((0..4).collect::<Vec<_>>());
    let _chunks = zbytes.chunks(0);
}

#[test]
fn test_empty_zbytes_chunks() {
    let zbytes = ZBytes::new();

    assert_eq!(zbytes.chunks(3).next(), None);
}

#[test]
fn test_zbytes_chunks_non_exact_division() {
    let zbytes = ZBytes::from((0..10).collect::<Vec<_>>());

    const EXPECTED_CHUNKS: &[&[u8]] = &[&[0, 1, 2], &[3, 4, 5], &[6, 7, 8], &[9]];
    const CHUNK_SIZE: usize = 3;

    assert!(check_zbytes_chunks(zbytes, CHUNK_SIZE, EXPECTED_CHUNKS))
}

#[test]
fn test_zbytes_chunks_exact_division() {
    let zbytes = ZBytes::from((0..12).collect::<Vec<_>>());

    const EXPECTED_CHUNKS: &[&[u8]] = &[&[0, 1, 2], &[3, 4, 5], &[6, 7, 8], &[9, 10, 11]];
    const CHUNK_SIZE: usize = 3;

    assert!(check_zbytes_chunks(zbytes, CHUNK_SIZE, EXPECTED_CHUNKS))
}

#[test]
fn test_zbytes_chunks_with_large_chunk_size() {
    let zbytes = ZBytes::from((0..5).collect::<Vec<_>>());

    const EXPECTED_CHUNKS: &[&[u8]] = &[&[0, 1, 2, 3, 4]];
    const CHUNK_SIZE: usize = 10;

    assert!(check_zbytes_chunks(zbytes, CHUNK_SIZE, EXPECTED_CHUNKS))
}

#[test]
fn test_zbytes_chunks_with_single_chunk() {
    let zbytes = ZBytes::from((0..7).collect::<Vec<_>>());

    const EXPECTED_CHUNKS: &[&[u8]] = &[&[0, 1, 2, 3, 4, 5, 6]];
    const CHUNK_SIZE: usize = 7;

    assert!(check_zbytes_chunks(zbytes, CHUNK_SIZE, EXPECTED_CHUNKS))
}

#[test]
fn test_zbytes_chunks_with_size_one() {
    let zbytes = ZBytes::from((0..4).collect::<Vec<_>>());

    const EXPECTED_CHUNKS: &[&[u8]] = &[&[0], &[1], &[2], &[3]];
    const CHUNK_SIZE: usize = 1;

    assert!(check_zbytes_chunks(zbytes, CHUNK_SIZE, EXPECTED_CHUNKS))
}

#[test]
fn test_non_contiguous_zbytes_chunks() {
    let data1 = (0..10).collect::<Vec<u8>>();
    let data2 = (10..20).collect::<Vec<u8>>();
    let mut zbuf = ZBuf::empty();
    zbuf.push_zslice(ZSlice::from(data1));
    zbuf.push_zslice(ZSlice::from(data2));
    let zbytes = ZBytes::from(zbuf);

    const EXPECTED_CHUNKS: &[&[u8]] = &[
        &[0, 1, 2, 3],
        &[4, 5, 6, 7],
        &[8, 9, 10, 11],
        &[12, 13, 14, 15],
        &[16, 17, 18, 19],
    ];
    const CHUNK_SIZE: usize = 4;

    assert!(check_zbytes_chunks(zbytes, CHUNK_SIZE, EXPECTED_CHUNKS))
}

fn check_zbytes_chunks(zbytes: ZBytes, chunk_size: usize, expected_chunks: &[&[u8]]) -> bool {
    for (idx, (found, expected)) in zbytes.chunks(chunk_size).zip(expected_chunks).enumerate() {
        if found.to_bytes().as_ref() != *expected {
            return false;
        }

        if idx == expected_chunks.len() - 1 {
            if found.len() > chunk_size {
                return false;
            }
        } else if found.len() != chunk_size {
            return false;
        }
    }

    true
}
