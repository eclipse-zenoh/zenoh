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
fn main() {
    // Add rustc version to zenohd
    let version_meta = rustc_version::version_meta().unwrap();
    println!(
        "cargo:rustc-env=RUSTC_VERSION={}",
        version_meta.short_version_string
    );

    let version = rustc_version::version().unwrap();
    if version >= rustc_version::Version::parse("1.85.0").unwrap()
        && version <= rustc_version::Version::parse("1.85.1").unwrap()
    {
        // https://github.com/rust-lang/rust/issues/138696
        println!("cargo:rustc-cfg=nolocal_thread_not_available");
    }

    // Only use `git_version!` when the workspace root has its own `.git` (a zenoh checkout),
    // otherwise it may pick up an unrelated enclosing repository:
    // https://github.com/eclipse-zenoh/zenoh/issues/2663
    // No `rerun-if-changed` on `.git`: that would rebuild zenoh on every git operation.
    let manifest_dir = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    if manifest_dir.join("../.git").exists() {
        println!("cargo:rustc-cfg=zenoh_git_checkout");
    }
}
