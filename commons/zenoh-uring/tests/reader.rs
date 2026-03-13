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
    net::{TcpListener, TcpStream, ToSocketAddrs},
    os::fd::AsRawFd,
    sync::{
        atomic::{AtomicBool, AtomicUsize},
        Arc,
    },
    time::Duration,
};

use libc::rand;
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

    fn wait_for_comlete(mut self) -> ZResult<()> {
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
    pub fn make(port: u16, iteration_count: usize, interval: Option<Duration>) -> ManagedTask {
        let addr = ("127.0.0.1", port);

        let finished = Arc::new(AtomicBool::new(false));

        let c_finished = finished.clone();
        let handle = Some(std::thread::spawn(move || {
            Self::writer_main(addr, iteration_count, interval, c_finished)
        }));

        ManagedTask::new(handle, finished)
    }

    fn writer_main<A: ToSocketAddrs>(
        addr: A,
        iteration_count: usize,
        interval: Option<Duration>,
        finished: Arc<AtomicBool>,
    ) -> ZResult<()> {
        let mut client = loop {
            if let Ok(client) = TcpStream::connect(&addr) {
                break client;
            }

            if finished.load(std::sync::atomic::Ordering::Relaxed) {
                bail!("unable to connect!");
            }
        };

        let mut i = 0u8;
        for _ in 0..iteration_count {
            let size =
                ((unsafe { rand().unsigned_abs() } % u16::MAX as u32) as u16).saturating_add(1);

            let mut data = Vec::with_capacity(size as usize + 2);
            data.write_all(&size.to_le_bytes())?;

            for _ in 0..size {
                data.push(i);
                i = i.wrapping_add(1);
            }

            //        println!("Write {size} bytes!");
            client.write_all(&data)?;

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

struct ReaderTask;

impl ReaderTask {
    pub fn make(reader: Reader, port: u16, iteration_count: usize) -> ManagedTask {
        let addr = ("127.0.0.1", port);

        let finished = Arc::new(AtomicBool::new(false));

        let c_finished = finished.clone();
        let handle = Some(std::thread::spawn(move || {
            Self::reader_main(reader, addr, iteration_count, c_finished)
        }));
        std::thread::sleep(Duration::from_millis(100));

        ManagedTask::new(handle, finished)
    }

    fn reader_main<A: ToSocketAddrs>(
        reader: Reader,
        addr: A,
        iteration_count: usize,
        finished: Arc<AtomicBool>,
    ) -> ZResult<()> {
        let listener = TcpListener::bind(addr)?;
        let (stream, _addr) = listener.accept()?;

        let iteration = Arc::new(AtomicUsize::new(0));
        let c_iteration = iteration.clone();

        let mut i = 0u8;
        let read_handle = reader
            .setup_fragmented_read(stream.as_raw_fd(), move |data| {
                //            println!("Got {} bytes!", data.size());

                if data.size() == 0 {
                    println!("Unexpected size: {}", data.size());
                    assert!(data.size() != 0);
                }

                for cell in data.iter() {
                    assert!(*cell == i);
                    i = i.wrapping_add(1);
                }

                c_iteration.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

                Ok(())
            })
            .unwrap();

        assert!(Arc::strong_count(&iteration) == 2);

        while !finished.load(std::sync::atomic::Ordering::Relaxed)
            && iteration.load(std::sync::atomic::Ordering::SeqCst) != iteration_count
        {
            std::thread::sleep(Duration::from_millis(100));
        }

        drop(read_handle);

        std::thread::sleep(Duration::from_millis(100));

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
    ) -> ManagedTask {
        let finished = Arc::new(AtomicBool::new(false));

        let c_finished = finished.clone();
        let handle = Some(std::thread::spawn(move || {
            Self::rw_main(reader, port, iteration_count, interval, c_finished)
        }));

        ManagedTask::new(handle, finished)
    }

    fn rw_main(
        reader: Reader,
        port: u16,
        iteration_count: usize,
        interval: Option<Duration>,
        finished: Arc<AtomicBool>,
    ) -> ZResult<()> {
        let reader = ReaderTask::make(reader, port, iteration_count);
        let writer = WriterTask::make(port, iteration_count, interval);

        while !finished.load(std::sync::atomic::Ordering::Relaxed)
            && !reader.poll_comlete()?
            && !writer.poll_comlete()?
        {
            std::thread::sleep(Duration::from_millis(100));
        }

        if finished.load(std::sync::atomic::Ordering::Relaxed) {
            writer.interrupt();
        }
        writer.wait_for_comlete()?;

        if finished.load(std::sync::atomic::Ordering::Relaxed) {
            reader.interrupt();
        }
        reader.wait_for_comlete()
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
                )
            } else {
                Self::start_stop_main_reader_ends_first(
                    reader_fn,
                    port,
                    iteration_count,
                    interval,
                    start_stop_count,
                    c_finished,
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
    ) -> ZResult<()> {
        for _ in 0..start_stop_count {
            let rw = RWTask::make(reader_fn(), port, iteration_count, interval);

            while !finished.load(std::sync::atomic::Ordering::Relaxed) && !rw.poll_comlete()? {
                std::thread::sleep(Duration::from_millis(100));
            }

            if finished.load(std::sync::atomic::Ordering::Relaxed) {
                rw.interrupt();
                rw.wait_for_comlete()?;
                break;
            } else {
                rw.wait_for_comlete()?;
            }
        }
        Ok(())
    }

    fn start_stop_main_reader_ends_first<F: Fn() -> Reader>(
        reader_fn: F,
        port: u16,
        _iteration_count: usize,
        interval: Option<Duration>,
        start_stop_count: usize,
        finished: Arc<AtomicBool>,
    ) -> ZResult<()> {
        for _ in 0..start_stop_count {
            let reader = ReaderTask::make(reader_fn(), port, usize::MAX);
            let writer = WriterTask::make(port, usize::MAX, interval);
            std::thread::sleep(Duration::from_millis(100));
            reader.interrupt();
            reader.wait_for_comlete()?;

            while !finished.load(std::sync::atomic::Ordering::Relaxed) && !writer.poll_comlete()? {
                std::thread::sleep(Duration::from_millis(100));
            }

            if finished.load(std::sync::atomic::Ordering::Relaxed) {
                writer.interrupt();
                let _ = writer.wait_for_comlete();
                break;
            } else {
                let _ = writer.wait_for_comlete();
            }
        }
        Ok(())
    }
}

#[test]
fn rw_single() {
    zenoh_util::try_init_log_from_env();

    let reader = Reader::new(65535 + 2, 16).unwrap();

    let port = 7780;
    let interval = None;
    let reader = ReaderTask::make(reader, port, ITERATION_COUNT);
    let writer = WriterTask::make(port, ITERATION_COUNT, interval);

    writer.wait_for_comlete().unwrap();
    reader.wait_for_comlete().unwrap();
}

#[test]
fn rw_parallel() {
    zenoh_util::try_init_log_from_env();

    let reader = Reader::new(65535 + 2, 16).unwrap();
    let count = 10;
    let base_port = 7781;
    let base_interval = None;

    let mut rw_pairs = vec![];
    for pair in 0..count {
        let reader = ReaderTask::make(
            reader.clone(),
            base_port + pair,
            ITERATION_COUNT / count as usize,
        );
        let writer = WriterTask::make(
            base_port + pair,
            ITERATION_COUNT / count as usize,
            base_interval,
        );
        rw_pairs.push((reader, writer));
    }

    for (reader, writer) in rw_pairs {
        writer.wait_for_comlete().unwrap();
        reader.wait_for_comlete().unwrap();
    }
}

#[test]
fn rw_parallel_and_start_stop() {
    zenoh_util::try_init_log_from_env();

    let reader = Reader::new(65535 + 2, 16).unwrap();
    let count = 1;
    let base_port = 7791;
    let base_interval = None; //Some(Duration::from_millis(1));

    let mut rw_background = vec![];
    for i in 0..count {
        let rw = RWTask::make(reader.clone(), base_port + i, usize::MAX, base_interval);
        rw_background.push(rw);
    }

    let run_start_stop_session = |reader_fn: Arc<dyn Fn() -> Reader + Send + Sync>| {
        tracing::info!("start-stop begin (writer ends first)...");
        let c_reader_fn = reader_fn.clone();
        let start_stop_writer_ends_first = StartStopTask::make(
            move || c_reader_fn(),
            base_port + count,
            ITERATION_COUNT / 1000,
            None,
            100,
            true,
        );
        start_stop_writer_ends_first.wait_for_comlete().unwrap();
        tracing::info!("start-stop done(writer ends first)!");

        tracing::info!("start-stop begin (reader ends first)...");
        let c_reader_fn = reader_fn.clone();
        let start_stop_reader_ends_first = StartStopTask::make(
            move || c_reader_fn(),
            base_port + count,
            ITERATION_COUNT / 1000,
            None,
            100,
            false,
        );
        start_stop_reader_ends_first.wait_for_comlete().unwrap();
        tracing::info!("start-stop done(reader ends first)!");
    };

    tracing::info!("Shared Reader...");
    run_start_stop_session(Arc::new(move || reader.clone()));
    tracing::info!("Exclusive Reader...");
    run_start_stop_session(Arc::new(|| Reader::new(65535 + 2, 16).unwrap()));

    // stop background tasks
    tracing::info!("Stopping background tasks...");
    for rw in rw_background {
        drop(rw);
    }
}
