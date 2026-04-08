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

pub mod fragmented_batch;
pub mod read_task;
pub mod rx_buffer;

use std::{
    ops::Neg,
    os::fd::{AsRawFd, RawFd},
    sync::{atomic::AtomicBool, Arc},
};

use flume::Receiver;
use io_uring::{
    cqueue,
    opcode::{self, AsyncCancel2},
    squeue, types, IoUring, SubmissionQueue,
};
use nix::sys::eventfd::EfdFlags;
//use thread_priority::{RealtimeThreadSchedulePolicy, ThreadBuilder, ThreadPriority};
use zenoh_core::bail;
use zenoh_result::ZResult;
use zenoh_runtime::ZRuntime;

use crate::{
    api::{
        reader::{fragmented_batch::FragmentedBatch, read_task::ReadTask, rx_buffer::RxBuffer},
        types::BufferCount,
    },
    batch_arena::BatchArena,
    reader::{
        buffer_group::{BufferGroup, GroupedArena},
        index::IndexGeneration,
        reactor_cmd::ReactorCmd,
        reservable_arena::ReservableArena,
        rx_context::Rx,
        rx_context_storage::RxContextStorage,
        submission::SubmissionIface,
        window::RxWindow,
        ReaderInner,
    },
};

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

    pub fn new(batch_size: usize, batch_count: BufferCount) -> ZResult<Self> {
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
            let ring: IoUring<squeue::Entry, cqueue::Entry> = IoUring::builder()
                .setup_submit_all()
                //.setup_sqpoll(1)
                //.setup_iopoll()
                //.setup_sqpoll_cpu(0)
                //.setup_coop_taskrun()
                .setup_defer_taskrun()
                .setup_single_issuer()
                .build((4096 /*batch_count*2*/).try_into()?)?;
            let arena = BatchArena::new(batch_size, batch_count, BufferCount::MAX)?;
            let arena = ReservableArena::new(arena, c_submitter);
            let arena = GroupedArena::new(arena);

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
                arena: &GroupedArena,
                sq: &mut SubmissionQueue<'_>,
                batch_count: BufferCount,
            ) -> ZResult<()> {
                //tracing::debug!("Reading cmds....");

                // receive external submissions
                while let Ok(val) = receiver.try_recv() {
                    tracing::debug!("Cmd: {:?}", val);

                    match val {
                        ReactorCmd::StartRx(fd, callback, set_once, error_sender) => {
                            let buffer_group = BufferGroup::new(arena, batch_count, sq)?;
                            let group_id = buffer_group.id();

                            let rx_context = Rx::new(fd, callback, error_sender, buffer_group);
                            let index = context_storage.alloc(rx_context);
                            set_once.set(index)?;

                            let recv = opcode::RecvMulti::new(types::Fd(fd), group_id)
                                .build()
                                //.flags(io_uring::squeue::Flags::ASYNC)
                                .user_data(index.into());

                            unsafe { sq.push(&recv)? }
                        }
                        ReactorCmd::StopRx(index_generation) => {
                            context_storage.free(index_generation);

                            let cancel_builder =
                                types::CancelBuilder::user_data(index_generation.into()).all();

                            let event = AsyncCancel2::new(cancel_builder)
                                .build()
                                .user_data(index_generation.into())
                                .flags(io_uring::squeue::Flags::ASYNC);

                            unsafe { sq.push(&event)? }
                        }
                    }
                }

                Ok(())
            }

            loop {
                while let Some(e) = unsafe { ring.completion_shared() }.next() {
                    let mut sq = unsafe { ring.submission_shared() };

                    roll_cmds(
                        &receiver,
                        &mut context_storage,
                        &arena,
                        &mut sq,
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
                            if Reader::multi(&context_storage, &e, index, &arena, &mut sq)? {
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

                // receive external submissions
                let mut sq = unsafe { ring.submission_shared() };
                roll_cmds(
                    &receiver,
                    &mut context_storage,
                    &arena,
                    &mut sq,
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
        arena: &GroupedArena,
        sq: &mut SubmissionQueue<'_>,
    ) -> ZResult<bool> {
        match context_storage.get(index) {
            Some(context) => match Reader::read_multi(e, index, context, sq) {
                Ok(val) => Ok(val),
                Err(e) => {
                    context.post_error(e);
                    Ok(false)
                }
            },
            None => Ok(Self::utilize_multi(e, arena)),
        }
    }

    fn read_multi(
        e: &io_uring::cqueue::Entry,
        index: IndexGeneration,
        context: &Rx,
        sq: &mut SubmissionQueue<'_>,
    ) -> ZResult<bool> {
        let mut need_submit = false;
        if e.result() < 0 {
            tracing::trace!("Error entry: {:?}", e);

            match e.result().neg() {
                libc::ENOBUFS => {
                    // We are out of buffers
                    tracing::debug!("ENOBUFS: Restart multishot receive for task {:?}", index);

                    let recv = opcode::RecvMulti::new(types::Fd(context.fd), context.buffer_group().id())
                            .build()
                            //.flags(io_uring::squeue::Flags::ASYNC)
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

                    if !io_uring::cqueue::more(e.flags()) {
                        tracing::debug!("IORING_CQE_F_BUFFER: Restart multishot receive!!!");

                        let recv = opcode::RecvMulti::new(
                            types::Fd(context.fd),
                            context.buffer_group().id(),
                        )
                        .build()
                        //.flags(io_uring::squeue::Flags::ASYNC)
                        .user_data(index.into());

                        unsafe { sq.push(&recv)? };
                        need_submit = true;
                    }

                    let buf_len = e.result() as usize;
                    let buffer = Arc::new(context.buffer_group().read_buffer(buf_id, buf_len, sq)?);
                    context.run_callback(buffer);
                }
                None => {
                    //bail!("no IORING_CQE_F_BUFFER: {:?}", e);
                }
            };
        }
        Ok(need_submit)
    }

    fn utilize_multi(e: &io_uring::cqueue::Entry, arena: &GroupedArena) -> bool {
        if e.result() >= 0 {
            match io_uring::cqueue::buffer_select(e.flags()) {
                Some(buf_id) => {
                    tracing::trace!("(utilize_multi) Read multishot entry: {:?}", e);
                    arena.recycle_batch(buf_id);
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
