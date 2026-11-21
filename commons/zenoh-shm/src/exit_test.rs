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

//! Helpers for tests that access SHM statics after they are finalized at process exit.
//!
//! Such a test re-runs itself in a child process, whose `#[destructor(1)]` runs after
//! the priority 0 destructors that finalize the statics.

use std::{io::Write, process::Command};

std::arch::global_asm!(".weak __llvm_profile_write_file");
extern "C" {
    fn __llvm_profile_write_file() -> i32;
}

/// Whether the current process is the child spawned by [`assert_child_succeeds`].
pub fn is_child(child_env: &str) -> bool {
    std::env::var_os(child_env).is_some()
}

/// Reports from a destructor that the child process succeeded.
///
/// The coverage runtime writes its profile at exit before the destructors run,
/// so this writes it again to include the code run by the destructor.
pub fn report_child_success(success_marker: &str) {
    let write_coverage_profile = std::hint::black_box(__llvm_profile_write_file as *const ());
    if !write_coverage_profile.is_null() {
        unsafe { __llvm_profile_write_file() };
    }
    let _ = writeln!(std::io::stdout(), "{success_marker}");
}

/// Runs `test_name` in a child process and asserts that it reported success.
pub fn assert_child_succeeds(test_name: &str, child_env: &str, success_marker: &str) {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test_name, "--nocapture"])
        .env(child_env, "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "child failed: {:?}\nstdout:\n{stdout}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains(success_marker), "stdout:\n{stdout}");
}
