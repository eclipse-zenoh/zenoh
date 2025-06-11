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
mod common;

use zenoh::Wait;

use crate::common::open_peer;

#[test]
fn buffer_size_config() {
    let tcp_conf = r#"{
        so_sndbuf: 65000,
        so_rcvbuf: 65000,
    }"#;
    open_peer()
        .with_json5("transport/link/tcp", tcp_conf)
        .wait();
}

#[test]
fn buffer_size_endpoint() {
    let endpoint = "tcp/[::]:0#so_sndbuf=65000;so_rcvbuf=65000";
    open_peer().with("listen/endpoints", [endpoint]).wait();
}

#[test]
fn buffer_size_endpoint_overwrite() {
    let tcp_conf = r#"{
        so_sndbuf: 0,
        so_rcvbuf: 0,
    }"#;
    let endpoint = "tcp/[::]:0#so_sndbuf=65000;so_rcvbuf=65000";
    open_peer()
        .with_json5("transport/link/tcp", tcp_conf)
        .with("listen/endpoints", [endpoint])
        .wait();
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
    let endpoint = "tcp/[::]:0#so_sndbuf=0;so_rcvbuf=0";
    open_peer().with("listen/endpoints", [endpoint]).wait();
}
