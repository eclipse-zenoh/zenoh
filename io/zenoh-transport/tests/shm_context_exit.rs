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
#![cfg(all(feature = "shared-memory", feature = "test", target_os = "linux"))]
use std::sync::atomic::{AtomicBool, Ordering};

use static_init::destructor;
use zenoh_config::ShmConf;
use zenoh_shm::exit_test;
use zenoh_transport::common::shm::shm_context::ShmContext;

const CHILD_ENV: &str = "ZENOH_TRANSPORT_TEST_SHM_CONTEXT_AFTER_EXIT_CHILD";
const SUCCESS_MARKER: &str = "shm context creation after exit returned an error";

static SHM_STATICS_INITIALIZED: AtomicBool = AtomicBool::new(false);

// Runs after the priority 0 destructors that finalize the SHM statics
#[destructor(1)]
extern "C" fn create_shm_context_after_statics_are_finalized() {
    if !SHM_STATICS_INITIALIZED.load(Ordering::SeqCst) {
        return;
    }

    if ShmContext::new(ShmConf::default(), None).is_err() {
        exit_test::report_child_success(SUCCESS_MARKER);
    }
}

#[test]
fn shm_context_creation_after_exit() {
    if !exit_test::is_child(CHILD_ENV) {
        exit_test::assert_child_succeeds(
            "shm_context_creation_after_exit",
            CHILD_ENV,
            SUCCESS_MARKER,
        );
        return;
    }

    // Initialize the SHM client storage so that it is finalized at exit
    assert!(ShmContext::new(ShmConf::default(), None).unwrap().is_some());
    SHM_STATICS_INITIALIZED.store(true, Ordering::SeqCst);
}
