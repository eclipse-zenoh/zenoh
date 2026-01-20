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
    ops::Neg,
    os::fd::{AsRawFd, RawFd},
    sync::{atomic::AtomicBool, mpsc::Sender, Arc},
};

use io_uring::{opcode, squeue::Flags, types, IoUring, SubmissionQueue};

use crate::{batch_arena::BatchArena, BUF_SIZE};

struct RxCallback {
    pub callback: UnsafeCell<Box<dyn FnMut(&[u8]) + 'static>>,
}

impl RxCallback {
    fn new(callback: UnsafeCell<Box<dyn FnMut(&[u8]) + 'static>>) -> Self {
        Self { callback }
    }
}

unsafe impl Send for RxCallback {}
unsafe impl Sync for RxCallback {}

struct Rx {
    pub callback: RxCallback,
    pub fd: RawFd,
    stop_flag: AtomicBool,
}

impl Rx {
    fn new(callback: Box<dyn FnMut(&[u8])>, fd: RawFd) -> Self {
        Self {
            callback: RxCallback::new(UnsafeCell::new(callback)),
            fd,
            stop_flag: AtomicBool::new(false),
        }
    }
}

pub struct ReadTask {
    rx: Arc<Rx>,
    user_data: u64,
    inner: Arc<ReaderInner>,
}

impl Drop for ReadTask {
    fn drop(&mut self) {
        self.rx
            .stop_flag
            .store(true, std::sync::atomic::Ordering::SeqCst);

        let event = opcode::AsyncCancel::new(self.user_data).build();

        let _ = self.inner.submit(event);
    }
}

impl ReadTask {
    fn new<Cb>(fd: RawFd, callback: Cb, inner: Arc<ReaderInner>) -> ZResult<Self>
    where
        Cb: FnMut(&[u8]) + 'static,
    {
        let rx = Arc::new(Rx::new(Box::new(callback), fd.clone()));
        let user_data = unsafe { std::mem::transmute(rx.clone()) };

        let recv = opcode::RecvMulti::new(types::Fd(fd), 0)
            .build()
            .user_data(user_data);

        inner.submit(recv)?;

        Ok(Self {
            rx,
            user_data,
            inner,
        })
    }
}

pub struct ReaderInner {
    waker: EventFd,
    sender: Sender<io_uring::squeue::Entry>,
    exit_flag: Arc<AtomicBool>,
}

impl ReaderInner {
    fn new(
        waker: EventFd,
        sender: Sender<io_uring::squeue::Entry>,
        exit_flag: Arc<AtomicBool>,
    ) -> Self {
        Self {
            waker,
            sender,
            exit_flag,
        }
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
        self.waker.write(0).unwrap();
    }
}

impl Drop for ReaderInner {
    fn drop(&mut self) {
        self.exit_flag
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.wake_reader_thread();
    }
}

#[derive(Clone)]
pub struct Reader {
    inner: Arc<ReaderInner>,
}

impl Reader {
    pub fn setup_read<Cb>(&self, fd: RawFd, callback: Cb) -> ZResult<ReadTask>
    where
        Cb: FnMut(&[u8]) + 'static,
    {
        ReadTask::new(fd, callback, self.inner.clone())
    }

    pub fn new() -> Self {
        // io_uring read
        let ring = IoUring::builder()
            .setup_submit_all()
            //.setup_sqpoll(1)
            //.setup_iopoll()
            //.setup_sqpoll_cpu(0)
            .setup_coop_taskrun()
            //.setup_defer_taskrun()
            //.setup_single_issuer()
            .build((1024).try_into().unwrap()).unwrap();

        // register eventfd to wake io_uring when necessary
        let waker = nix::sys::eventfd::EventFd::from_value_and_flags(
            0,
            EfdFlags::EFD_CLOEXEC | EfdFlags::EFD_NONBLOCK,
        )
        .unwrap();
        ring.submitter()
            .register_eventfd(waker.as_raw_fd())
            .unwrap();

        let mut arena = BatchArena::new(64);
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

        let (sender, receiver) = std::sync::mpsc::channel();

        let exit_flag = Arc::new(AtomicBool::new(false));

        let c_exit_flag = exit_flag.clone();
        let _handle = std::thread::spawn(move || {
            loop {
                let mut sq = unsafe { ring.submission_shared() };
                let mut cq = unsafe { ring.completion_shared() };
                while let Some(e) = cq.next() {
                    //println!("e: {:?}", e);

                    if e.user_data() == 0 {
                        println!("Zero-user-data entry: {:?}", e);
                        continue;
                    }

                    let rx: &Arc<Rx> = unsafe { std::mem::transmute(&mut e.user_data()) };

                    let need_submit = Reader::read_multi(&e, rx, &mut arena, &mut sq);

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

        let inner = Arc::new(ReaderInner::new(waker, sender, exit_flag));

        Self { inner }
    }

    fn read_multi(
        e: &io_uring::cqueue::Entry,
        rx: &Rx,
        arena: &mut BatchArena,
        sq: &mut SubmissionQueue<'_>,
    ) -> bool {
        let mut need_submit = false;
        if e.result() < 0 {
            match e.result().neg() {
                libc::ENOBUFS => {
                    // We are out of buffers
                    println!("ENOBUFS: Restart multishot receive!!!");

                    let provide_buffers = arena.provide_root_buffers().flags(Flags::SKIP_SUCCESS);
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
                        let data = &mut arena[buf_id as usize][0..buf_len];

                        let callback = unsafe { &mut *rx.callback.callback.get() };
                        callback(data);

                        let provide_buffers = opcode::ProvideBuffers::new(
                            data.as_mut_ptr(),
                            BUF_SIZE as i32,
                            1,
                            0,
                            buf_id as u16,
                        )
                        .build()
                        .flags(Flags::SKIP_SUCCESS);

                        unsafe { sq.push(&provide_buffers).unwrap() };
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
