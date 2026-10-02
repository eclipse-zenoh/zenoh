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
#![cfg(all(target_os = "linux", feature = "shared-memory"))]

//! With `shared_memory.mode: "lazy"` (the default), opening a session must not
//! start the SHM background threads. The test counts the threads by name in
//! `/proc`, so it runs on Linux only. Each mode runs in its own process,
//! because the SHM statics are process-global.

use std::{process::Command, time::Duration};

use zenoh::Config;

const PROBE: &str = "PROBE_SHM_LAZY_INIT";
const MARKER: &str = "shm_lazy_init_peer watchdog threads: ";

fn watchdog_threads() -> usize {
    std::fs::read_dir("/proc/self/task")
        .unwrap()
        .filter_map(|task| std::fs::read_to_string(task.unwrap().path().join("comm")).ok())
        .filter(|name| name.starts_with("Watchdog"))
        .count()
}

#[ignore]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shm_lazy_init_peer() {
    let Ok(mode) = std::env::var(PROBE) else {
        return;
    };
    let mut config = Config::default();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    match mode.as_str() {
        "disabled" => {
            config.transport.shared_memory.set_enabled(false).unwrap();
        }
        "init" => {
            config
                .transport
                .shared_memory
                .set_mode(zenoh_config::ShmInitMode::Init)
                .unwrap();
        }
        _ => {}
    }
    let _session = zenoh::open(config).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    eprintln!("{MARKER}{}", watchdog_threads());
}

fn threads_after_open(mode: &str) -> usize {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "shm_lazy_init_peer",
            "--nocapture",
            "--exact",
            "--include-ignored",
        ])
        .env(PROBE, mode)
        .output()
        .expect("Failed to run peer in separate process");
    let stderr = String::from_utf8_lossy(&output.stderr);
    stderr
        .lines()
        .find_map(|line| line.split_once(MARKER))
        .and_then(|(_, count)| count.trim().parse().ok())
        .unwrap_or_else(|| panic!("peer for mode {mode} did not report:\n{stderr}"))
}

#[test]
fn lazy_mode_does_not_start_shm_threads() {
    // Controls: the count must see threads when `init` asks for them, and
    // none when SHM is off. Otherwise a zero in lazy mode proves nothing.
    assert_eq!(threads_after_open("disabled"), 0);
    assert!(
        threads_after_open("init") > 0,
        "mode init started no SHM thread: the test cannot detect them"
    );
    assert_eq!(
        threads_after_open("lazy"),
        0,
        "mode lazy started SHM threads at session open"
    );
}
