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
#![cfg(all(unix, feature = "shared-memory"))]

//! A process that exits with an open session must not panic when a peer
//! connects while its SHM statics are being finalized. The window is a few
//! hundred microseconds per exit, so many short lived peers run concurrently:
//! each new one scouts and connects to the others, some of which are exiting.

use std::{process::Command, time::Duration};

use zenoh::Config;

const PROBE: &str = "PROBE_SHM_EXIT_PEER";
const CONCURRENCY: usize = 16;
const ITERATIONS: usize = 15;

fn config() -> Config {
    let mut config = Config::default();
    config.scouting.set_delay(Some(0)).unwrap();
    config
}

#[ignore]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shm_exit_peer() {
    if std::env::var(PROBE).is_err() {
        return;
    }
    let session = zenoh::open(config()).await.unwrap();
    let queryable = session
        .declare_queryable("shm_exit/service")
        .callback(|_| {})
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    // Leak the session: the process exits with it open, like an application
    // that relies on process exit for cleanup.
    std::mem::forget(queryable);
    std::mem::forget(session);
}

fn run_peer() -> Result<(), String> {
    let output = Command::new(std::env::current_exe().unwrap())
        .arg("shm_exit_peer")
        .arg("--nocapture")
        .arg("--exact")
        .arg("--include-ignored")
        .env(PROBE, "true")
        .env("RUST_BACKTRACE", "1")
        .output()
        .expect("Failed to run peer in separate process");
    let stderr = String::from_utf8_lossy(&output.stderr);
    // The static_init access error is the failure this test exists for.
    // Other panics at exit, such as routing panics under peer churn, are
    // reported but do not fail the test, so that unrelated bugs do not
    // hide this one.
    if stderr.contains("AccessError") {
        return Err(format!("peer panicked at exit:\n{stderr}"));
    }
    if stderr.contains("panicked") {
        eprintln!("peer panicked at exit for another reason:\n{stderr}");
    }
    Ok(())
}

#[test]
fn shm_exit_race() {
    let workers: Vec<_> = (0..CONCURRENCY)
        .map(|_| {
            std::thread::spawn(|| {
                (0..ITERATIONS)
                    .filter_map(|_| run_peer().err())
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    let failures: Vec<String> = workers.into_iter().flat_map(|w| w.join().unwrap()).collect();
    assert!(
        failures.is_empty(),
        "{} of {} peers panicked or crashed at exit, first:\n{}",
        failures.len(),
        CONCURRENCY * ITERATIONS,
        failures[0]
    );
}
