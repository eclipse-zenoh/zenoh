//
// Copyright (c) 2023 ZettaScale Technology
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
#![cfg(feature = "test")]
use std::{fmt::Display, slice};

use zenoh_shm::posix_shm::segment::Segment;

pub mod common;
use common::{validate_memory, TEST_SEGMENT_PREFIX};

fn validate_segment<ID>(
    created_segment: &Segment<ID>,
    opened_segment: &Segment<ID>,
    expected_elem_count: usize,
) where
    rand::distributions::Standard: rand::distributions::Distribution<ID>,
    ID: Clone + Display,
{
    assert!(created_segment.len() == expected_elem_count);
    assert!(opened_segment.len() >= expected_elem_count);

    let ptr1 = created_segment.as_ptr();
    let ptr2 = opened_segment.as_ptr();

    let slice1 = unsafe { slice::from_raw_parts_mut(ptr1, expected_elem_count) };
    let slice2 = unsafe { slice::from_raw_parts(ptr2, expected_elem_count) };

    validate_memory(slice1, slice2);
}

fn test_segment<ID>()
where
    rand::distributions::Standard: rand::distributions::Distribution<ID>,
    ID: Copy + Clone + Display,
{
    let elem_count = 900;

    let created_segment: Segment<ID> =
        Segment::create(elem_count, TEST_SEGMENT_PREFIX).expect("error creating new segment");

    let opened_segment_instance_1 = Segment::open(created_segment.id(), TEST_SEGMENT_PREFIX)
        .expect("error opening existing segment!");

    validate_segment(&created_segment, &opened_segment_instance_1, elem_count);

    let opened_segment_instance_2 = Segment::open(created_segment.id(), TEST_SEGMENT_PREFIX)
        .expect("error opening existing segment!");

    validate_segment(&created_segment, &opened_segment_instance_1, elem_count);
    validate_segment(&created_segment, &opened_segment_instance_2, elem_count);

    drop(opened_segment_instance_1);
    validate_segment(&created_segment, &opened_segment_instance_2, elem_count);
}

/// UNSIGNED ///

#[test]
fn segment_u8_id() {
    test_segment::<u8>()
}

#[test]
fn segment_u16_id() {
    test_segment::<u16>()
}

#[test]
fn segment_u32_id() {
    test_segment::<u32>()
}

#[test]
fn segment_u64_id() {
    test_segment::<u64>()
}

#[test]
fn segment_u128_id() {
    test_segment::<u128>()
}

/// SIGNED ///

#[test]
fn segment_i8_id() {
    test_segment::<i8>()
}

#[test]
fn segment_i16_id() {
    test_segment::<i16>()
}

#[test]
fn segment_i32_id() {
    test_segment::<i32>()
}

#[test]
fn segment_i64_id() {
    test_segment::<i64>()
}

#[test]
fn segment_i128_id() {
    test_segment::<i128>()
}

/// Behaviour checks ///

#[test]
fn segment_open() {
    let created_segment: Segment<u8> =
        Segment::create(900, TEST_SEGMENT_PREFIX).expect("error creating new segment");

    let _opened_segment = Segment::open(created_segment.id(), TEST_SEGMENT_PREFIX)
        .expect("error opening existing segment!");
}

#[test]
fn segment_open_error() {
    let id = {
        let created_segment: Segment<u8> =
            Segment::create(900, TEST_SEGMENT_PREFIX).expect("error creating new segment");
        created_segment.id()
    };

    let _opened_segment = Segment::open(id, TEST_SEGMENT_PREFIX)
        .expect_err("must fail: opened not existing segment!");
}
