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

use nix::sys::eventfd::{EfdFlags, EventFd};
use zenoh_result::ZResult;

use std::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut, Neg},
    os::fd::{AsRawFd, RawFd},
    sync::{atomic::AtomicBool, mpsc::Sender, Arc},
};

use io_uring::{opcode, squeue::Flags, types, IoUring, SubmissionQueue};

use crate::{batch_arena::BatchArena, BUF_SIZE};

struct RxCallback {
    pub callback: UnsafeCell<Box<dyn FnMut(Arc<RxBuffer>) + 'static>>,
}

impl RxCallback {
    fn new(callback: UnsafeCell<Box<dyn FnMut(Arc<RxBuffer>) + 'static>>) -> Self {
        Self { callback }
    }
}

unsafe impl Send for RxCallback {}
unsafe impl Sync for RxCallback {}

struct Rx {
    pub callback: RxCallback,
    pub fd: RawFd,
}

impl Rx {
    fn new(callback: Box<dyn FnMut(Arc<RxBuffer>)>, fd: RawFd) -> Self {
        Self {
            callback: RxCallback::new(UnsafeCell::new(callback)),
            fd,
        }
    }
}

pub struct ReadTask {
    _rx: Arc<Rx>,
    user_data: u64,
    inner: Arc<ReaderInner>,
}

impl Drop for ReadTask {
    fn drop(&mut self) {
        let event = opcode::AsyncCancel::new(self.user_data).build();

        let _ = self.inner.submitter.submit(event);
    }
}

impl ReadTask {
    fn new<Cb>(fd: RawFd, callback: Cb, inner: Arc<ReaderInner>) -> ZResult<Self>
    where
        Cb: FnMut(Arc<RxBuffer>) + 'static,
    {
        let rx = Arc::new(Rx::new(Box::new(callback), fd.clone()));
        let user_data = unsafe { std::mem::transmute(rx.clone()) };

        let recv = opcode::RecvMulti::new(types::Fd(fd), 0)
            .build()
            .user_data(user_data);

        inner.submitter.submit(recv)?;

        Ok(Self {
            _rx: rx,
            user_data,
            inner,
        })
    }
}

#[derive(Debug, Clone)]
struct SubmissionIface {
    waker: Arc<EventFd>,
    sender: Sender<io_uring::squeue::Entry>,
}

impl SubmissionIface {
    fn new(waker: Arc<EventFd>, sender: Sender<io_uring::squeue::Entry>) -> Self {
        Self { waker, sender }
    }

    fn submit(&self, entry: io_uring::squeue::Entry) -> ZResult<()> {
        self.sender.send(entry).map_err(|e| e.to_string())?;
        self.wake_reader_thread();
        Ok(())
    }

    fn wake_reader_thread(&self) {
        // A write() to an eventfd can (rarely) block if the internal counter is near overflow.
        // With non-blocking mode (EfdFlags::EFD_NONBLOCK), you’ll get EAGAIN instead of risking
        // a hang during shutdown logic.
        // TODO: handle EAGAIN or switch to blocking mode
        self.waker.write(1).unwrap();
    }
}

pub struct ReaderInner {
    submitter: SubmissionIface,
    exit_flag: Arc<AtomicBool>,
}

impl ReaderInner {
    fn new(submitter: SubmissionIface, exit_flag: Arc<AtomicBool>) -> Self {
        Self {
            submitter,
            exit_flag,
        }
    }
}

impl Drop for ReaderInner {
    fn drop(&mut self) {
        self.exit_flag
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.submitter.wake_reader_thread();
    }
}

#[derive(Debug)]
pub struct FragmentedBatch {
    size: usize,
    data_offset: usize,
    buffers: Vec<Arc<RxBuffer>>,
}

impl FragmentedBatch {
    pub fn iter(&self) -> impl Iterator<Item = &u8> {
        self.buffers
            .iter()
            .flat_map(|inner| inner.iter())
            .skip(self.data_offset)
            .take(self.size)
    }

    pub fn size(&self) -> usize {
        self.size
    }
}

#[derive(Debug)]
struct BatchAccumulator {
    accumulated_size: usize,
    batch: FragmentedBatch,
}

impl BatchAccumulator {
    fn new(accumulated_size: usize, batch: FragmentedBatch) -> Self {
        Self {
            accumulated_size,
            batch,
        }
    }
}

#[derive(Debug)]
enum RxWindowState {
    Initial,
    SizeFragmented(u8),
    Accumulating(BatchAccumulator),
}

#[derive(Debug)]
pub struct RxWindow {
    state: RxWindowState,
}

impl Default for RxWindow {
    fn default() -> Self {
        Self {
            state: RxWindowState::Initial,
        }
    }
}

macro_rules! log {
    ($level:expr, $($arg:tt)*) => {
        //eprintln!("[{}] {}:{} - {}",
        //    $level,
        //    file!(),
        //    line!(),
        //    format_args!($($arg)*));
    };
}

impl RxWindow {
    fn push<F>(&mut self, buffer: Arc<RxBuffer>, on_batch: &mut F)
    where
        F: FnMut(FragmentedBatch),
    {
        match &mut self.state {
            RxWindowState::Initial => {
                let mut leftover = buffer.len() as usize;
                let mut pos = 0;

                while leftover > 0 {
                    // buffer contains size fragment
                    if leftover == 1 {
                        self.state = RxWindowState::SizeFragmented(buffer[pos]);
                        break;
                    }

                    let size =
                        u16::from_le_bytes(buffer[pos..pos + 2].try_into().unwrap()) as usize;
                    pos += 2;
                    leftover -= 2;

                    // only size
                    if leftover == 0 {
                        let fragment = FragmentedBatch {
                            size,
                            data_offset: 0,
                            buffers: vec![],
                        };
                        let accumulator = BatchAccumulator::new(leftover, fragment);
                        self.state = RxWindowState::Accumulating(accumulator);
                        break;
                    }

                    // buffer contains batch fragment
                    if leftover < size {
                        let fragment = FragmentedBatch {
                            size,
                            data_offset: pos,
                            buffers: vec![buffer],
                        };
                        let accumulator = BatchAccumulator::new(leftover, fragment);
                        self.state = RxWindowState::Accumulating(accumulator);
                        break;
                    }

                    // buffer contains at least one more batch
                    let batch = FragmentedBatch {
                        size,
                        data_offset: pos,
                        buffers: vec![buffer.clone()],
                    };
                    log!("INFO", "on_batch");
                    on_batch(batch);
                    pos += size;
                    leftover -= size;
                }
            }
            RxWindowState::SizeFragmented(size_fragment) => {
                // defragment size
                let mut size = u16::from_le_bytes([*size_fragment, buffer[0]]) as usize;
                let mut leftover = buffer.len() as usize - 1;
                let mut pos = 1;

                loop {
                    // only size
                    if leftover == 0 {
                        let fragment = FragmentedBatch {
                            size,
                            data_offset: 0,
                            buffers: vec![],
                        };
                        let accumulator = BatchAccumulator::new(leftover, fragment);
                        self.state = RxWindowState::Accumulating(accumulator);
                        break;
                    }

                    // buffer contains batch fragment
                    if leftover < size {
                        let fragment = FragmentedBatch {
                            size,
                            data_offset: pos,
                            buffers: vec![buffer],
                        };
                        let accumulator = BatchAccumulator::new(leftover, fragment);
                        self.state = RxWindowState::Accumulating(accumulator);
                        break;
                    }

                    // buffer contains at least one more batch
                    let batch = FragmentedBatch {
                        size,
                        data_offset: pos,
                        buffers: vec![buffer.clone()],
                    };
                    log!("INFO", "on_batch");
                    on_batch(batch);
                    pos += size;
                    leftover -= size;

                    // buffer ends with some exact number of batches
                    if leftover == 0 {
                        self.state = RxWindowState::Initial;
                        break;
                    }

                    // buffer contains size fragment
                    if leftover == 1 {
                        self.state = RxWindowState::SizeFragmented(buffer[pos]);
                        break;
                    }

                    size = u16::from_le_bytes(buffer[pos..pos + 2].try_into().unwrap()) as usize;
                    pos += 2;
                    leftover -= 2;
                }
            }
            RxWindowState::Accumulating(batch_accumulator) => {
                batch_accumulator.accumulated_size += buffer.len();
                batch_accumulator.batch.buffers.push(buffer.clone());

                if batch_accumulator.accumulated_size >= batch_accumulator.batch.size {
                    let mut leftover =
                        batch_accumulator.accumulated_size - batch_accumulator.batch.size;
                    let mut pos = buffer.len() - leftover;

                    {
                        // send fragmented batch
                        let mut batch = FragmentedBatch {
                            size: batch_accumulator.batch.size,
                            data_offset: batch_accumulator.batch.data_offset,
                            buffers: vec![],
                        };
                        std::mem::swap(&mut batch.buffers, &mut batch_accumulator.batch.buffers);
                        log!("INFO", "on_batch");
                        on_batch(batch);
                    }

                    loop {
                        // no more data
                        if leftover == 0 {
                            self.state = RxWindowState::Initial;
                            break;
                        }

                        // buffer contains size fragment
                        if leftover == 1 {
                            self.state = RxWindowState::SizeFragmented(buffer[pos]);
                            break;
                        }

                        let size =
                            u16::from_le_bytes(buffer[pos..pos + 2].try_into().unwrap()) as usize;
                        pos += 2;
                        leftover -= 2;

                        // only size
                        if leftover == 0 {
                            let fragment = FragmentedBatch {
                                size,
                                data_offset: 0,
                                buffers: vec![],
                            };
                            let accumulator = BatchAccumulator::new(leftover, fragment);
                            self.state = RxWindowState::Accumulating(accumulator);
                            break;
                        }

                        // buffer contains batch fragment
                        if leftover < size {
                            let fragment = FragmentedBatch {
                                size,
                                data_offset: pos,
                                buffers: vec![buffer],
                            };
                            let accumulator = BatchAccumulator::new(leftover, fragment);
                            self.state = RxWindowState::Accumulating(accumulator);
                            break;
                        }

                        // buffer contains at least one more batch
                        let batch = FragmentedBatch {
                            size,
                            data_offset: pos,
                            buffers: vec![buffer.clone()],
                        };
                        log!("INFO", "on_batch");
                        on_batch(batch);
                        pos += size;
                        leftover -= size;
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct RxBuffer {
    data: &'static mut [u8],
    buf_id: u16,
    arena: Arc<ReservableArenaInner>,
}

impl Deref for RxBuffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl DerefMut for RxBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl RxBuffer {
    fn new(data: &'static mut [u8], buf_id: u16, arena: Arc<ReservableArenaInner>) -> Self {
        Self {
            data,
            buf_id,
            arena,
        }
    }
}

impl Drop for RxBuffer {
    fn drop(&mut self) {
        let provide_buffers = opcode::ProvideBuffers::new(
            self.data.as_mut_ptr(),
            BUF_SIZE as i32,
            1,
            0,
            self.buf_id as u16,
        )
        .build()
        .flags(Flags::SKIP_SUCCESS);

        self.arena.submitter.submit(provide_buffers).unwrap();
    }
}

#[derive(Debug)]
struct ReservableArenaInner {
    arena: BatchArena,
    submitter: SubmissionIface,
}

impl ReservableArenaInner {
    fn new(arena: BatchArena, submitter: SubmissionIface) -> Self {
        Self { arena, submitter }
    }
}

struct ReservableArena {
    inner: Arc<ReservableArenaInner>,
}

impl ReservableArena {
    fn new(arena: BatchArena, submitter: SubmissionIface) -> Self {
        let inner = Arc::new(ReservableArenaInner::new(arena, submitter));
        Self { inner }
    }

    unsafe fn buffer(&self, buf_id: u16, buf_len: usize) -> RxBuffer {
        let data = &mut self.inner.arena.index_mut_unchecked(buf_id as usize)[0..buf_len];
        RxBuffer::new(data, buf_id, self.inner.clone())
    }
}

#[derive(Clone)]
pub struct Reader {
    inner: Arc<ReaderInner>,
}

impl Reader {
    pub fn setup_fragmented_read<Cb>(&self, fd: RawFd, mut callback: Cb) -> ZResult<ReadTask>
    where
        Cb: FnMut(FragmentedBatch) + 'static,
    {
        let mut window = RxWindow::default();
        let raw_callback = move |buffer| {
            window.push(buffer, &mut callback);
        };

        ReadTask::new(fd, raw_callback, self.inner.clone())
    }

    pub fn setup_read<Cb>(&self, fd: RawFd, callback: Cb) -> ZResult<ReadTask>
    where
        Cb: FnMut(Arc<RxBuffer>) + 'static,
    {
        ReadTask::new(fd, callback, self.inner.clone())
    }

    pub fn new() -> Self {
        // create eventfd to wake io_uring on demand by producing read events
        let waker = Arc::new(
            nix::sys::eventfd::EventFd::from_value_and_flags(
                0,
                EfdFlags::EFD_CLOEXEC | EfdFlags::EFD_NONBLOCK,
            )
            .unwrap(),
        );

        // Create memory for waker's read events
        let mut dummy_mem = {
            let mem: [u8; 8] = [0; 8];
            Box::new(mem)
        };
        let c_waker = waker.clone();

        let (sender, receiver) = std::sync::mpsc::channel();

        let submitter = SubmissionIface::new(waker, sender);
        let c_submitter = submitter.clone();

        let exit_flag = Arc::new(AtomicBool::new(false));

        let c_exit_flag = exit_flag.clone();
        let _ = std::thread::spawn(move || {
            // io_uring read
            let ring = IoUring::builder()
                .setup_submit_all()
                //.setup_sqpoll(1)
                //.setup_iopoll()
                //.setup_sqpoll_cpu(0)
                //.setup_coop_taskrun()
                .setup_defer_taskrun()
                .setup_single_issuer()
                .build((1024).try_into().unwrap()).unwrap();
            let arena = BatchArena::new(64);
            {
                let provide_buffers = arena.provide_root_buffers();
                unsafe { ring.submission_shared().push(&provide_buffers).unwrap() };
                ring.submit_and_wait(1).unwrap();

                let cq: io_uring::cqueue::Entry = unsafe {
                    ring.completion_shared()
                        .next()
                        .expect("completion queue is empty")
                };
                assert!(cq.result() == 0);
            }

            let arena = ReservableArena::new(arena, c_submitter);

            // read for waker
            let waker_read =
                opcode::Read::new(types::Fd(c_waker.as_raw_fd()), dummy_mem.as_mut_ptr(), 8)
                    .build();

            unsafe { ring.submission_shared().push(&waker_read).unwrap() };

            loop {
                let mut sq = unsafe { ring.submission_shared() };
                let mut cq = unsafe { ring.completion_shared() };
                while let Some(e) = cq.next() {
                    //println!("e: {:?}", e);

                    if e.user_data() == 0 {
                        println!("Zero-user-data entry: {:?}", e);
                        unsafe { ring.submission_shared().push(&waker_read).unwrap() };
                        continue;
                    }

                    let rx: &Arc<Rx> = unsafe { std::mem::transmute(&mut e.user_data()) };

                    let need_submit = Reader::read_multi(&e, rx, &arena, &mut sq);

                    if need_submit {
                        sq.sync();
                        ring.submit().unwrap();
                        cq.sync();
                    }
                }
                drop(cq);

                // receive external submissions
                while let Ok(val) = receiver.try_recv() {
                    unsafe { sq.push(&val).unwrap() };
                }
                drop(sq);

                if c_exit_flag.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }

                // this wait can be interrupted by Self::wake_reader_thread
                ring.submit_and_wait(1).unwrap();
                //                c_ring.submit().unwrap();

                //                sq.sync();
                //                cq.sync();
                //                if cq.is_empty() {
                //                    println!("cq empty");
                //                    c_ring.submit_and_wait(1).unwrap();
                //                } else {
                //                    println!("cq non-empty");
                //                    c_ring.submit().unwrap();
                //                }
                //                cq.sync();
            }
        });

        let inner = Arc::new(ReaderInner::new(submitter, exit_flag));

        Self { inner }
    }

    fn read_multi(
        e: &io_uring::cqueue::Entry,
        rx: &Rx,
        arena: &ReservableArena,
        sq: &mut SubmissionQueue<'_>,
    ) -> bool {
        let mut need_submit = false;
        if e.result() < 0 {
            match e.result().neg() {
                libc::ENOBUFS => {
                    // We are out of buffers
                    println!("ENOBUFS: Restart multishot receive!!!");

                    let provide_buffers = arena
                        .inner
                        .arena
                        .provide_root_buffers()
                        .flags(Flags::SKIP_SUCCESS);
                    unsafe { sq.push(&provide_buffers).unwrap() };

                    let recv = opcode::RecvMulti::new(types::Fd(rx.fd), 0)
                        .build()
                        .user_data(e.user_data());

                    unsafe { sq.push(&recv).unwrap() };
                    need_submit = true;
                }
                unexpected => println!("Unexpected uring error: {unexpected}"),
            }
        } else {
            match io_uring::cqueue::buffer_select(e.flags()) {
                Some(buf_id) => {
                    //println!("Read multishot entry: {:?}", e);

                    if !io_uring::cqueue::more(e.flags()) {
                        println!("IORING_CQE_F_BUFFER: Restart multishot receive!!!");
                        let recv = opcode::RecvMulti::new(types::Fd(rx.fd), 0)
                            .build()
                            .user_data(e.user_data());
                        unsafe { sq.push(&recv).unwrap() };
                        need_submit = true;
                    }

                    let buf_len = e.result() as usize;

                    //println!("buf_id: {buf_id}, buf_len: {buf_len}");

                    if buf_len > 0 {
                        let rx_bufer = Arc::new(unsafe { arena.buffer(buf_id, buf_len) });
                        let callback = unsafe { &mut *rx.callback.callback.get() };
                        callback(rx_bufer);
                    } else {
                        println!("zero buf len");
                    }
                }
                None => println!("no IORING_CQE_F_BUFFER!"),
            };
        }
        need_submit
    }
}
