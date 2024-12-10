//
// Copyright (c) 2024 ZettaScale Technology
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

use zenoh::{Config, Wait};

#[test]
fn buffer_size_config() {
    let mut config = Config::default();
    config
        .insert_json5(
            "transport/link/tcp",
            r#"
            {
                so_sndbuf: 65000,
                so_rcvbuf: 65000,
            }
            "#,
        )
        .unwrap();

    config
        .insert_json5("listen/endpoints", r#"["tcp/[::]:0"]"#)
        .unwrap();

    zenoh::open(config).wait().unwrap();
}

#[test]
fn buffer_size_endpoint() {
    let mut config = Config::default();
    config
        .insert_json5(
            "listen/endpoints",
            r#"["tcp/[::]:0#so_sndbuf=65000;so_rcvbuf=65000"]"#,
        )
        .unwrap();

    zenoh::open(config).wait().unwrap();
}

#[test]
fn buffer_size_endpoint_overwrite() {
    let mut config = Config::default();
    config
        .insert_json5(
            "transport/link/tcp",
            r#"
            {
                so_sndbuf: 0,
                so_rcvbuf: 0,
            }
            "#,
        )
        .unwrap();

    config
        .insert_json5(
            "listen/endpoints",
            r#"["tcp/[::]:0#so_sndbuf=65000;so_rcvbuf=65000"]"#,
        )
        .unwrap();

    zenoh::open(config).wait().unwrap();
}

#[cfg(target_os = "macos")]
#[test]
#[should_panic(expected = "Can not create a new TCP listener")]
fn buffer_size_zero() {
    listen_zero_buffers();
}

#[cfg(not(target_os = "macos"))]
#[test]
fn buffer_size_zero() {
    listen_zero_buffers();
}

fn listen_zero_buffers() {
    let mut config = Config::default();
    config
        .insert_json5(
            "listen/endpoints",
            r#"["tcp/[::]:0#so_sndbuf=0;so_rcvbuf=0"]"#,
        )
        .unwrap();
    zenoh::open(config).wait().unwrap();
}
