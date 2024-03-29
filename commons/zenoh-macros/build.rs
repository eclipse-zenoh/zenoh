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
use std::{env, ffi::OsString, fs::OpenOptions, io::Write, process::Command};

fn main() {
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
    let output = Command::new(rustc)
        .arg("-v")
        .arg("-V")
        .output()
        .expect("Couldn't get rustc version");
    let version_rs = std::path::PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("version.rs");
    let mut version_rs = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(version_rs)
        .unwrap();
    version_rs.write_all(&output.stdout).unwrap();
}
