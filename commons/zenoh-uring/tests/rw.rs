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

#[cfg(target_os = "linux")]
mod linux_tests {
    use std::{
        io::Write,
        net::{TcpListener, TcpStream},
        os::fd::AsRawFd,
        sync::{atomic::AtomicUsize, Arc},
        time::Duration,
    };

    use zenoh_uring::api::{reader::Reader, types::BufferCount};

    pub const BUF_SIZE: usize = 65537;
    pub const BUF_COUNT: BufferCount = 4;

    pub fn monotonic_now_ns() -> u128 {
        unsafe {
            let mut ts: libc::timespec = std::mem::zeroed();
            libc::clock_gettime(libc::CLOCK_MONOTONIC_RAW, &mut ts);
            (ts.tv_sec as u128) * 1_000_000_000 + (ts.tv_nsec as u128)
        }
    }

    fn writer_main() {
        let addr = ("127.0.0.1", 7777);
        let mut client = TcpStream::connect(addr).unwrap();
        client.set_nodelay(true).unwrap();

        const SIZE: usize = BUF_SIZE;

        let mut arr = [0u8; SIZE];
        let length = (SIZE - 2) as u16;
        arr[0..2].copy_from_slice(&length.to_le_bytes());
        loop {
            let time = monotonic_now_ns();
            arr[2..18].copy_from_slice(&time.to_le_bytes());
            client.write_all(&arr).unwrap();
        }
    }

    fn reader_main() {
        let addr = ("127.0.0.1", 7777);
        let listener = TcpListener::bind(addr).unwrap();

        let ctr = Arc::new(AtomicUsize::new(0));
        let len = Arc::new(AtomicUsize::new(0));
        let accum_latency = Arc::new(AtomicUsize::new(0));

        let c_ctr = ctr.clone();
        let c_len = len.clone();
        let c_accum_latency = accum_latency.clone();

        let (stream, _addr) = listener.accept().unwrap();
        stream.set_nodelay(true).unwrap();

        let reader = Reader::new(BUF_SIZE, BUF_COUNT).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(100));

        let _read_handle = zenoh_runtime::ZRuntime::Application
            .block_on(
                reader.setup_fragmented_read(stream.as_raw_fd(), move |data| {
                    let time = monotonic_now_ns();

                    let bytes = data
                        .iter()
                        .take(16)
                        .enumerate()
                        .try_fold([0u8; 16], |mut acc, (i, b)| {
                            acc[i] = *b;
                            Ok::<_, ()>(acc)
                        })
                        .ok()
                        .unwrap(); // unwrap if you know there are 2 bytes

                    let restored = u128::from_le_bytes(bytes);

                    let latency = (time - restored) as usize;

                    c_ctr.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    c_len.fetch_add(data.size(), std::sync::atomic::Ordering::SeqCst);
                    c_accum_latency.fetch_add(latency, std::sync::atomic::Ordering::SeqCst);

                    Ok(())
                }),
            )
            .unwrap();

        loop {
            let ctr = ctr.swap(0, std::sync::atomic::Ordering::SeqCst);
            let len = len.swap(0, std::sync::atomic::Ordering::SeqCst);
            let mean_latency =
                accum_latency.swap(0, std::sync::atomic::Ordering::SeqCst) / ctr.max(1);

            let full_msg_count = ctr;
            let mbps = (len as f64) / (1024.0 * 1024.0);
            let avg_fragments = ctr as f64 / full_msg_count.max(1) as f64;

            println!("{full_msg_count} msg*s ({mbps} MB*s), mean lat: {mean_latency} ns, avg frag: {avg_fragments:.4}");
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    #[ignore]
    #[test]
    fn rw() {
        zenoh_util::try_init_log_from_env();
        let _ = std::thread::spawn(reader_main);
        std::thread::sleep(Duration::from_secs(1));
        writer_main();
    }
}
