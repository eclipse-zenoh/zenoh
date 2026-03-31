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
    num::{NonZeroU32, NonZeroU64},
    ops::{Deref, DerefMut, Neg},
    os::fd::{AsRawFd, RawFd},
    rc::Rc,
    sync::{atomic::AtomicBool, Arc},
};

use flume::{Receiver, Sender};
use io_uring::{opcode, squeue::Flags, types, IoUring, SubmissionQueue};
use nix::sys::eventfd::{EfdFlags, EventFd};
//use thread_priority::{RealtimeThreadSchedulePolicy, ThreadBuilder, ThreadPriority};
use tokio::sync::mpsc::{error::TryRecvError, UnboundedReceiver, UnboundedSender};
use zenoh_core::{bail, zerror};
use zenoh_result::ZResult;
use zenoh_runtime::ZRuntime;

use crate::batch_arena::BatchArena;

type RxCallbackImpl = dyn FnMut(Arc<RxBuffer>) -> ZResult<()> + Send + 'static;

#[derive(Debug)]
struct RxCallback {
    pub callback: UnsafeCell<Box<RxCallbackImpl>>,
}

impl RxCallback {
    fn new(callback: UnsafeCell<Box<RxCallbackImpl>>) -> Self {
        Self { callback }
    }
}

#[derive(Debug)]
struct Rx {
    cb: RxCallback,
    pub fd: RawFd,
    error_sender: UnboundedSender<zenoh_result::Error>,
}

impl Rx {
    fn new(fd: RawFd, cb: RxCallback, error_sender: UnboundedSender<zenoh_result::Error>) -> Self {
        let rx = Self {
            cb,
            error_sender,
            fd,
        };
        tracing::debug!("RX context created: {:?}", rx);
        rx
    }

    fn run_callback(&self, buffer: Arc<RxBuffer>) {
        tracing::trace!("CB begin....");
        let callback = unsafe { &mut *self.cb.callback.get() };
        tracing::trace!("CB end....");

        if let Err(e) = (callback)(buffer) {
            self.post_error(e);
        }
    }

    fn post_error(&self, error: zenoh_result::Error) {
        tracing::error!("Read task error: {error}");
        let _ = self.error_sender.send(error);
    }
}

impl Drop for Rx {
    fn drop(&mut self) {
        tracing::debug!("Destroy RX context: {:?}", self);
    }
}

#[derive(Debug)]
enum ReactorCmd {
    StartRx(
        RawFd,
        RxCallback,
        Arc<tokio::sync::SetOnce<IndexGeneration>>,
        UnboundedSender<zenoh_result::Error>,
    ),
    StopRx(IndexGeneration),
}
#[derive(Debug)]
pub struct ReadTask {
    submitter: SubmissionIface,
    error_receiver: UnboundedReceiver<zenoh_result::Error>,
    index: IndexGeneration,
}

impl Drop for ReadTask {
    fn drop(&mut self) {
        tracing::debug!("Stopping read task: {:?}", self);

        let stop_rx_cmd = ReactorCmd::StopRx(self.index);
        let _ = self.submitter.submit(stop_rx_cmd);
    }
}

impl ReadTask {
    pub async fn stop(&mut self) {
        tracing::debug!("Async stopping read task: {:?}", self);

        let stop_rx_cmd = ReactorCmd::StopRx(self.index);
        let _ = self.submitter.submit(stop_rx_cmd);

        // waiting for read task context to be destroyed within the reactor
        while self.error_receiver.recv().await.is_some() {}
    }

    async fn new<Cb>(fd: RawFd, callback: Cb, submitter: SubmissionIface) -> ZResult<Self>
    where
        Cb: FnMut(Arc<RxBuffer>) -> ZResult<()> + Send + 'static,
    {
        tracing::debug!("Creating read task: fd = {fd}");

        let (error_sender, error_receiver) = tokio::sync::mpsc::unbounded_channel();
        let index = Arc::new(tokio::sync::SetOnce::new());
        let rx_callback = RxCallback::new(UnsafeCell::new(Box::new(callback)));

        let start_rx_cmd = ReactorCmd::StartRx(fd, rx_callback, index.clone(), error_sender);

        submitter.submit(start_rx_cmd)?;

        tracing::debug!("Start waiting index for read task: fd = {fd}");
        let index = *index.wait().await;
        tracing::debug!("Got index {:?} for read task: fd = {fd}", index);

        Ok(Self {
            submitter,
            error_receiver,
            index,
        })
    }

    pub fn try_read_error_sync(
        &mut self,
    ) -> core::result::Result<zenoh_result::Error, TryRecvError> {
        self.error_receiver.try_recv()
    }

    pub async fn read_error(&mut self) -> zenoh_result::Error {
        let e = self.error_receiver.recv().await;
        e.unwrap_or_else(|| zerror!("Task stopped").into())
    }
}

#[derive(Debug, Clone)]
struct SubmissionIface {
    waker: Arc<EventFd>,
    sender: Sender<ReactorCmd>,
}

impl SubmissionIface {
    fn new(waker: Arc<EventFd>, sender: Sender<ReactorCmd>) -> Self {
        Self { waker, sender }
    }

    fn submit(&self, cmd: ReactorCmd) -> ZResult<()> {
        self.submit_quiet(cmd)?;
        self.wake_reader_thread()
    }

    fn submit_quiet(&self, cmd: ReactorCmd) -> ZResult<()> {
        tracing::debug!("Submit cmd: {:?}", cmd);
        self.sender.send(cmd).map_err(|e| e.to_string().into())
    }

    fn wake_reader_thread(&self) -> ZResult<()> {
        // A write() to an eventfd can (rarely) block if the internal counter is near overflow.
        // With non-blocking mode (EfdFlags::EFD_NONBLOCK), you’ll get EAGAIN instead of risking
        // a hang during shutdown logic.
        // TODO: handle EAGAIN or switch to blocking mode
        tracing::debug!("Waking reader thread");
        self.waker.write(1).map(|_| ()).map_err(|e| e.into())
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
        tracing::debug!("Drop ReaderInner: {:?}", self);
        self.exit_flag
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let _ = self.submitter.wake_reader_thread();
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

    // TODO: change to zerocopy approach with multi-slice support for read codec
    pub fn contagious_copy(&self) -> Vec<u8> {
        let mut result = Vec::with_capacity(self.size);

        let mut leftover = self.size;

        for (i, buf) in self.buffers.iter().enumerate() {
            let first = i == 0;
            let last = i == self.buffers.len() - 1;

            let mut slice: &[u8] = buf.deref(); // assuming RxBuffer exposes this

            if first {
                slice = &slice[self.data_offset..];
            }

            if last {
                slice = &slice[..leftover];
            }

            result.extend_from_slice(slice);

            leftover -= slice.len();
        }

        result
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

//macro_rules! log {
//    ($level:expr, $($arg:tt)*) => {
//        eprintln!("[{}] {}:{} - {}",
//            $level,
//            file!(),
//            line!(),
//            format_args!($($arg)*));
//    };
//}

impl RxWindow {
    fn push<F>(&mut self, buffer: Arc<RxBuffer>, on_batch: &mut F) -> ZResult<()>
    where
        F: FnMut(FragmentedBatch) -> ZResult<()>,
    {
        tracing::trace!("Buffer len: {}", buffer.len());

        fn parse_size(bytes: [u8; 2]) -> usize {
            let size = u16::from_le_bytes(bytes) as usize;
            tracing::trace!("parsed size: {}", size);
            size
        }

        match &mut self.state {
            RxWindowState::Initial => {
                let mut leftover = buffer.len();
                let mut pos = 0;

                while leftover > 0 {
                    // buffer contains size fragment
                    if leftover == 1 {
                        self.state = RxWindowState::SizeFragmented(buffer[pos]);
                        break;
                    }

                    // size may be 0!
                    let size = parse_size(buffer[pos..pos + 2].try_into().unwrap());
                    pos += 2;
                    leftover -= 2;

                    // only size
                    if leftover == 0 {
                        let fragment = FragmentedBatch {
                            size,
                            data_offset: 0,
                            buffers: vec![],
                        };

                        if size == 0 {
                            on_batch(fragment)?;
                            self.state = RxWindowState::Initial;
                        } else {
                            let accumulator = BatchAccumulator::new(leftover, fragment);
                            self.state = RxWindowState::Accumulating(accumulator);
                        }
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

                    // don't borrow buffer for ephemeral batches
                    let buffers = if size == 0 {
                        vec![]
                    } else {
                        vec![buffer.clone()]
                    };

                    // buffer contains at least one more batch
                    let batch = FragmentedBatch {
                        size,
                        data_offset: pos,
                        buffers,
                    };
                    tracing::trace!("on_batch");
                    on_batch(batch)?;
                    pos += size;
                    leftover -= size;
                }
            }
            RxWindowState::SizeFragmented(size_fragment) => {
                // defragment size
                // size may be 0!
                let mut size = parse_size([*size_fragment, buffer[0]]);
                let mut leftover = buffer.len() - 1;
                let mut pos = 1;

                loop {
                    // only size
                    if leftover == 0 {
                        let fragment = FragmentedBatch {
                            size,
                            data_offset: 0,
                            buffers: vec![],
                        };

                        if size == 0 {
                            on_batch(fragment)?;
                            self.state = RxWindowState::Initial;
                        } else {
                            let accumulator = BatchAccumulator::new(leftover, fragment);
                            self.state = RxWindowState::Accumulating(accumulator);
                        }
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

                    // don't borrow buffer for ephemeral batches
                    let buffers = if size == 0 {
                        vec![]
                    } else {
                        vec![buffer.clone()]
                    };

                    // buffer contains at least one more batch
                    let batch = FragmentedBatch {
                        size,
                        data_offset: pos,
                        buffers,
                    };
                    tracing::trace!("on_batch");
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

                    // size may be 0!
                    size = parse_size(buffer[pos..pos + 2].try_into().unwrap());
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
                        tracing::trace!("on_batch");
                        on_batch(batch)?;
                    }

                    loop {
                        // no more data
                        if leftover == 0 {
                            self.state = RxWindowState::Initial;
                            tracing::trace!("Accumulating -> Initial");
                            break;
                        }

                        // buffer contains size fragment
                        if leftover == 1 {
                            self.state = RxWindowState::SizeFragmented(buffer[pos]);
                            tracing::trace!("Accumulating -> SizeFragmented");
                            break;
                        }

                        // size may be 0!
                        let size = parse_size(buffer[pos..pos + 2].try_into().unwrap());
                        pos += 2;
                        leftover -= 2;

                        // only size
                        if leftover == 0 {
                            let fragment = FragmentedBatch {
                                size,
                                data_offset: 0,
                                buffers: vec![],
                            };

                            if size == 0 {
                                on_batch(fragment)?;
                                self.state = RxWindowState::Initial;
                                tracing::trace!("Accumulating -> Initial");
                            } else {
                                let accumulator = BatchAccumulator::new(leftover, fragment);
                                self.state = RxWindowState::Accumulating(accumulator);
                                tracing::trace!("Accumulating -> Accumulating (only size)");
                            }
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
                            tracing::trace!("Accumulating -> Accumulating (fragment)");
                            break;
                        }

                        // don't borrow buffer for ephemeral batches
                        let buffers = if size == 0 {
                            vec![]
                        } else {
                            vec![buffer.clone()]
                        };

                        // buffer contains at least one more batch
                        let batch = FragmentedBatch {
                            size,
                            data_offset: pos,
                            buffers,
                        };
                        tracing::trace!("on_batch");
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
        self.data
    }
}

impl DerefMut for RxBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data
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
        self.arena.recycle_batch(self.buf_id);
    }
}

struct ReservableArenaInner {
    arena: BatchArena,
    submitter: SubmissionIface,
    recycled_batches: atomic_queue::Queue<u16>,
}

impl std::fmt::Debug for ReservableArenaInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReservableArenaInner")
            .field("arena", &self.arena)
            .field("submitter", &self.submitter)
            .finish()
    }
}

impl ReservableArenaInner {
    fn new(arena: BatchArena, submitter: SubmissionIface) -> Self {
        let recycled_batches = atomic_queue::Queue::new(u16::MAX as usize);
        Self {
            arena,
            submitter,
            recycled_batches,
        }
    }

    fn recycle_batch(&self, buf_id: u16) {
        assert!(self.recycled_batches.push(buf_id));
    }

    fn pop_recycled_batch(&self) -> Option<(io_uring::squeue::Entry, usize)> {
        match self.recycled_batches.pop() {
            Some(buf_id) => {
                let data = unsafe { &mut self.arena.index_mut_unchecked(buf_id as usize)[0..1] };
                Some((
                    opcode::ProvideBuffers::new(
                        data.as_mut_ptr(),
                        self.arena.batch_size() as i32,
                        1,
                        0,
                        buf_id,
                    )
                    .build()
                    .flags(Flags::SKIP_SUCCESS),
                    1,
                ))
            }
            None => self.arena.allocate_more_batches(),
        }
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

#[derive(Debug)]
struct RxContextCell {
    context: Option<Rc<Rx>>,
    generation: NonZeroU32,
}

impl RxContextCell {
    fn new(context: Rc<Rx>, generation: NonZeroU32) -> Self {
        Self {
            context: Some(context),
            generation,
        }
    }

    fn get(&self, generation: NonZeroU32) -> Option<&Rx> {
        if self.generation == generation {
            return self.context.as_deref();
        }
        None
    }

    fn free(&mut self, generation: NonZeroU32) {
        if self.generation == generation {
            if let Some(context) = self.context.take() {
                tracing::debug!("Begin RxContext destroy {:?}", context);
                assert!(Rc::strong_count(&context) == 1);
                drop(context);
                tracing::debug!("End RxContext destroy!");
            }
        } else {
            tracing::debug!(
                "Unable to free: generation mismatch! {:?}, generation: {generation}",
                self
            );
        }
    }

    fn try_alloc(&mut self, generation: NonZeroU32, context: &Rc<Rx>) -> bool {
        if self.context.is_none() {
            self.context = Some(context.clone());
            self.generation = generation;
            return true;
        }
        false
    }
}

struct RxContextStorage {
    data: Vec<RxContextCell>,
    next_generation: NonZeroU32,
}

impl RxContextStorage {
    fn new() -> Self {
        let data = Vec::with_capacity(16);
        Self {
            data,
            next_generation: NonZeroU32::MIN,
        }
    }

    fn get(&self, id: IndexGeneration) -> Option<&Rx> {
        self.data[id.index() as usize].get(id.generation())
    }

    fn free(&mut self, id: IndexGeneration) {
        self.data[id.index() as usize].free(id.generation())
    }

    fn alloc(&mut self, context: Rc<Rx>) -> IndexGeneration {
        self.next_generation = self.next_generation.saturating_add(1);
        if self.next_generation == NonZeroU32::MAX {
            self.next_generation = NonZeroU32::MIN;
        }

        for (index, cell) in self.data.iter_mut().enumerate() {
            if cell.try_alloc(self.next_generation, &context) {
                return IndexGeneration::new(index as u32, self.next_generation);
            }
        }

        let new_cell = RxContextCell::new(context, self.next_generation);
        self.data.push(new_cell);
        IndexGeneration::new((self.data.len() - 1) as u32, self.next_generation)
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IndexGeneration(NonZeroU64);

impl IndexGeneration {
    pub const INVALID_MIN: u64 = 0;
    pub const INVALID_MAX: u64 = u64::MAX;

    #[inline]
    pub const fn new(index: u32, generation: NonZeroU32) -> Self {
        // SAFETY: this is safe because generation is NonZeroU32
        Self(unsafe {
            NonZeroU64::new_unchecked(((generation.get() as u64) << 32) | (index as u64))
        })
    }

    /// # Safety
    ///
    /// This is safe if `val` is not INVALID_MIN and not INVALID_MAX.
    #[inline]
    pub const unsafe fn new_unchecked(val: u64) -> Self {
        Self(NonZeroU64::new_unchecked(val))
    }

    #[inline]
    pub const fn index(self) -> u32 {
        self.0.get() as u32
    }

    #[inline]
    pub const fn generation(self) -> NonZeroU32 {
        unsafe { NonZeroU32::new_unchecked((self.0.get() >> 32) as u32) }
    }
}

impl From<IndexGeneration> for u64 {
    #[inline]
    fn from(value: IndexGeneration) -> Self {
        value.0.get()
    }
}

#[derive(Clone, Debug)]
pub struct Reader {
    inner: Arc<ReaderInner>,
    receiver: tokio::sync::watch::Receiver<String>,
}

impl Drop for Reader {
    fn drop(&mut self) {
        tracing::debug!("Drop Reader: {:?}", self);
    }
}

impl Reader {
    pub async fn setup_fragmented_read<Cb>(&self, fd: RawFd, mut callback: Cb) -> ZResult<ReadTask>
    where
        Cb: FnMut(FragmentedBatch) -> ZResult<()> + Send + 'static,
    {
        tracing::debug!("Setting up fragmented read task for fd: {fd}");

        let mut window = RxWindow::default();
        let raw_callback = move |buffer| window.push(buffer, &mut callback);

        ReadTask::new(fd, raw_callback, self.inner.submitter.clone()).await
    }

    pub async fn setup_read<Cb>(&self, fd: RawFd, callback: Cb) -> ZResult<ReadTask>
    where
        Cb: FnMut(Arc<RxBuffer>) -> ZResult<()> + Send + 'static,
    {
        tracing::debug!("Setting up read task for fd: {fd}");

        ReadTask::new(fd, callback, self.inner.submitter.clone()).await
    }

    pub fn new(batch_size: usize, batch_count: usize) -> ZResult<Self> {
        // create eventfd to wake io_uring on demand by producing read events
        let waker = Arc::new(nix::sys::eventfd::EventFd::from_value_and_flags(
            0,
            EfdFlags::EFD_CLOEXEC,
        )?);

        let c_waker = waker.clone();

        let (sender, receiver) = flume::unbounded();

        let submitter = SubmissionIface::new(waker, sender);
        let c_submitter = submitter.clone();

        let exit_flag = Arc::new(AtomicBool::new(false));

        let c_exit_flag = exit_flag.clone();

        let (join_sender, mut join_receiver) = tokio::sync::watch::channel("".into());
        join_receiver.mark_unchanged();

        let ring_worker = move || -> ZResult<()> {
            // Create Rx context storage
            let mut context_storage = RxContextStorage::new();

            // io_uring read
            let ring = IoUring::builder()
                .setup_submit_all()
                //.setup_sqpoll(1)
                //.setup_iopoll()
                //.setup_sqpoll_cpu(0)
                //.setup_coop_taskrun()
                .setup_defer_taskrun()
                .setup_single_issuer()
                .build((4096 /*batch_count*2*/).try_into()?)?;
            let arena = BatchArena::new(batch_size, batch_count, u16::MAX as usize)?;
            {
                let provide_buffers = arena.provide_root_buffers();
                unsafe { ring.submission_shared().push(&provide_buffers)? };
                ring.submit_and_wait(1)?;

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
                opcode::PollAdd::new(types::Fd(c_waker.as_raw_fd()), libc::POLLIN as _)
                    .build()
                    .user_data(IndexGeneration::INVALID_MAX)
                    .flags(io_uring::squeue::Flags::ASYNC);

            unsafe { ring.submission_shared().push(&waker_read)? };

            fn roll_cmds(
                receiver: &Receiver<ReactorCmd>,
                context_storage: &mut RxContextStorage,
                arena: &ReservableArena,
                sq: &mut SubmissionQueue<'_>,
                ctr: &mut i32,
                batch_count: usize,
            ) -> ZResult<()> {
                //tracing::debug!("Reading cmds....");

                // receive external submissions
                while let Ok(val) = receiver.try_recv() {
                    tracing::debug!("Cmd: {:?}", val);

                    match val {
                        ReactorCmd::StartRx(fd, callback, set_once, error_sender) => {
                            let rx_context = Rc::new(Rx::new(fd, callback, error_sender));
                            let index = context_storage.alloc(rx_context);
                            set_once.set(index)?;

                            *ctr -= (batch_count / 2) as i32;
                            roll_ring_batches(arena, ctr, sq)?;

                            let recv = opcode::RecvMulti::new(types::Fd(fd), 0)
                                .build()
                                .flags(io_uring::squeue::Flags::ASYNC)
                                .user_data(index.into());

                            unsafe { sq.push(&recv)? }
                        }
                        ReactorCmd::StopRx(index_generation) => {
                            context_storage.free(index_generation);

                            let event = opcode::AsyncCancel::new(index_generation.into())
                                .build()
                                .user_data(index_generation.into())
                                .flags(io_uring::squeue::Flags::ASYNC);

                            unsafe { sq.push(&event)? }

                            *ctr += (batch_count / 2) as i32;
                        }
                    }
                }

                Ok(())
            }

            fn roll_ring_batches(
                arena: &ReservableArena,
                ctr: &mut i32,
                sq: &mut SubmissionQueue<'_>,
            ) -> ZResult<()> {
                while *ctr < 0 {
                    match arena.inner.pop_recycled_batch() {
                        Some((batch, count)) => {
                            unsafe { sq.push(&batch)? }
                            *ctr += count as i32;
                        }
                        None => break,
                    }
                }
                Ok(())
            }

            let mut batch_ctr = (batch_count / 2) as i32;

            loop {
                while let Some(e) = unsafe { ring.completion_shared() }.next() {
                    let mut sq = unsafe { ring.submission_shared() };

                    roll_ring_batches(&arena, &mut batch_ctr, &mut sq)?;
                    roll_cmds(
                        &receiver,
                        &mut context_storage,
                        &arena,
                        &mut sq,
                        &mut batch_ctr,
                        batch_count,
                    )?;

                    match e.user_data() {
                        IndexGeneration::INVALID_MIN => {
                            tracing::debug!("Zero-user-data entry: {:?}", e);
                        }
                        IndexGeneration::INVALID_MAX => {
                            tracing::debug!("Waker event: {:?}", e);
                            let _ = c_waker.read()?;
                            unsafe { sq.push(&waker_read)? };
                        }
                        index => {
                            let index = unsafe { IndexGeneration::new_unchecked(index) };
                            if Reader::multi(
                                &context_storage,
                                &e,
                                index,
                                &arena,
                                &mut sq,
                                &mut batch_ctr,
                            )? {
                                let len = sq.len() as u32;
                                drop(sq);
                                //ring.submit()?;
                                unsafe {
                                    ring.submitter().enter::<libc::sigset_t>(
                                        len,
                                        0,
                                        io_uring::EnterFlags::GETEVENTS.bits(),
                                        None,
                                    )?;
                                }
                            }
                        }
                    }
                }

                let mut sq = unsafe { ring.submission_shared() };

                // reclaim batches
                roll_ring_batches(&arena, &mut batch_ctr, &mut sq)?;

                // receive external submissions
                roll_cmds(
                    &receiver,
                    &mut context_storage,
                    &arena,
                    &mut sq,
                    &mut batch_ctr,
                    batch_count,
                )?;

                //let len = sq.len();

                drop(sq);

                if c_exit_flag.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }

                // this wait can be interrupted by Self::wake_reader_thread
                ring.submit_and_wait(1)?;
                //std::thread::sleep(std::time::Duration::from_millis(1));
                //ring.submit()?;

                //unsafe {
                //    ring.submitter().enter::<libc::sigset_t>(
                //        len as u32,
                //        0,
                //        io_uring::EnterFlags::GETEVENTS.bits(),
                //        None,
                //    )?;
                //}
            }
            Ok(())
        };

        ZRuntime::RX.spawn_blocking(move || {
            if let Err(e) = ring_worker() {
                tracing::error!("Uring reactor error: {e}");
                let _ = join_sender.send(e.to_string());
            }
            tracing::debug!("Urng reactor thread finished!");
        });

        /*
        #[cfg(unix)]
        let builder = ThreadBuilder::default()
            .name("uring_task")
            .policy(thread_priority::ThreadSchedulePolicy::Realtime(
                RealtimeThreadSchedulePolicy::Fifo,
            ))
            .priority(ThreadPriority::Min);

        let _ = builder.spawn(move |result| {
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
                        tracing::debug!( "{}", err);
            }

            if let Err(e) = ring_worker() {
                let _ = join_sender.send(e.to_string());
            }
        });
        */

        let inner = Arc::new(ReaderInner::new(submitter, exit_flag));

        Ok(Self {
            inner,
            receiver: join_receiver,
        })
    }

    pub async fn wait_finished(&self) -> ZResult<()> {
        let mut r = self.receiver.clone();
        match r.changed().await {
            Ok(_) => Err((*r.borrow_and_update()).clone().into()),
            Err(_) => Ok(()),
        }
    }

    fn multi(
        context_storage: &RxContextStorage,
        e: &io_uring::cqueue::Entry,
        index: IndexGeneration,
        arena: &ReservableArena,
        sq: &mut SubmissionQueue<'_>,
        ctr: &mut i32,
    ) -> ZResult<bool> {
        match context_storage.get(index) {
            Some(context) => match Reader::read_multi(e, index, context, arena, sq, ctr) {
                Ok(val) => Ok(val),
                Err(e) => {
                    context.post_error(e);
                    Ok(false)
                }
            },
            None => Ok(Self::utilize_multi(e, arena, ctr)),
        }
    }

    fn read_multi(
        e: &io_uring::cqueue::Entry,
        index: IndexGeneration,
        context: &Rx,
        arena: &ReservableArena,
        sq: &mut SubmissionQueue<'_>,
        ctr: &mut i32,
    ) -> ZResult<bool> {
        let mut need_submit = false;
        if e.result() < 0 {
            tracing::trace!("Error entry: {:?}", e);

            match e.result().neg() {
                libc::ENOBUFS => {
                    // We are out of buffers
                    tracing::debug!("ENOBUFS: Restart multishot receive for task {:?}", index);

                    let recv = opcode::RecvMulti::new(types::Fd(context.fd), 0)
                        .build()
                        .flags(io_uring::squeue::Flags::ASYNC)
                        .user_data(index.into());

                    unsafe { sq.push(&recv)? };
                    need_submit = true;
                }
                libc::ECANCELED => {
                    bail!("Rx task cancelled: {:?}", index);
                }
                libc::EALREADY => {
                    bail!("Operation already in progress for task {:?}", index);
                }
                unexpected => {
                    bail!("Task-related uring error: {}, task {:?}", unexpected, index);
                }
            }
        } else {
            match io_uring::cqueue::buffer_select(e.flags()) {
                Some(buf_id) => {
                    tracing::trace!("Read multishot entry: {:?}", e);

                    *ctr -= 1;

                    if !io_uring::cqueue::more(e.flags()) {
                        tracing::debug!("IORING_CQE_F_BUFFER: Restart multishot receive!!!");

                        let recv = opcode::RecvMulti::new(types::Fd(context.fd), 0)
                            .build()
                            .flags(io_uring::squeue::Flags::ASYNC)
                            .user_data(index.into());

                        unsafe { sq.push(&recv)? };
                        need_submit = true;
                    }

                    let buf_len = e.result() as usize;
                    if buf_len > 0 {
                        let rx_buffer = Arc::new(unsafe { arena.buffer(buf_id, buf_len) });
                        context.run_callback(rx_buffer);
                    } else {
                        arena.inner.recycle_batch(buf_id);
                        tracing::debug!("zero buf len");
                    }
                }
                None => {
                    //bail!("no IORING_CQE_F_BUFFER: {:?}", e);
                }
            };
        }
        Ok(need_submit)
    }

    fn utilize_multi(e: &io_uring::cqueue::Entry, arena: &ReservableArena, ctr: &mut i32) -> bool {
        if e.result() >= 0 {
            match io_uring::cqueue::buffer_select(e.flags()) {
                Some(buf_id) => {
                    tracing::trace!("(utilize_multi) Read multishot entry: {:?}", e);
                    *ctr -= 1;
                    arena.inner.recycle_batch(buf_id);
                    return true;
                }
                None => {
                    //bail!("no IORING_CQE_F_BUFFER: {:?}", e);
                }
            }
        }
        false
    }
}
