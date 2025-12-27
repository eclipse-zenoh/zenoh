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

use std::{net::{TcpListener, TcpStream}, os::fd::AsRawFd, sync::{Arc, atomic::AtomicUsize}, time::Duration};

use io_uring::{IoUring, types};
use zenoh_uring::{BUF_SIZE, reader::Reader, writer::Writer};

use crate::common::monotonic_now_ns;

pub mod common;


fn writer_main() {
    //let addr = "/tmp/rw.sock";
    //let client = UnixStream::connect(addr).unwrap();

    let addr = ("127.0.0.1", 7777);
    let client = TcpStream::connect(addr).unwrap();
    client.set_nodelay(true).unwrap();

    let writer = Writer::new();

    //let ctr = Arc::new(AtomicUsize::new(0));

    //let c_ctr = ctr.clone();

    //    let _ = std::thread::spawn(move || {
    let mut select_latencies_accum = 0u128;
    let mut write_latencies_accum = 0u128;
    let mut times_accum = 0u128;
    loop {
        std::thread::sleep(Duration::from_micros(1000));

        let time_before_select = monotonic_now_ns();
        let mut buffer = writer.select_buffer();

        let time_after_select = monotonic_now_ns();

        let slice = buffer.as_mut();
        slice[0..16].copy_from_slice(&time_before_select.to_le_bytes());

        writer.write(
            types::Fd(client.as_raw_fd()),
            buffer,
            BUF_SIZE, //actual len
        );

        let time_after_write = monotonic_now_ns();

        select_latencies_accum += time_after_select - time_before_select;
        write_latencies_accum += time_after_write - time_after_select;
        times_accum += 1;

        if times_accum == 5000 {
            //panic!("stop");
            println!(
                "Avg latencies, ns: selection: {} ns, write: {}",
                select_latencies_accum / times_accum,
                write_latencies_accum / times_accum
            );
            select_latencies_accum = 0;
            write_latencies_accum = 0;
            times_accum = 0;
        }

        //c_ctr.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        //client.flush().unwrap();
        //std::thread::yield_now();
        //std::thread::sleep(Duration::from_micros(1000));
    }
    //    });

    //loop {
    //    let ctr = ctr.swap(0, std::sync::atomic::Ordering::SeqCst);
    //    println!("{ctr} msg*s");
    //    std::thread::sleep(Duration::from_secs(1));
    //}
}




fn reader_main() {
    //let addr = "/tmp/rw.sock";
    //std::fs::remove_file(addr);
    //let listener = std::os::unix::net::UnixListener::bind(addr)?;

    let addr = ("127.0.0.1", 7777);
    let listener = TcpListener::bind(addr).unwrap();

    let ctr = Arc::new(AtomicUsize::new(0));
    let len = Arc::new(AtomicUsize::new(0));
    let accum_latency = Arc::new(AtomicUsize::new(0));

    let c_ctr = ctr.clone();
    let c_len = len.clone();
    let c_accum_latency = accum_latency.clone();

    let (mut stream, _addr) = listener.accept().unwrap();
    stream.set_nodelay(true).unwrap();
    
    
    /// io_uring read
    let ring = Arc::new(
        IoUring::builder()
            .setup_submit_all()
            //.setup_sqpoll(1)
            //.setup_iopoll()
            //.setup_sqpoll_cpu(0)
            .setup_coop_taskrun()
            //.setup_defer_taskrun()
            //.setup_single_issuer()
            .build((1024).try_into().unwrap()).unwrap(),
    );

    let reader = Reader::new(ring);

    reader.setup_read(stream.as_raw_fd(), move |data| {
        //assert!(data.len() == BUF_SIZE);

        let time = monotonic_now_ns();
        let restored = u128::from_le_bytes(data[0..16].try_into().unwrap());

        let latency = (time - restored) as usize;
        //println!("latency: {latency} ns");

        c_ctr.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        c_len.fetch_add(data.len(), std::sync::atomic::Ordering::SeqCst);
        c_accum_latency.fetch_add(latency, std::sync::atomic::Ordering::SeqCst);
    });

    
/*    
    // standard read
    let _ = std::thread::spawn(move || {

        let mut arr = [0u8; BUF_SIZE];
        loop {
            std::io::Read::read_exact(&mut stream, &mut arr).unwrap();
            let time = monotonic_now_ns();
            let restored = u128::from_le_bytes(arr[0..16].try_into().unwrap());

            let latency = (time - restored) as usize;
            //println!("latency: {latency} ns");

            c_ctr.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            c_len.fetch_add(arr.len(), std::sync::atomic::Ordering::SeqCst);
            c_accum_latency.fetch_add(latency, std::sync::atomic::Ordering::SeqCst);
        }
    });
    */

    loop {
        let ctr = ctr.swap(0, std::sync::atomic::Ordering::SeqCst);
        let len = len.swap(0, std::sync::atomic::Ordering::SeqCst);
        let mean_latency = accum_latency.swap(0, std::sync::atomic::Ordering::SeqCst) / ctr.max(1);

        let full_msg_count = len / BUF_SIZE;
        let mbps = (len as f64) / (1024.0 * 1024.0);
        let avg_fragments = ctr as f64 / full_msg_count.max(1) as f64;

        println!("{full_msg_count} msg*s ({mbps} MB*s), mean lat: {mean_latency} ns, avg frag: {avg_fragments:.4}");
        std::thread::sleep(Duration::from_secs(1));
    }
}





#[test]
fn rw() {
    let _ = std::thread::spawn(reader_main);
    std::thread::sleep(Duration::from_secs(1));
    writer_main();
}
