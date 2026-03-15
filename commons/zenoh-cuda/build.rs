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
fn main() {
    // Re-run if CUDA_PATH changes so the linker search path stays in sync.
    println!("cargo:rerun-if-env-changed=CUDA_PATH");

    // Link against CUDA runtime library
    println!("cargo:rustc-link-lib=dylib=cudart");

    // Common CUDA installation paths
    if let Ok(cuda_path) = std::env::var("CUDA_PATH") {
        println!("cargo:rustc-link-search={cuda_path}/lib64");
        println!("cargo:rustc-link-search={cuda_path}/lib");
    } else {
        println!("cargo:rustc-link-search=/usr/local/cuda/lib64");
        println!("cargo:rustc-link-search=/usr/local/cuda/lib");
        println!("cargo:rustc-link-search=/usr/lib/x86_64-linux-gnu");
    }
}
