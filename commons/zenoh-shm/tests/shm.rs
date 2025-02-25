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
use zenoh_shm::shm::Segment;

#[test]
fn create() {
    let id = 0u32;
    let len = 1024.try_into().unwrap();
    let created_segment = Segment::create(id, len ).unwrap();
    assert!(created_segment.len() >= len);
    created_segment.unlink();
}

#[test]
fn create_and_open() {
    let id = 1u32;
    let len = 1024.try_into().unwrap();
    let created_segment = Segment::create(id, len ).unwrap();
    let opened_segment = Segment::open(id).unwrap();
    assert!(created_segment.len() >= len);
    assert!(opened_segment.len() >= len);
    opened_segment.unlink();
}

#[test]
fn create_and_open_amd_reopen() {
    let id = 2u32;
    let len = 1024.try_into().unwrap();
    let created_segment = Segment::create(id, len ).unwrap();
    let opened_segment = Segment::open(id).unwrap();
    let opened_segment2 = Segment::open(id).unwrap();
    assert!(created_segment.len() >= len);
    assert!(opened_segment.len() >= len);
    assert!(opened_segment2.len() >= len);
    opened_segment.unlink();
}

#[test]
fn create_and_open_amd_reopen_and_open_closed() {
    let id = 3u32;
    let len = 1024.try_into().unwrap();
    let created_segment = Segment::create(id, len ).unwrap();
    let opened_segment = Segment::open(id).unwrap();
    assert!(created_segment.len() >= len);
    assert!(opened_segment.len() >= len);

    drop(created_segment);

    let opened_segment2 = Segment::open(id).unwrap();
    assert!(opened_segment2.len() >= len);
    opened_segment2.unlink();
}

#[test]
fn persistency() {
    let id = 4u32;
    let len = 1024.try_into().unwrap();
    let created_segment = Segment::create(id, len ).unwrap();
    assert!(created_segment.len() >= len);
    drop(created_segment);

    let opened_segment = Segment::open(id).unwrap();
    assert!(opened_segment.len() >= len);
    opened_segment.unlink();
}

#[test]
fn unlink() {
    let id = 5u32;
    let len = 1024.try_into().unwrap();
    let created_segment = Segment::create(id, len ).unwrap();
    assert!(created_segment.len() >= len);
    created_segment.unlink();

    assert!(Segment::open(id).is_err());
}