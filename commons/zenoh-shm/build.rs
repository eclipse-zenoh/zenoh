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
    // these aliases should at least be included in the same aliases of Nix crate:
    // ___________________
    // |                 |
    // |  Nix aliases    |
    // |  ___________    |
    // |  |   Our   |    |
    // |  | aliases |    |
    // |  |_________|    |
    // |_________________|
    //
    // NOTE: hand-rolled instead of using the `cfg_aliases` crate -- its macro
    // expansion trips rustc's `semicolon_in_expressions_from_macros` lint,
    // which is a hard error on current nightly (no fixed crate release
    // exists as of writing). This is equivalent, just spelled out.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    let dragonfly = target_os == "dragonfly";
    let ios = target_os == "ios";
    let freebsd = target_os == "freebsd";
    let macos = target_os == "macos";
    let netbsd = target_os == "netbsd";
    let openbsd = target_os == "openbsd";
    let watchos = target_os == "watchos";
    let tvos = target_os == "tvos";
    let visionos = target_os == "visionos";
    let redox = target_os == "redox";

    let apple_targets = ios || macos || watchos || tvos || visionos;
    let bsd = freebsd || dragonfly || netbsd || openbsd || apple_targets;
    // we use this alias to detect platforms that
    // don't support advisory file locking on tmpfs
    let shm_external_lockfile = bsd || redox;

    for (name, active) in [
        ("dragonfly", dragonfly),
        ("ios", ios),
        ("freebsd", freebsd),
        ("macos", macos),
        ("netbsd", netbsd),
        ("openbsd", openbsd),
        ("watchos", watchos),
        ("tvos", tvos),
        ("visionos", visionos),
        ("apple_targets", apple_targets),
        ("bsd", bsd),
        ("shm_external_lockfile", shm_external_lockfile),
    ] {
        println!("cargo:rustc-check-cfg=cfg({name})");
        if active {
            println!("cargo:rustc-cfg={name}");
        }
    }
}
