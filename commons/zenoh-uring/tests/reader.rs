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
    net::{TcpListener, TcpStream, ToSocketAddrs, UdpSocket},
    os::fd::{AsRawFd, RawFd},
    sync::{
        atomic::{AtomicBool, AtomicUsize},
        Arc,
    },
    time::Duration,
};

use libc::rand;
use test_case::test_case;
use zenoh_core::bail;
use zenoh_result::ZResult;
use zenoh_uring::reader::Reader;

pub mod common;

pub const ITERATION_COUNT: usize = 10000;

struct ManagedTask {
    handle: Option<std::thread::JoinHandle<ZResult<()>>>,
    finished: Arc<AtomicBool>,
}

impl ManagedTask {
    fn new(
        handle: Option<std::thread::JoinHandle<ZResult<()>>>,
        finished: Arc<AtomicBool>,
    ) -> Self {
        Self { handle, finished }
    }

    fn wait_for_complete(mut self) -> ZResult<()> {
        self.handle.take().unwrap().join().unwrap()
    }

    fn poll_comlete(&self) -> ZResult<bool> {
        match &self.handle {
            Some(val) => Ok(val.is_finished()),
            None => bail!("Already joined"),
        }
    }

    fn interrupt(&self) {
        self.finished
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Drop for ManagedTask {
    fn drop(&mut self) {
        self.interrupt();
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap().unwrap();
        }
    }
}

struct WriterTask;

impl WriterTask {
    pub fn make(
        port: u16,
        iteration_count: usize,
        interval: Option<Duration>,
        is_tcp: bool,
    ) -> ManagedTask {
        let addr = ("127.0.0.1", port);

        let finished = Arc::new(AtomicBool::new(false));

        let c_finished = finished.clone();
        let handle = Some(std::thread::spawn(move || {
            Self::writer_main(addr, iteration_count, interval, c_finished, is_tcp)
        }));

        ManagedTask::new(handle, finished)
    }

    fn writer_main<A: ToSocketAddrs>(
        addr: A,
        iteration_count: usize,
        interval: Option<Duration>,
        finished: Arc<AtomicBool>,
        is_tcp: bool,
    ) -> ZResult<()> {
        let (_fd, mut socket, _) = loop {
            if let Ok(connected) = connect_socket(is_tcp, &addr) {
                break connected;
            }
            tracing::error!("Unable to connect to socket!");
            if finished.load(std::sync::atomic::Ordering::Relaxed) {
                bail!("unable to connect!");
            }
        };

        let max_size = if is_tcp { 65535u32 } else { 65000 };

        let iterations = {
            if is_tcp {
                iteration_count
            } else {
                usize::MAX
            }
        };

        let mut i = 0u8;
        for _ in 0..iterations {
            let size =
                std::cmp::max(1, std::cmp::min(unsafe { rand().unsigned_abs() }, max_size)) as u16;

            let mut data = Vec::with_capacity(size as usize + 2);
            data.write_all(&size.to_le_bytes())?;

            for _ in 0..size {
                data.push(i);
                i = i.wrapping_add(1);
            }

            let result = socket._write_all(&data);
            if is_tcp {
                result?;
            }

            if finished.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }

            if let Some(interval) = &interval {
                std::thread::sleep(*interval);
            }
        }

        Ok(())
    }
}

trait TestSocket: AsRawFd {
    fn _write_all(&mut self, buf: &[u8]) -> std::io::Result<()>;
}

impl TestSocket for TcpStream {
    fn _write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        self.write_all(buf)
    }
}
impl TestSocket for UdpSocket {
    fn _write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        self.send(buf).map(|val| {
            assert_eq!(buf.len(), val);
        })
    }
}

fn connect_socket<A: ToSocketAddrs>(
    is_tcp: bool,
    remote_addr: A,
) -> std::io::Result<(RawFd, Box<dyn TestSocket>, Option<Box<dyn AsRawFd>>)> {
    if is_tcp {
        // TCP: directly connect to the remote address
        let stream = TcpStream::connect(remote_addr)?;
        let fd = stream.as_raw_fd();
        Ok((fd, Box::new(stream), None))
    } else {
        // UDP: bind to an ephemeral local port, then set the default remote address
        let socket = UdpSocket::bind("0.0.0.0:0")?; // any interface, OS‑assigned port
        socket.connect(remote_addr)?; // sets the remote for send/recv
        let fd = socket.as_raw_fd();
        Ok((fd, Box::new(socket), None))
    }
}

fn bind_socket<A: ToSocketAddrs>(
    is_tcp: bool,
    addr: A,
) -> std::io::Result<(Box<dyn AsRawFd>, Option<Box<dyn AsRawFd>>)> {
    if is_tcp {
        let listener = TcpListener::bind(addr)?;
        let (stream, _) = listener.accept()?;
        Ok((Box::new(stream), Some(Box::new(listener))))
    } else {
        let socket = UdpSocket::bind(addr)?;
        Ok((Box::new(socket), None))
    }
}

struct ReaderTask;

impl ReaderTask {
    pub fn make(reader: Reader, port: u16, iteration_count: usize, is_tcp: bool) -> ManagedTask {
        let addr = ("127.0.0.1", port);

        let finished = Arc::new(AtomicBool::new(false));

        let c_finished = finished.clone();
        let handle = Some(std::thread::spawn(move || {
            Self::reader_main(reader, addr, iteration_count, c_finished, is_tcp)
        }));
        std::thread::sleep(Duration::from_millis(100));

        ManagedTask::new(handle, finished)
    }

    fn reader_main<A: ToSocketAddrs>(
        reader: Reader,
        addr: A,
        iteration_count: usize,
        finished: Arc<AtomicBool>,
        is_tcp: bool,
    ) -> ZResult<()> {
        let (socket, _) = bind_socket(is_tcp, addr)?;

        let iteration = Arc::new(AtomicUsize::new(0));
        let c_iteration = iteration.clone();

        let mut i = 0u8;

        let mut read_handle = zenoh_runtime::ZRuntime::Application.block_on(
            reader.setup_fragmented_read(socket.as_raw_fd(), move |data| {
                //            println!("Got {} bytes!", data.size());

                if data.size() == 0 {
                    println!("Unexpected size: {}", data.size());
                    assert!(data.size() != 0);
                }

                if is_tcp {
                    for cell in data.iter() {
                        assert!(*cell == i);
                        i = i.wrapping_add(1);
                    }
                }

                c_iteration.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

                Ok(())
            }),
        )?;

        assert!(Arc::strong_count(&iteration) == 2);

        while read_handle.try_read_error_sync().is_err()
            && !finished.load(std::sync::atomic::Ordering::Relaxed)
            && iteration.load(std::sync::atomic::Ordering::SeqCst) < iteration_count
        {
            std::thread::sleep(Duration::from_millis(100));
        }

        drop(read_handle);

        // wait for strong_count to become exclusive which means ReadTask dropped it's callback
        for _ in 0..100 {
            if Arc::strong_count(&iteration) == 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(Arc::strong_count(&iteration) == 1);

        Ok(())
    }
}

struct RWTask;

impl RWTask {
    pub fn make(
        reader: Reader,
        port: u16,
        iteration_count: usize,
        interval: Option<Duration>,
        is_tcp: bool,
    ) -> ManagedTask {
        let finished = Arc::new(AtomicBool::new(false));

        let c_finished = finished.clone();
        let handle = Some(std::thread::spawn(move || {
            Self::rw_main(reader, port, iteration_count, interval, c_finished, is_tcp)
        }));

        ManagedTask::new(handle, finished)
    }

    fn rw_main(
        reader: Reader,
        port: u16,
        iteration_count: usize,
        interval: Option<Duration>,
        finished: Arc<AtomicBool>,
        is_tcp: bool,
    ) -> ZResult<()> {
        let reader = ReaderTask::make(reader, port, iteration_count, is_tcp);
        let writer = WriterTask::make(port, iteration_count, interval, is_tcp);

        while !finished.load(std::sync::atomic::Ordering::Relaxed)
            && !reader.poll_comlete()?
            && !writer.poll_comlete()?
        {
            std::thread::sleep(Duration::from_millis(100));
        }

        if finished.load(std::sync::atomic::Ordering::Relaxed) {
            reader.interrupt();
        }
        reader.wait_for_complete()?;

        if !is_tcp || finished.load(std::sync::atomic::Ordering::Relaxed) {
            writer.interrupt();
            let _ = writer.wait_for_complete();
            Ok(())
        } else {
            writer.wait_for_complete()
        }
    }
}

struct StartStopTask;

impl StartStopTask {
    pub fn make<F: Fn() -> Reader + Send + 'static>(
        reader_fn: F,
        port: u16,
        iteration_count: usize,
        interval: Option<Duration>,
        start_stop_count: usize,
        writer_ends_first: bool,
        is_tcp: bool,
    ) -> ManagedTask {
        let finished = Arc::new(AtomicBool::new(false));

        let c_finished = finished.clone();
        let handle = Some(std::thread::spawn(move || {
            if writer_ends_first {
                Self::start_stop_main_writer_ends_first(
                    reader_fn,
                    port,
                    iteration_count,
                    interval,
                    start_stop_count,
                    c_finished,
                    is_tcp,
                )
            } else {
                Self::start_stop_main_reader_ends_first(
                    reader_fn,
                    port,
                    iteration_count,
                    interval,
                    start_stop_count,
                    c_finished,
                    is_tcp,
                )
            }
        }));

        ManagedTask::new(handle, finished)
    }

    fn start_stop_main_writer_ends_first<F: Fn() -> Reader>(
        reader_fn: F,
        port: u16,
        iteration_count: usize,
        interval: Option<Duration>,
        start_stop_count: usize,
        finished: Arc<AtomicBool>,
        is_tcp: bool,
    ) -> ZResult<()> {
        for _ in 0..start_stop_count {
            let rw = RWTask::make(reader_fn(), port, iteration_count, interval, is_tcp);

            while !finished.load(std::sync::atomic::Ordering::Relaxed) && !rw.poll_comlete()? {
                std::thread::sleep(Duration::from_millis(100));
            }

            if finished.load(std::sync::atomic::Ordering::Relaxed) {
                rw.interrupt();
                rw.wait_for_complete()?;
                break;
            } else {
                rw.wait_for_complete()?;
            }
        }
        Ok(())
    }

    fn start_stop_main_reader_ends_first<F: Fn() -> Reader>(
        reader_fn: F,
        port: u16,
        iteration_count: usize,
        interval: Option<Duration>,
        start_stop_count: usize,
        finished: Arc<AtomicBool>,
        is_tcp: bool,
    ) -> ZResult<()> {
        for _ in 0..start_stop_count {
            let reader = ReaderTask::make(reader_fn(), port, iteration_count, is_tcp);
            let writer = WriterTask::make(port, usize::MAX, interval, is_tcp);

            while !finished.load(std::sync::atomic::Ordering::Relaxed)
                && !reader.poll_comlete()?
            {
                std::thread::sleep(Duration::from_millis(100));
            }

            if reader.poll_comlete()? {
                reader.wait_for_complete()?;
            } else {
                // drop reader first!
                drop(reader);
                break;
            }

            if !is_tcp {
                writer.interrupt();
            }

            while !finished.load(std::sync::atomic::Ordering::Relaxed) && !writer.poll_comlete()? {
                std::thread::sleep(Duration::from_millis(100));
            }

            if finished.load(std::sync::atomic::Ordering::Relaxed) {
                writer.interrupt();
                let _ = writer.wait_for_complete();
                break;
            } else {
                let _ = writer.wait_for_complete();
            }
        }
        Ok(())
    }
}

#[test_case(true; "tcp")]
#[test_case(false; "udp")]
fn rw_single(is_tcp: bool) {
    zenoh_util::try_init_log_from_env();

    let reader = Reader::new(65535 + 2, 16).unwrap();

    let port = 7780;
    let interval = None;
    let reader = ReaderTask::make(reader, port, ITERATION_COUNT, is_tcp);
    let writer = WriterTask::make(port, ITERATION_COUNT, interval, is_tcp);

    reader.wait_for_complete().unwrap();

    if !is_tcp {
        writer.interrupt();
    }
    writer.wait_for_complete().unwrap();
}

#[test_case(true; "tcp")]
#[test_case(false; "udp")]
fn rw_single_pause_after_interrupt(is_tcp: bool) {
    zenoh_util::try_init_log_from_env();

    let reader = Reader::new(65535 + 2, 16).unwrap();

    let port = 7781;
    let interval = None;
    let reader = ReaderTask::make(reader, port, ITERATION_COUNT, is_tcp);
    let writer = WriterTask::make(port, ITERATION_COUNT, interval, is_tcp);

    reader.wait_for_complete().unwrap();

    std::thread::sleep(Duration::from_secs(1));

    if !is_tcp {
        writer.interrupt();
    }
    writer.wait_for_complete().unwrap();
}

#[test_case(true; "tcp")]
#[test_case(false; "udp")]
fn rw_parallel(is_tcp: bool) {
    zenoh_util::try_init_log_from_env();

    let reader = Reader::new(65535 + 2, 16).unwrap();
    let count = 64;
    let base_port = 7782;
    let base_interval = None;

    let mut rw_tasks = vec![];
    for pair in 0..count {
        let rw = RWTask::make(
            reader.clone(),
            base_port + pair,
            ITERATION_COUNT / count as usize,
            base_interval,
            is_tcp,
        );
        rw_tasks.push(rw);
    }

    for rw in rw_tasks {
        rw.wait_for_complete().unwrap();
    }
}

#[test_case(true,  true,  true, 0;  "tcp_writer_first_shared_reader")]
#[test_case(true,  true,  false, 1; "tcp_writer_first_exclusive_reader")]
#[test_case(true,  false, true, 2;  "tcp_reader_first_shared_reader")]
#[test_case(true,  false, false, 3; "tcp_reader_first_exclusive_reader")]
#[test_case(false, true,  true, 4;  "udp_writer_first_shared_reader")]
#[test_case(false, true,  false, 5; "udp_writer_first_exclusive_reader")]
#[test_case(false, false, true, 6;  "udp_reader_first_shared_reader")]
#[test_case(false, false, false, 7; "udp_reader_first_exclusive_reader")]
fn rw_parallel_and_start_stop(
    is_tcp: bool,
    writer_ends_first: bool,
    shared_reader: bool,
    port_offset: u16,
) {
    zenoh_util::try_init_log_from_env();

    let reader = Reader::new(65535 + 2, 16).unwrap();
    let count = 2;
    let base_port = 7782 + 64 + port_offset * (count + 1);
    let base_interval = None; //Some(Duration::from_millis(1));

    let mut rw_background = vec![];
    for i in 0..count {
        let rw = RWTask::make(
            reader.clone(),
            base_port + i,
            usize::MAX,
            base_interval,
            is_tcp,
        );
        rw_background.push(rw);
    }

    let run_start_stop_session = |reader_fn: Arc<dyn Fn() -> Reader + Send + Sync>| {
        let c_reader_fn = reader_fn.clone();
        let start_stop_writer_ends_first = StartStopTask::make(
            move || c_reader_fn(),
            base_port + count,
            ITERATION_COUNT / 1000,
            None,
            100,
            writer_ends_first,
            is_tcp,
        );
        start_stop_writer_ends_first.wait_for_complete().unwrap();
    };

    if shared_reader {
        tracing::info!("Shared Reader...");
        run_start_stop_session(Arc::new(move || reader.clone()));
    } else {
        tracing::info!("Exclusive Reader...");
        run_start_stop_session(Arc::new(|| Reader::new(65535 + 2, 16).unwrap()));
    }

    // stop background tasks
    tracing::info!("Stopping background tasks...");
    for rw in rw_background {
        drop(rw);
    }
}
