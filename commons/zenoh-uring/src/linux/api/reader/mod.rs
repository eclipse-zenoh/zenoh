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

pub mod fragmented_batch;
pub mod read_task;
pub mod rx_buffer;

use std::{
    collections::HashMap,
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
        rx_context::{Retiring, Rx},
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
        let batch_size = batch_size + batch_size / 2; // add some headroom to reduce ENOBUFS errors

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
            // Buffer groups of stopped tasks, keyed by the user_data of the
            // completion they wait for: the task's own terminal receive
            // completion first, then the RemoveBuffers completion.
            let mut retiring: HashMap<u64, Retiring> = HashMap::new();

            // io_uring read
            let ring: IoUring<squeue::Entry, cqueue::Entry> = IoUring::builder()
                .setup_submit_all()
                .setup_defer_taskrun()
                .setup_single_issuer()
                .build(4096)?;
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
                retiring: &mut HashMap<u64, Retiring>,
                arena: &GroupedArena,
                sq: &mut SubmissionQueue<'_>,
                batch_count: BufferCount,
            ) -> ZResult<()> {
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

                            // Issued inline (not ASYNC): on a socket without
                            // data the receive is registered for polling before
                            // this submission returns, where a cancellation
                            // finds it. A receive forced onto io-wq can be
                            // missed by the cancellation while it starts, and
                            // would then outlive its task.
                            let recv = opcode::RecvMulti::new(types::Fd(fd), group_id)
                                .build()
                                .user_data(index.into());

                            unsafe { sq.push(&recv)? }
                        }
                        ReactorCmd::StopRx(index_generation) => {
                            let Some(rx) = context_storage.take(index_generation) else {
                                continue;
                            };
                            let recv_live = rx.recv_live.get();
                            let group = rx.into_retiring();
                            if recv_live {
                                // wait for the receive to terminate before
                                // touching its buffers
                                retiring.insert(index_generation.into(), group);

                                let cancel_builder =
                                    types::CancelBuilder::user_data(index_generation.into()).all();
                                let event = AsyncCancel2::new(cancel_builder)
                                    .build()
                                    .user_data(IndexGeneration::INVALID_MIN);
                                unsafe { sq.push(&event)? }
                            } else {
                                Reader::retire(index_generation, group, retiring, sq)?;
                            }
                        }
                    }
                }

                Ok(())
            }

            loop {
                #[cfg(feature = "uring_trace")]
                let mut i = 0;

                while let Some(e) = unsafe { ring.completion_shared() }.next() {
                    let mut sq = unsafe { ring.submission_shared() };

                    roll_cmds(
                        &receiver,
                        &mut context_storage,
                        &mut retiring,
                        &arena,
                        &mut sq,
                        batch_count,
                    )?;

                    match e.user_data() {
                        IndexGeneration::INVALID_MIN => {
                            tracing::debug!("Zero-user-data entry: {:?}", e);
                        }
                        user_data if IndexGeneration::is_retire_user_data(user_data) => {
                            if let Some(group) = retiring.remove(&user_data) {
                                group.buffer_group.buffers_removed(e.result());
                            }
                        }
                        IndexGeneration::INVALID_MAX => {
                            tracing::debug!("Waker event: {:?}", e);
                            let _ = c_waker.read()?;
                            unsafe { sq.push(&waker_read)? };
                        }
                        index => {
                            #[cfg(feature = "uring_trace")]
                            {
                                i += 1;
                            }

                            let index = unsafe { IndexGeneration::new_unchecked(index) };
                            let to_submit = Reader::multi(
                                &context_storage,
                                &mut retiring,
                                &e,
                                index,
                                &arena,
                                &mut sq,
                            )?;
                            let len = sq.len() as u32;
                            if to_submit || len >= (batch_count / 2) as u32 {
                                drop(sq);
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

                #[cfg(feature = "uring_trace")]
                tracing::info!("Processed {} completion entries", i);

                // receive external submissions
                let mut sq = unsafe { ring.submission_shared() };
                roll_cmds(
                    &receiver,
                    &mut context_storage,
                    &mut retiring,
                    &arena,
                    &mut sq,
                    batch_count,
                )?;
                drop(sq);

                if c_exit_flag.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }

                // this wait can be interrupted by Self::wake_reader_thread
                ring.submit_and_wait(1)?;
            }
            Ok(())
        };

        ZRuntime::RX.spawn_blocking(move || {
            if let Err(e) = ring_worker() {
                tracing::error!("Uring reactor error: {e}");
                let _ = join_sender.send(e.to_string());
            }
            tracing::debug!("Uring reactor thread finished!");
        });

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

    /// No receive of the stopped task is in flight anymore: reclaim the
    /// buffers still queued in its group.
    fn retire(
        index: IndexGeneration,
        group: Retiring,
        retiring: &mut HashMap<u64, Retiring>,
        sq: &mut SubmissionQueue<'_>,
    ) -> ZResult<()> {
        let user_data = index.retire_user_data();
        if group.buffer_group.remove_buffers(user_data, sq)? {
            retiring.insert(user_data, group);
        }
        Ok(())
    }

    fn multi(
        context_storage: &RxContextStorage,
        retiring: &mut HashMap<u64, Retiring>,
        e: &io_uring::cqueue::Entry,
        index: IndexGeneration,
        arena: &GroupedArena,
        sq: &mut SubmissionQueue<'_>,
    ) -> ZResult<bool> {
        // last completion of a multishot receive
        let terminal = !cqueue::more(e.flags());
        match context_storage.get(index) {
            Some(context) => {
                if terminal {
                    // set again by read_multi if it queues a new receive
                    context.recv_live.set(false);
                }
                match Reader::read_multi(e, index, context, sq) {
                    Ok(val) => Ok(val),
                    Err(e) => {
                        context.post_error(e);
                        Ok(false)
                    }
                }
            }
            None => {
                let user_data: u64 = index.into();
                let consumed = Self::utilize_multi(e, retiring.get(&user_data), arena);
                if terminal {
                    if let Some(group) = retiring.remove(&user_data) {
                        Self::retire(index, group, retiring, sq)?;
                        return Ok(true);
                    }
                }
                Ok(consumed)
            }
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
            tracing::debug!("Error entry: {:?}", e);

            match e.result().neg() {
                libc::ENOBUFS => {
                    // We are out of buffers
                    tracing::debug!("ENOBUFS: Restart multishot receive for task {:?}", index);

                    let recv =
                        opcode::RecvMulti::new(types::Fd(context.fd), context.buffer_group().id())
                            .build()
                            .user_data(index.into());

                    unsafe { sq.push(&recv)? };
                    context.recv_live.set(true);
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
            if let Some(buf_id) = io_uring::cqueue::buffer_select(e.flags()) {
                #[cfg(feature = "uring_trace")]
                tracing::trace!("Read multishot entry: {:?}", e);

                if !io_uring::cqueue::more(e.flags()) {
                    tracing::debug!("IORING_CQE_F_BUFFER: Restart multishot receive!!!");

                    let recv =
                        opcode::RecvMulti::new(types::Fd(context.fd), context.buffer_group().id())
                            .build()
                            .user_data(index.into());

                    unsafe { sq.push(&recv)? };
                    context.recv_live.set(true);
                    need_submit = true;
                }

                let buf_len = e.result() as usize;
                let buffer = Arc::new(context.buffer_group().read_buffer(buf_id, buf_len, sq)?);
                context.run_callback(buffer);
            }
        }
        Ok(need_submit)
    }

    fn utilize_multi(
        e: &io_uring::cqueue::Entry,
        group: Option<&Retiring>,
        arena: &GroupedArena,
    ) -> bool {
        if e.result() >= 0 {
            if let Some(buf_id) = io_uring::cqueue::buffer_select(e.flags()) {
                tracing::trace!("(utilize_multi) Read multishot entry: {:?}", e);
                match group {
                    Some(group) => group.buffer_group.release_consumed(buf_id),
                    None => arena.recycle_batch(buf_id),
                }
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::UnsafeCell,
        collections::HashMap,
        io::Write,
        os::{fd::AsRawFd, unix::net::UnixStream},
        sync::Arc,
    };

    use io_uring::{cqueue, opcode, squeue, types, IoUring};
    use nix::sys::eventfd::{EfdFlags, EventFd};

    use super::Reader;
    use crate::{
        api::types::BufferCount,
        batch_arena::BatchArena,
        reader::{
            buffer_group::{BufferGroup, GroupedArena},
            reservable_arena::ReservableArena,
            rx_context::{Retiring, Rx, RxCallback},
            rx_context_storage::RxContextStorage,
            submission::SubmissionIface,
        },
    };

    /// A task whose receive just completed with a buffer and without
    /// `F_MORE`: `Reader::multi` must queue a replacement receive and mark
    /// it live even if replenishing the buffer group fails afterwards
    /// because the submission queue is full.
    ///
    /// `free_slots` is the number of SQ entries left for `Reader::multi`.
    fn terminal_buffer_completion(free_slots: usize) -> bool {
        let mut ring: IoUring<squeue::Entry, cqueue::Entry> = IoUring::new(4).unwrap();
        let arena = BatchArena::new(64, 1, BufferCount::MAX).unwrap();
        let waker = Arc::new(EventFd::from_value_and_flags(0, EfdFlags::EFD_CLOEXEC).unwrap());
        let (cmd_tx, _cmd_rx) = flume::unbounded();
        let arena = ReservableArena::new(arena, SubmissionIface::new(waker, cmd_tx));
        let arena = GroupedArena::new(arena);
        let (peer, mut writer) = UnixStream::pair().unwrap();
        let mut storage = RxContextStorage::new();
        let mut retiring: HashMap<u64, Retiring> = HashMap::new();
        let (err_tx, mut err_rx) = tokio::sync::mpsc::unbounded_channel();

        // one task with a one-buffer group and a one-shot receive on it
        let group = BufferGroup::new(&arena, 1, &mut ring.submission()).unwrap();
        let recv = opcode::Recv::new(types::Fd(peer.as_raw_fd()), std::ptr::null_mut(), 0)
            .buf_group(group.id())
            .build()
            .flags(squeue::Flags::BUFFER_SELECT);
        let callback = RxCallback::new(UnsafeCell::new(Box::new(|_| Ok(()))));
        let index = storage.alloc(Rx::new(peer.as_raw_fd(), callback, err_tx, group));
        unsafe {
            ring.submission()
                .push(&recv.user_data(index.into()))
                .unwrap()
        };
        writer.write_all(b"data").unwrap();
        ring.submit_and_wait(1).unwrap();
        let cqe = ring.completion().next().unwrap();
        assert!(cqueue::buffer_select(cqe.flags()).is_some() && !cqueue::more(cqe.flags()));

        // leave exactly `free_slots` entries for the handler
        let mut sq = ring.submission();
        while sq.capacity() - sq.len() > free_slots {
            unsafe { sq.push(&opcode::Nop::new().build()).unwrap() };
        }
        let result = Reader::multi(&storage, &mut retiring, &cqe, index, &arena, &mut sq);
        assert!(result.is_ok(), "handler errors are posted, not returned");
        assert_eq!(
            sq.capacity() - sq.len(),
            0,
            "the handler used the free slots"
        );
        assert!(err_rx.try_recv().is_ok(), "the failed push was reported");
        storage.get(index).unwrap().recv_live.get()
    }

    #[test]
    fn restart_queued_before_replenishment_fails_stays_live() {
        assert!(terminal_buffer_completion(1));
    }

    #[test]
    fn failed_restart_is_not_live() {
        assert!(!terminal_buffer_completion(0));
    }
}
