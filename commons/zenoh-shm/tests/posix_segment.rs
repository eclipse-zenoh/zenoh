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

use std::{fmt::Display, slice};

use zenoh_shm::posix_shm::segment::Segment;

pub mod common;
use common::{validate_memory, TEST_SEGMENT_PREFIX};

fn validate_segment<ID>(segment1: &Segment<ID>, segment2: &Segment<ID>)
where
    rand::distributions::Standard: rand::distributions::Distribution<ID>,
    ID: Clone + Display,
{
    assert!(segment1.len() == segment2.len());

    let ptr1 = segment1.as_ptr();
    let ptr2 = segment2.as_ptr();

    let slice1 = unsafe { slice::from_raw_parts_mut(ptr1, segment1.len()) };
    let slice2 = unsafe { slice::from_raw_parts(ptr2, segment2.len()) };

    validate_memory(slice1, slice2);
}

fn test_segment<ID>()
where
    rand::distributions::Standard: rand::distributions::Distribution<ID>,
    ID: Copy + Clone + Display,
{
    let new_segment: Segment<ID> =
        Segment::create(900, TEST_SEGMENT_PREFIX).expect("error creating new segment");

    let opened_segment_instance_1 = Segment::open(new_segment.id(), TEST_SEGMENT_PREFIX)
        .expect("error opening existing segment!");

    validate_segment(&new_segment, &opened_segment_instance_1);

    let opened_segment_instance_2 = Segment::open(new_segment.id(), TEST_SEGMENT_PREFIX)
        .expect("error opening existing segment!");

    validate_segment(&new_segment, &opened_segment_instance_1);
    validate_segment(&new_segment, &opened_segment_instance_2);

    drop(opened_segment_instance_1);
    validate_segment(&new_segment, &opened_segment_instance_2);
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
    let new_segment: Segment<u8> =
        Segment::create(900, TEST_SEGMENT_PREFIX).expect("error creating new segment");

    let _opened_segment = Segment::open(new_segment.id(), TEST_SEGMENT_PREFIX)
        .expect("error opening existing segment!");
}

#[test]
fn segment_open_error() {
    let id = {
        let new_segment: Segment<u8> =
            Segment::create(900, TEST_SEGMENT_PREFIX).expect("error creating new segment");
        new_segment.id()
    };

    let _opened_segment = Segment::open(id, TEST_SEGMENT_PREFIX)
        .expect_err("must fail: opened not existing segment!");
}
