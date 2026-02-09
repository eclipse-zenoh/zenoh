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
    cell::UnsafeCell,
    ops::{Deref, DerefMut, Neg},
    os::fd::{AsRawFd, RawFd},
    sync::{atomic::AtomicBool, mpsc::Sender, Arc},
};

use io_uring::{opcode, squeue::Flags, types, EnterFlags, IoUring, SubmissionQueue};
use nix::sys::eventfd::{EfdFlags, EventFd};
#[cfg(unix)]
use thread_priority::{RealtimeThreadSchedulePolicy, ThreadBuilder, ThreadPriority};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use zenoh_core::{bail, zerror};
use zenoh_result::ZResult;

use crate::{batch_arena::BatchArena, BUF_SIZE};

#[derive(Debug)]
struct RxCallback {
    pub callback: UnsafeCell<Box<dyn FnMut(Arc<RxBuffer>) -> ZResult<()> + 'static>>,
}

impl RxCallback {
    fn new(callback: UnsafeCell<Box<dyn FnMut(Arc<RxBuffer>) -> ZResult<()> + 'static>>) -> Self {
        Self { callback }
    }
}

unsafe impl Send for RxCallback {}
unsafe impl Sync for RxCallback {}

#[derive(Debug)]
struct Rx {
    cb: RxCallback,
    pub fd: RawFd,
    error_sender: UnboundedSender<zenoh_result::Error>,
}

impl Rx {
    fn new(
        callback: Box<dyn FnMut(Arc<RxBuffer>) -> ZResult<()>>,
        fd: RawFd,
    ) -> (Self, UnboundedReceiver<zenoh_result::Error>) {
        let (error_sender, receiver) = tokio::sync::mpsc::unbounded_channel();

        let rx = Self {
            cb: RxCallback::new(UnsafeCell::new(callback)),
            error_sender,
            fd,
        };
        tracing::debug!("RX context created: {:?}", rx);
        (rx, receiver)
    }

    fn run_callback(&self, buffer: Arc<RxBuffer>) {
        let callback = unsafe { &mut *self.cb.callback.get() };
        if let Err(e) = (callback)(buffer) {
            self.post_error(e);
        }
    }

    fn post_error(&self, error: zenoh_result::Error) {
        let _ = self.error_sender.send(error);
    }
}

impl Drop for Rx {
    fn drop(&mut self) {
        tracing::debug!("Destroy RX context: {:?}", self);
    }
}

#[derive(Debug)]
pub struct ReadTask {
    user_data: u64,
    inner: Arc<ReaderInner>,
    error_listener: UnboundedReceiver<zenoh_result::Error>,
}

impl Drop for ReadTask {
    fn drop(&mut self) {
        tracing::debug!("Stopping read task: {:?}", self);

        let event = opcode::AsyncCancel::new(self.user_data).build();
        let _ = self.inner.submitter.submit(event);
    }
}

impl ReadTask {
    fn new<Cb>(fd: RawFd, callback: Cb, inner: Arc<ReaderInner>) -> ZResult<Self>
    where
        Cb: FnMut(Arc<RxBuffer>) -> ZResult<()> + 'static,
    {
        let (rx, error_listener) = Rx::new(Box::new(callback), fd.clone());
        let rx = Arc::new(rx);
        let user_data = unsafe { std::mem::transmute(rx) };

        let recv = opcode::RecvMulti::new(types::Fd(fd), 0)
            .build()
            .user_data(user_data);

        inner.submitter.submit(recv)?;

        let task = Self {
            user_data,
            inner,
            error_listener,
        };

        tracing::debug!("Read task created: {:?}", task);

        Ok(task)
    }

    pub async fn read_error(&mut self) -> zenoh_result::Error {
        let e = self.error_listener.recv().await;
        e.unwrap_or_else(|| zerror!("Task stopped").into())
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
        self.submit_quiet(entry)?;
        self.wake_reader_thread();
        Ok(())
    }

    fn submit_quiet(&self, entry: io_uring::squeue::Entry) -> ZResult<()> {
        self.sender.send(entry).map_err(|e| e.to_string().into())
    }

    fn wake_reader_thread(&self) {
        // A write() to an eventfd can (rarely) block if the internal counter is near overflow.
        // With non-blocking mode (EfdFlags::EFD_NONBLOCK), you’ll get EAGAIN instead of risking
        // a hang during shutdown logic.
        // TODO: handle EAGAIN or switch to blocking mode
        self.waker.write(1).unwrap();
    }
}

#[derive(Debug)]
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
    pub data_offset: usize,
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

    pub fn try_contagious_zerocopy(&self) -> Option<Arc<RxBuffer>> {
        if self.buffers.len() == 1 {
            return Some(self.buffers[0].clone());
        }
        None
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
    fn push<F>(&mut self, buffer: Arc<RxBuffer>, on_batch: &mut F) -> ZResult<()>
    where
        F: FnMut(FragmentedBatch) -> ZResult<()>,
    {
        log!("INFO", "Buffer len: {}", buffer.len());

        fn parse_size(bytes: [u8; 2]) -> ZResult<usize> {
            let size = u16::from_le_bytes(bytes) as usize;
            assert!(size != 0);
            if size == 0 {
                bail!("Zero-sized buffer in stream!");
            }
            log!("INFO", "parsed size: {}", size);
            Ok(size as usize)
        }

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

                    let size = parse_size(buffer[pos..pos + 2].try_into().unwrap())?;
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
                        assert!(size != 0);
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
                        assert!(size != 0);
                        break;
                    }

                    // buffer contains at least one more batch
                    let batch = FragmentedBatch {
                        size,
                        data_offset: pos,
                        buffers: vec![buffer.clone()],
                    };
                    log!("INFO", "on_batch");
                    on_batch(batch)?;
                    pos += size;
                    leftover -= size;
                }
            }
            RxWindowState::SizeFragmented(size_fragment) => {
                // defragment size
                let mut size = parse_size([*size_fragment, buffer[0]])?;
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
                        assert!(size != 0);
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
                        assert!(size != 0);
                        break;
                    }

                    // buffer contains at least one more batch
                    let batch = FragmentedBatch {
                        size,
                        data_offset: pos,
                        buffers: vec![buffer.clone()],
                    };
                    log!("INFO", "on_batch");
                    on_batch(batch)?;
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

                    size = parse_size(buffer[pos..pos + 2].try_into().unwrap())?;
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
                            buffers: vec![], //  batch_accumulator.batch.buffers.clone(),
                        };
                        std::mem::swap(&mut batch.buffers, &mut batch_accumulator.batch.buffers);
                        log!("INFO", "on_batch");
                        on_batch(batch)?;
                    }

                    loop {
                        // no more data
                        if leftover == 0 {
                            self.state = RxWindowState::Initial;
                            log!("INFO", "Accumulating -> Initial");
                            break;
                        }

                        // buffer contains size fragment
                        if leftover == 1 {
                            self.state = RxWindowState::SizeFragmented(buffer[pos]);
                            log!("INFO", "Accumulating -> SizeFragmented");
                            break;
                        }

                        let size = parse_size(buffer[pos..pos + 2].try_into().unwrap())?;
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
                            assert!(size != 0);
                            log!("INFO", "Accumulating -> Accumulating (only size)");
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
                            assert!(size != 0);
                            log!("INFO", "Accumulating -> Accumulating (fragment)");
                            break;
                        }

                        // buffer contains at least one more batch
                        let batch = FragmentedBatch {
                            size,
                            data_offset: pos,
                            buffers: vec![buffer.clone()],
                        };
                        log!("INFO", "on_batch");
                        on_batch(batch)?;
                        pos += size;
                        leftover -= size;
                    }
                }
            }
        }
        Ok(())
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

        self.arena.submitter.submit_quiet(provide_buffers).unwrap();
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
        Cb: FnMut(FragmentedBatch) -> ZResult<()> + 'static,
    {
        tracing::debug!("Setting up fragmented read task for fd: {fd}");

        let mut window = RxWindow::default();
        let raw_callback = move |buffer| window.push(buffer, &mut callback);

        ReadTask::new(fd, raw_callback, self.inner.clone())
    }

    pub fn setup_read<Cb>(&self, fd: RawFd, callback: Cb) -> ZResult<ReadTask>
    where
        Cb: FnMut(Arc<RxBuffer>) -> ZResult<()> + 'static,
    {
        tracing::debug!("Setting up read task for fd: {fd}");

        ReadTask::new(fd, callback, self.inner.clone())
    }

    pub fn new() -> Self {
        // create eventfd to wake io_uring on demand by producing read events
        let waker = Arc::new(
            nix::sys::eventfd::EventFd::from_value_and_flags(0, EfdFlags::EFD_CLOEXEC).unwrap(),
        );

        let c_waker = waker.clone();

        let (sender, receiver) = std::sync::mpsc::channel();

        let submitter = SubmissionIface::new(waker, sender);
        let c_submitter = submitter.clone();

        let exit_flag = Arc::new(AtomicBool::new(false));

        let c_exit_flag = exit_flag.clone();

        #[cfg(unix)]
        let builder = ThreadBuilder::default()
            .name("uring_task")
            .policy(thread_priority::ThreadSchedulePolicy::Realtime(
                RealtimeThreadSchedulePolicy::Fifo,
            ))
            .priority(ThreadPriority::Min);

        let _ = builder.spawn(move |result| {
            // Create memory for waker's read events
            let mut dummy_mem = {
                let mem: [u8; 8] = [0; 8];
                Box::new(mem)
            };

            if let Err(e) = result {
                let mut err = format!(
                    "{:?}: error setting scheduling priority for thread: {:?}, will run with ",
                    std::thread::current().name(),
                    e
                );
                #[cfg(windows)]
                {
                    err.push_str("the default one. ");
                }
                #[cfg(unix)]
                {
                    use thread_priority::ThreadPriorityValue;

                    for priority in (ThreadPriorityValue::MIN..ThreadPriorityValue::MAX).rev() {
                        if let Ok(p) = priority.try_into() {
                            use thread_priority::set_current_thread_priority;

                            if set_current_thread_priority(ThreadPriority::Crossplatform(p)).is_ok()
                            {
                                err.push_str(&format!("priority {priority}. "));
                                break;
                            }
                        }
                    }
                }
                err.push_str("This is not an hard error and it can be safely ignored under normal operating conditions. \
                Though the SHM subsystem may experience some timeouts in case of an heavy congested system where this watchdog thread may not be scheduled at the required frequency.");
                println!("{}", err);
            }


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
            let arena = BatchArena::new(16);
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
                while let Some(e) = unsafe { ring.completion_shared() }.next() {
                    let mut sq = unsafe { ring.submission_shared() };

                    //println!("e: {:?}", e);
                    if e.user_data() == 0 {
                        println!("Zero-user-data entry: {:?}", e);
                        unsafe { sq.push(&waker_read).unwrap() };
                        continue;
                    }

                    let rx: &Arc<Rx> = unsafe { std::mem::transmute(&e.user_data()) };

                    let (need_submit, _sock_nonempty) = Reader::read_multi(&e, rx, &arena, &mut sq);

                    while let Ok(val) = receiver.try_recv() {
                        unsafe { sq.push(&val).unwrap() };
                        //recv_ctr += 1;
                    }

                    if need_submit {
                        let len = sq.len() as u32;
                        drop(sq);

                        unsafe { ring.submitter().enter::<libc::sigset_t>(
                            len,
                             0,
                              EnterFlags::GETEVENTS.bits(),
                               None).unwrap(); }
                    } else {
                        drop(sq);
                    }
                }

                //println!("loop_ctr: {loop_ctr}");
                //println!("recv_ctr: {recv_ctr}");

                // receive external submissions
                let mut sq = unsafe { ring.submission_shared() };
                while let Ok(val) = receiver.try_recv() {
                    unsafe { sq.push(&val).unwrap() };
                }
                drop(sq);

                if c_exit_flag.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }

                // this wait can be interrupted by Self::wake_reader_thread
                ring.submit_and_wait(1).unwrap();

                //unsafe { ring.submitter().enter::<libc::sigset_t>(
                //            sq.len() as u32,
                //             0,
                //              EnterFlags::GETEVENTS.bits(),
                //               None).unwrap(); }
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
    ) -> (bool, bool) {
        let mut need_submit = false;
        let mut sock_nonempty = false;
        if e.result() < 0 {
            match e.result().neg() {
                libc::ENOBUFS => {
                    // We are out of buffers
                    //println!("ENOBUFS: Restart multishot receive!!!");

                    //let provide_buffers = arena
                    //    .inner
                    //    .arena
                    //    .provide_root_buffers()
                    //    .flags(Flags::SKIP_SUCCESS);
                    //unsafe { sq.push(&provide_buffers).unwrap() };

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

                    assert!(io_uring::cqueue::buffer_more(e.flags()) == false);
                    assert!(io_uring::cqueue::notif(e.flags()) == false);

                    sock_nonempty |= io_uring::cqueue::sock_nonempty(e.flags());

                    //println!("buf_id: {buf_id}, buf_len: {buf_len}");

                    if buf_len > 0 {
                        let rx_buffer = Arc::new(unsafe { arena.buffer(buf_id, buf_len) });
                        rx.run_callback(rx_buffer);
                    } else {
                        println!("zero buf len");
                    }
                }
                None => {
                    println!("no IORING_CQE_F_BUFFER!");

                    println!("Stopping read task");
                    assert!(e.user_data() != 0);
                    let rx: Arc<Rx> = unsafe { std::mem::transmute(e.user_data()) };
                    rx.post_error(zerror!("Read task interrupt").into());
                    drop(rx);
                }
            };
        }
        (need_submit, sock_nonempty)
    }
}
