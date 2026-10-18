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

//! Buffers provided to the kernel for a stopped read task must be reclaimed:
//! otherwise every link (re)connection grows the mlocked arena.

#[cfg(target_os = "linux")]
mod linux_tests {
    use std::{
        io::Write,
        net::{TcpListener, TcpStream},
        os::fd::AsRawFd,
        sync::{
            atomic::{AtomicUsize, Ordering},
            mpsc, Arc,
        },
        time::Duration,
    };

    use zenoh_uring::api::reader::{rx_buffer::RxBuffer, Reader};

    const TIMEOUT: Duration = Duration::from_secs(10);

    /// Process-wide locked memory; the arena is the only mlock user here.
    fn vmlck_kib() -> usize {
        std::fs::read_to_string("/proc/self/status")
            .unwrap()
            .lines()
            .find_map(|l| l.strip_prefix("VmLck:"))
            .and_then(|v| v.split_whitespace().next())
            .and_then(|v| v.parse().ok())
            .unwrap()
    }

    /// Wait until the reader thread of a dropped `Reader` unmapped its arena.
    fn wait_unlocked() {
        let deadline = std::time::Instant::now() + TIMEOUT;
        while vmlck_kib() != 0 {
            assert!(std::time::Instant::now() < deadline, "arena not released");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    struct Link {
        listener: TcpListener,
        received: Arc<AtomicUsize>,
    }

    impl Link {
        fn new() -> Self {
            Self {
                listener: TcpListener::bind("127.0.0.1:0").unwrap(),
                received: Arc::new(AtomicUsize::new(0)),
            }
        }

        /// One link lifetime: connect, receive `msgs` payloads through a read
        /// task, stop the task, close both sockets.
        async fn cycle(&self, reader: &Reader, msgs: usize) {
            let mut client = TcpStream::connect(self.listener.local_addr().unwrap()).unwrap();
            let (server, _) = self.listener.accept().unwrap();
            let got = self.received.clone();
            let expected = got.load(Ordering::SeqCst) + msgs * 64;
            let mut task = reader
                .setup_read(server.as_raw_fd(), move |buf| {
                    got.fetch_add(buf.len(), Ordering::SeqCst);
                    Ok(())
                })
                .await
                .unwrap();
            for _ in 0..msgs {
                client.write_all(&[0xAB; 64]).unwrap();
            }
            while self.received.load(Ordering::SeqCst) < expected {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            // returns once the kernel handed every buffer of the task back
            task.stop().await;
        }
    }

    /// `cycles` sequential link lifetimes on one reader with `msgs` payloads
    /// each. With concurrency 1 the retired group's buffers are recycled
    /// before the next group is created, so after the first cycle the locked
    /// arena must not grow at all.
    async fn churn(batch_size: usize, batch_count: u16, cycles: usize, msgs: usize) {
        let reader = Reader::new(batch_size, batch_count).unwrap();
        let link = Link::new();
        link.cycle(&reader, msgs).await;
        let warm = vmlck_kib();
        for _ in 1..cycles {
            link.cycle(&reader, msgs).await;
        }
        let last = vmlck_kib();
        println!("[{batch_size}x{batch_count} msgs={msgs}] after first cycle {warm} KiB, after {cycles} cycles {last} KiB");
        // the arena is 1.5 * batch_size * batch_count, expanded at most once
        let arena = batch_size * 3 / 2 * batch_count as usize / 1024;
        assert!(warm >= arena, "arena not locked: {warm} KiB");
        assert!(warm <= 2 * arena + 64, "arena too big: {warm} KiB");
        assert_eq!(last, warm, "locked arena grew across {cycles} link cycles");
        drop(reader);
        wait_unlocked();
    }

    /// Stop tasks while the peer keeps writing: buffers may be selected by a
    /// receive in flight at StopRx time. Retirement must wait for it, so
    /// stopping must reclaim everything (a leaked group would show up as an
    /// arena expansion) and the sender must end up rejected.
    async fn stop_under_traffic(cycles: usize) {
        let reader = Reader::new(4096, 16).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let mut warm = 0;
        for cycle in 0..cycles {
            let mut client = TcpStream::connect(addr).unwrap();
            let (server, _) = listener.accept().unwrap();
            let (writer_done, done) = mpsc::channel();
            let writer = std::thread::spawn(move || {
                while client.write_all(&[0xCD; 4096]).is_ok() {}
                writer_done.send(()).unwrap();
            });
            let received = Arc::new(AtomicUsize::new(0));
            let got = received.clone();
            let mut task = reader
                .setup_read(server.as_raw_fd(), move |buf| {
                    assert!(buf.iter().all(|b| *b == 0xCD));
                    got.fetch_add(buf.len(), Ordering::SeqCst);
                    Ok(())
                })
                .await
                .unwrap();
            while received.load(Ordering::SeqCst) < 64 * 1024 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            task.stop().await;
            drop(server);
            done.recv_timeout(TIMEOUT).expect("writer not rejected");
            writer.join().unwrap();
            if cycle == 0 {
                warm = vmlck_kib();
            }
        }
        let last = vmlck_kib();
        println!(
            "[stop under traffic] after first cycle {warm} KiB, after {cycles} cycles {last} KiB"
        );
        assert_eq!(
            last, warm,
            "locked arena grew across {cycles} stops under traffic"
        );
        drop(reader);
        wait_unlocked();
    }

    /// Buffers handed to the application outlive their task: they must stay
    /// intact while later tasks receive, and the arena must neither grow
    /// while they are held nor stay locked once they are released.
    async fn buffer_outlives_task() {
        let reader = Reader::new(4096, 16).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (held_tx, held_rx) = mpsc::channel::<Arc<RxBuffer>>();

        let mut client = TcpStream::connect(addr).unwrap();
        let (server, _) = listener.accept().unwrap();
        let mut task = reader
            .setup_read(server.as_raw_fd(), move |buf| {
                held_tx.send(buf).unwrap();
                Ok(())
            })
            .await
            .unwrap();
        client.write_all(&[0x11; 1000]).unwrap();
        let mut held: Vec<Arc<RxBuffer>> = Vec::new();
        while held.iter().map(|b| b.len()).sum::<usize>() < 1000 {
            held.push(held_rx.recv_timeout(TIMEOUT).unwrap());
        }
        task.stop().await;
        drop(server);
        drop(client);
        drop(held_rx);
        let warm = vmlck_kib();

        // every other buffer cycles through 32 more links carrying data
        let link = Link::new();
        for _ in 0..32 {
            link.cycle(&reader, 8).await;
        }
        assert!(
            held.iter().flat_map(|b| b.iter()).all(|b| *b == 0x11),
            "held buffers were overwritten"
        );
        assert_eq!(
            vmlck_kib(),
            warm,
            "locked arena grew while a buffer was held"
        );
        drop(held);
        link.cycle(&reader, 8).await;
        assert_eq!(vmlck_kib(), warm);
        drop(reader);
        wait_unlocked();
    }

    #[test]
    fn stopped_tasks_release_their_buffers() {
        zenoh_util::try_init_log_from_env();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            tokio::time::timeout(Duration::from_secs(120), async {
                churn(4096, 16, 24, 0).await;
                churn(4096, 16, 24, 8).await;
                // transport geometry: Uring::new(65535, 65535) -> Reader::new(65537, 16)
                churn(65537, 16, 12, 4).await;
                stop_under_traffic(24).await;
                buffer_outlives_task().await;
            })
            .await
            .expect("test timed out");
        });
    }
}
