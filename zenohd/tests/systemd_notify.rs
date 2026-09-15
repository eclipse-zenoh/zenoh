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
#![cfg(all(feature = "systemd", target_os = "linux"))]

use std::{
    net::TcpStream,
    os::unix::net::UnixDatagram,
    process::{Child, Command},
    time::Duration,
};

use zenoh_test::get_free_tcp_port;

const TIMEOUT: Duration = Duration::from_secs(60);

struct KillOnDrop(Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn readiness_is_notified_after_listeners_are_bound() {
    let port = get_free_tcp_port();
    let socket_path =
        std::env::temp_dir().join(format!("zenohd-notify-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&socket_path);
    let socket = UnixDatagram::bind(&socket_path).unwrap();
    socket.set_read_timeout(Some(TIMEOUT)).unwrap();

    let child = Command::new(env!("CARGO_BIN_EXE_zenohd"))
        .args([
            "--no-multicast-scouting",
            "-l",
            &format!("tcp/127.0.0.1:{port}"),
        ])
        .env("NOTIFY_SOCKET", &socket_path)
        .spawn()
        .unwrap();
    let _child = KillOnDrop(child);

    let mut buf = [0u8; 1024];
    let n = socket
        .recv(&mut buf)
        .expect("no readiness notification received");
    let msg = std::str::from_utf8(&buf[..n]).unwrap();
    assert!(
        msg.lines().any(|line| line == "READY=1"),
        "unexpected notification: {msg}"
    );

    // READY=1 must not be sent before the router accepts sessions
    TcpStream::connect(("127.0.0.1", port)).expect("listener not accepting after READY=1");

    let _ = std::fs::remove_file(&socket_path);
}
