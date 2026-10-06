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

#![cfg(all(
    target_os = "linux",
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64",
        target_arch = "loongarch64",
        target_arch = "powerpc64"
    )
))]

use std::{
    process::Command,
    time::{Duration, Instant},
};

use zenoh_uring::api::reader::Reader;

#[test]
fn initialization() {
    const CHILD: &str = "ZENOH_URING_INIT_TEST";
    let Ok(case) = std::env::var(CHILD) else {
        // Resource limits and seccomp must not affect other tests or their runtimes.
        for case in ["available", "setup", "enter", "memlock"] {
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "initialization", "--nocapture"])
                .env(CHILD, case)
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(20);
            loop {
                if let Some(status) = child.try_wait().unwrap() {
                    assert!(status.success(), "initialization case {case}: {status}");
                    break;
                }
                if Instant::now() >= deadline {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("initialization case {case} timed out");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        return;
    };

    match case.as_str() {
        "setup" | "enter" => {
            let syscall = if case == "setup" {
                libc::SYS_io_uring_setup
            } else {
                libc::SYS_io_uring_enter
            };
            let mut filter = [
                libc::sock_filter {
                    code: (libc::BPF_LD | libc::BPF_W | libc::BPF_ABS) as u16,
                    jt: 0,
                    jf: 0,
                    k: 0,
                },
                libc::sock_filter {
                    code: (libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K) as u16,
                    jt: 0,
                    jf: 1,
                    k: syscall as u32,
                },
                libc::sock_filter {
                    code: (libc::BPF_RET | libc::BPF_K) as u16,
                    jt: 0,
                    jf: 0,
                    k: libc::SECCOMP_RET_ERRNO | libc::EPERM as u32,
                },
                libc::sock_filter {
                    code: (libc::BPF_RET | libc::BPF_K) as u16,
                    jt: 0,
                    jf: 0,
                    k: libc::SECCOMP_RET_ALLOW,
                },
            ];
            let program = libc::sock_fprog {
                len: filter.len() as u16,
                filter: filter.as_mut_ptr(),
            };
            unsafe {
                assert_eq!(libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0), 0);
                assert_eq!(
                    libc::prctl(
                        libc::PR_SET_SECCOMP,
                        libc::SECCOMP_MODE_FILTER,
                        &program,
                        0,
                        0
                    ),
                    0
                );
            }
        }
        "memlock" => {
            let status = std::fs::read_to_string("/proc/self/status").unwrap();
            let capabilities = status
                .lines()
                .find_map(|line| line.strip_prefix("CapEff:"))
                .unwrap();
            // CAP_IPC_LOCK bypasses RLIMIT_MEMLOCK, regardless of the process UID.
            if u64::from_str_radix(capabilities.trim(), 16).unwrap() & (1 << 14) != 0 {
                eprintln!("Skipping memlock case: CAP_IPC_LOCK bypasses RLIMIT_MEMLOCK");
                return;
            }
            let limit = libc::rlimit {
                rlim_cur: 65536,
                rlim_max: 65536,
            };
            assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_MEMLOCK, &limit) }, 0);
        }
        "available" => {}
        _ => unreachable!(),
    }

    let result = Reader::new(65537, 16);
    if case == "available" {
        result.unwrap();
    } else {
        let error = result.expect_err("Reader must return the actual reactor startup error");
        if case == "memlock" {
            assert!(
                error.to_string().contains("Unable to reserve initial"),
                "{error}"
            );
        } else {
            assert_eq!(
                error
                    .downcast_ref::<std::io::Error>()
                    .unwrap()
                    .raw_os_error(),
                Some(libc::EPERM)
            );
        }
    }
}
