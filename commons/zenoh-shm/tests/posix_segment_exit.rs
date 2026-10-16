//
// Copyright (c) 2026 ZettaScale Technology
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
#![cfg(all(feature = "test", target_os = "linux"))]
use std::sync::atomic::{AtomicU32, Ordering};

use static_init::destructor;
use zenoh_shm::{exit_test, posix_shm::segment::Segment, shm};

const CHILD_ENV: &str = "ZENOH_SHM_TEST_SEGMENT_AFTER_EXIT_CHILD";
const SUCCESS_MARKER: &str = "segment access after exit returned errors";
const SEGMENT_LEN: usize = 64;

static PERSISTENT_SEGMENT_ID: AtomicU32 = AtomicU32::new(0);

// Runs after the priority 0 destructors that finalize the SHM statics
#[destructor(1)]
extern "C" fn access_segments_after_statics_are_finalized() {
    let id = PERSISTENT_SEGMENT_ID.load(Ordering::SeqCst);
    if id == 0 {
        return;
    }

    let len = SEGMENT_LEN.try_into().unwrap();
    let created = Segment::<u32>::create(len);
    let opened = Segment::<u32>::open(id);
    shm::Segment::<u32>::ensure_not_persistent(id);

    if created.is_err() && opened.is_err() {
        exit_test::report_child_success(SUCCESS_MARKER);
    }
}

fn create_persistent_segment() -> u32 {
    let len = SEGMENT_LEN.try_into().unwrap();
    std::iter::repeat_with(rand::random::<u32>)
        .filter(|id| *id != 0)
        .find_map(|id| {
            // Keep the segment and its lock alive past the SHM cleanup at exit
            std::mem::forget(shm::Segment::create(id, len).ok()?);
            Some(id)
        })
        .unwrap()
}

#[test]
fn posix_segment_access_after_exit() {
    if !exit_test::is_child(CHILD_ENV) {
        exit_test::assert_child_succeeds(
            "posix_segment_access_after_exit",
            CHILD_ENV,
            SUCCESS_MARKER,
        );
        return;
    }

    // Initialize the SHM statics so that they are finalized at exit
    let _segment = Segment::<u32>::create(SEGMENT_LEN.try_into().unwrap()).unwrap();
    PERSISTENT_SEGMENT_ID.store(create_persistent_segment(), Ordering::SeqCst);
}
