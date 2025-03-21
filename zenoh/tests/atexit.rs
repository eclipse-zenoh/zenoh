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

#![cfg(feature = "internal_config")]

fn run_in_separate_process(main_name: &str, must_panic: bool) {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .arg(main_name)
        .arg("--nocapture")
        .arg("--show-output")
        .arg("--exact")
        .arg("--include-ignored")
        .env(
            {
                match must_panic {
                    true => "PROBE_PANIC_IN_TEST",
                    false => "PROBE_NO_PANIC_IN_TEST",
                }
            },
            "true",
        )
        .output()
        .expect("Failed to run test in separate process");

    match must_panic {
        true => {
            assert!(
                !output.status.success(),
                "Inner test should have been failed"
            );
        }
        false => {
            assert!(output.status.success(), "Inner test failed");
        }
    };
}

#[ignore]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn panic_in_separate_process() {
    if std::env::var("PROBE_PANIC_IN_TEST").is_ok() {
        panic!("Panic in test because PROBE_PANIC_IN_TEST is set!");
    }
}

#[ignore]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn atexit_panic_in_separate_process() {
    unsafe { libc::atexit(panic_in_atexit) };
}

extern "C" fn panic_in_atexit() {
    if std::env::var("PROBE_PANIC_IN_TEST").is_ok() {
        panic!("Panic in atexit because PROBE_PANIC_IN_TEST is set!");
    }
}

#[test]
fn panic_is_seen_in_separate_process() {
    run_in_separate_process("panic_in_separate_process", true);
}

#[test]
fn panic_is_seen_in_separate_process_atexit() {
    run_in_separate_process("atexit_panic_in_separate_process", true);
}

use std::sync::OnceLock;

use zenoh::{Session, Wait};
static SESSION: OnceLock<Session> = OnceLock::new();

#[ignore]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn session_close_in_atexit_main() {
    if std::env::var("PROBE_PANIC_IN_TEST").is_ok() {
        panic!("Panic in test because PROBE_PANIC_IN_TEST is set!");
    }

    // Open the sessions
    let mut config = zenoh::Config::default();
    config
        .listen
        .endpoints
        .set(vec!["tcp/127.0.0.1:19446".parse().unwrap()])
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();

    let s = zenoh::open(config).await.unwrap();
    let _session = SESSION.get_or_init(move || s);

    unsafe { libc::atexit(close_session_in_atexit) };
}

extern "C" fn close_session_in_atexit() {
    let session = SESSION.get().unwrap();
    session.close().wait().unwrap();
}

#[cfg(not(nolocal_thread_not_available))]
#[test]
fn session_close_in_atexit() {
    run_in_separate_process("session_close_in_atexit_main", true);
    run_in_separate_process("session_close_in_atexit_main", false);
}

#[cfg(all(feature = "unstable", feature = "internal"))]
use std::sync::Mutex;

#[cfg(all(feature = "unstable", feature = "internal"))]
use zenoh::{internal::builders::close::NolocalJoinHandle, Result};

#[cfg(all(feature = "unstable", feature = "internal"))]
static BACKGROUND_SESSION: OnceLock<Session> = OnceLock::new();
#[cfg(all(feature = "unstable", feature = "internal"))]
static CLOSE: OnceLock<Mutex<Option<NolocalJoinHandle<Result<()>>>>> = OnceLock::new();

#[cfg(all(feature = "unstable", feature = "internal"))]
#[ignore]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn background_session_close_in_atexit_main() {
    if std::env::var("PROBE_PANIC_IN_TEST").is_ok() {
        panic!("Panic in test because PROBE_PANIC_IN_TEST is set!");
    }

    // Open the sessions
    let mut config = zenoh::Config::default();
    config
        .listen
        .endpoints
        .set(vec!["tcp/127.0.0.1:19445".parse().unwrap()])
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();

    let s = zenoh::open(config).await.unwrap();
    let session = BACKGROUND_SESSION.get_or_init(move || s);

    let _close = CLOSE.get_or_init(|| Mutex::new(Some(session.close().in_background().wait())));

    unsafe { libc::atexit(background_close_session_in_atexit) };
}

#[cfg(all(feature = "unstable", feature = "internal"))]
extern "C" fn background_close_session_in_atexit() {
    let mut close = CLOSE.get().unwrap().lock().unwrap();
    close.take().unwrap().wait().unwrap();
}

#[cfg(all(feature = "unstable", feature = "internal"))]
#[test]
fn background_session_close_in_atexit() {
    run_in_separate_process("background_session_close_in_atexit_main", true);
    run_in_separate_process("background_session_close_in_atexit_main", false);
}
