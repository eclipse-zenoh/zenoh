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

fn main() {
    // `shm_external_lockfile` is the only cfg alias this crate actually
    // queries (see src/posix_shm/cleanup.rs, src/shm/unix.rs) -- it marks
    // platforms that don't support advisory file locking on tmpfs: the BSD
    // family (including all Apple targets, which are BSD-derived) plus
    // Redox. Computed directly instead of pulling in the `cfg_aliases`
    // crate for a single derived flag.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS")
        .expect("CARGO_CFG_TARGET_OS must be set by Cargo when running build scripts");
    let shm_external_lockfile = matches!(
        target_os.as_str(),
        "freebsd"
            | "dragonfly"
            | "netbsd"
            | "openbsd"
            | "ios"
            | "macos"
            | "watchos"
            | "tvos"
            | "visionos"
            | "redox"
    );

    println!("cargo:rustc-check-cfg=cfg(shm_external_lockfile)");
    if shm_external_lockfile {
        println!("cargo:rustc-cfg=shm_external_lockfile");
    }
}
