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

use std::{
    io::Write,
    net::{TcpListener, TcpStream},
    os::fd::AsRawFd,
    sync::{atomic::AtomicUsize, Arc},
    time::Duration,
};

use libc::rand;
use zenoh_uring::reader::Reader;

pub mod common;

pub const ITERATION_COUNT: usize = 100000;

fn writer_main() {
    let addr = ("127.0.0.1", 7780);
    let mut client = TcpStream::connect(addr).unwrap();

    let mut i = 0u8;
    for _ in 0..ITERATION_COUNT {
        let size = (unsafe { rand() + 1 } % u16::MAX as i32) as u16;

        let mut data = Vec::new();
        data.reserve(size as usize + 2);
        data.write(&size.to_le_bytes()).unwrap();

        for _ in 0..size {
            data.push(i);
            i = i.wrapping_add(1);
        }

//        println!("Write {size} bytes!");
        client.write_all(&data).unwrap();
    }
}

fn reader_main() {
    let addr = ("127.0.0.1", 7780);
    let listener = TcpListener::bind(addr).unwrap();
    let (stream, _addr) = listener.accept().unwrap();

    let reader = Reader::new();

    let iteration = Arc::new(AtomicUsize::new(0));
    let c_iteration = iteration.clone();

    let mut i = 0u8;
    let _read_handle = reader
        .setup_fragmented_read(stream.as_raw_fd(), move |data| {
//            println!("Got {} bytes!", data.size());

            for cell in data.iter() {
                assert!(*cell == i);
                i = i.wrapping_add(1);
            }

            c_iteration.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();

    loop {
        if iteration.load(std::sync::atomic::Ordering::SeqCst) == ITERATION_COUNT {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn rw() {
    let _ = std::thread::spawn(reader_main);
    std::thread::sleep(Duration::from_secs(1));
    writer_main();
}
