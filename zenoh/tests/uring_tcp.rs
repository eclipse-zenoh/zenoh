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
    feature = "uring",
    feature = "transport_tcp",
    target_os = "linux",
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64",
        target_arch = "loongarch64",
        target_arch = "powerpc64"
    )
))]

use std::{collections::HashSet, fs, time::Duration};

use zenoh::{config::WhatAmI, Config};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn uring_tcp_has_no_read_interest() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let port = zenoh_test::get_free_tcp_port();
        let endpoint = format!("tcp/127.0.0.1:{port}");
        let mut listen = Config::default();
        listen.set_mode(Some(WhatAmI::Peer)).unwrap();
        listen.scouting.multicast.set_enabled(Some(false)).unwrap();
        listen
            .listen
            .endpoints
            .set(vec![endpoint.parse().unwrap()])
            .unwrap();
        let mut connect = Config::default();
        connect.set_mode(Some(WhatAmI::Peer)).unwrap();
        connect.scouting.multicast.set_enabled(Some(false)).unwrap();
        connect
            .connect
            .endpoints
            .set(vec![endpoint.parse().unwrap()])
            .unwrap();
        let receiver = zenoh::open(listen).await.unwrap();
        let subscriber = receiver.declare_subscriber("test/uring/tcp").await.unwrap();
        let sender = zenoh::open(connect).await.unwrap();
        let publisher = sender.declare_publisher("test/uring/tcp").await.unwrap();
        while !publisher.matching_status().await.unwrap().matching() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        publisher.put("hello").await.unwrap();
        assert_eq!(
            subscriber
                .recv_async()
                .await
                .unwrap()
                .payload()
                .to_bytes()
                .as_ref(),
            b"hello"
        );

        // Only inspect established connections to this test's listener, not its listening socket.
        let port = format!("{port:04X}");
        let inodes: HashSet<_> = fs::read_to_string("/proc/net/tcp")
            .unwrap()
            .lines()
            .skip(1)
            .filter_map(|line| {
                let fields: Vec<_> = line.split_whitespace().collect();
                (fields[3] == "01"
                    && [fields[1], fields[2]]
                        .iter()
                        .any(|address| address.rsplit(':').next() == Some(port.as_str())))
                .then(|| fields[9].to_owned())
            })
            .collect();
        let fds: HashSet<_> = fs::read_dir("/proc/self/fd")
            .unwrap()
            .filter_map(|entry| {
                let entry = entry.unwrap();
                let target = fs::read_link(entry.path()).ok()?;
                let target = target.to_str()?;
                let inode = target.strip_prefix("socket:[")?.strip_suffix(']')?;
                inodes
                    .contains(inode)
                    .then(|| entry.file_name().to_str().unwrap().to_owned())
            })
            .collect();
        assert_eq!(fds.len(), 2);
        let mut registered = HashSet::new();
        for entry in fs::read_dir("/proc/self/fdinfo").unwrap() {
            let Ok(info) = fs::read_to_string(entry.unwrap().path()) else {
                continue;
            };
            for line in info.lines() {
                let fields: Vec<_> = line.split_whitespace().collect();
                if fields.first() == Some(&"tfd:") && fds.contains(fields[1]) {
                    let events = u32::from_str_radix(fields[3], 16).unwrap();
                    assert_eq!(events & 1, 0, "TCP fd {} still has EPOLLIN", fields[1]);
                    assert_ne!(events & 4, 0, "TCP fd {} lost EPOLLOUT", fields[1]);
                    registered.insert(fields[1].to_owned());
                }
            }
        }
        assert_eq!(registered, fds);
        sender.close().await.unwrap();
        receiver.close().await.unwrap();
    })
    .await
    .unwrap();
}
