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

#![cfg(feature = "internal_config")]

use std::process::Command;
use std::sync::OnceLock;
use zenoh::Session;
use zenoh::Wait;

static SESSION: OnceLock<Session> = OnceLock::new();

#[ignore]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn session_close_in_atexit_main() {
    if std::env::var("PANIC_IN_TEST").is_ok() {
        panic!("Panic in test because PANIC_IN_TEST is set!");
    }

    // Open the sessions
    let mut config = zenoh::Config::default();
    config
        .listen
        .endpoints
        .set(vec!["tcp/127.0.0.1:19447".parse().unwrap()])
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();

    let s = zenoh::open(config).await.unwrap();
    let _session = SESSION.get_or_init(move || s);

    unsafe { libc::atexit(close_session_in_atexit) };
}

extern "C" fn close_session_in_atexit() {
    let session = SESSION.get().unwrap();
    let _ = session.close().wait().unwrap();
}

#[test]
fn panic_is_seen_in_separate_process() {
    let output = Command::new(std::env::current_exe().unwrap()) // Get current test binary
        .arg("session_close_in_atexit_main")
        .arg("--nocapture") // Optional: show output
        .arg("--show-output") // Run a single test by exact name
        .arg("--exact") // Run a single test by exact name
        .arg("--include-ignored") // Run a single test by exact name
        .env("PANIC_IN_TEST", "true") // Set environment variable
        .output()
        .expect("Failed to run test in separate process");

    assert!(
        !output.status.success(),
        "Inner test should have been failed"
    );
}

#[test]
fn session_close_in_atexit() {
    let output = Command::new(std::env::current_exe().unwrap()) // Get current test binary
        .arg("session_close_in_atexit_main")
        .arg("--nocapture") // Optional: show output
        .arg("--show-output") // Run a single test by exact name
        .arg("--exact") // Run a single test by exact name
        .arg("--include-ignored") // Run a single test by exact name
        .output()
        .expect("Failed to run test in separate process");

    assert!(output.status.success(), "Inner test failed");
}
