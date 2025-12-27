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

use std::{ops::Neg, os::fd::RawFd, sync::Arc};

use io_uring::{cqueue, opcode, squeue::Flags, types, IoUring, SubmissionQueue};

use crate::{BUF_SIZE, batch_arena::BatchArena};

struct Rx {
    pub callback: Box<dyn FnMut(&[u8]) + 'static>,
    pub fd: RawFd,
}

impl Rx {
    fn new(callback: Box<dyn FnMut(&[u8])>, fd: RawFd) -> Self {
        Self { callback, fd }
    }
}

/*
|____________________ARENA______________________|
|__Segment__|__Segment__|__Segment__|__Segment__|
|buf|buf|buf|buf|buf|buf|buf|buf|buf|buf|buf|buf|

Ringbuf reclamation happens on Segment basis

*/

/*
struct BatchSegment {
}

struct BatchStorage {
    arena: BatchArena,
    segments: Vec<Arc<BatchSegment>>
}
*/

enum UserData {
    ReadMulti(Rx),
}

pub struct Reader {
    ring: Arc<IoUring>,
}

impl Reader {
    pub fn setup_read<Cb>(&self, fd: RawFd, callback: Cb)
    where
        Cb: FnMut(&[u8]) + 'static,
    {
        let boxed_rx = Box::new(UserData::ReadMulti(Rx::new(Box::new(callback), fd.clone())));
        let user_data = unsafe { std::mem::transmute(boxed_rx) };
        let recv = opcode::RecvMulti::new(types::Fd(fd), 0)
            .build()
            .user_data(user_data);

        unsafe { self.ring.submission_shared().push(&recv).unwrap() };

        self.ring.submit().unwrap();
    }

    pub fn new(ring: Arc<IoUring>) -> Self {
        let mut arena = BatchArena::new(64);
        {
            {
                let provide_buffers = arena.provide_root_buffers();
                unsafe { ring.submission_shared().push(&provide_buffers).unwrap() };
            }
            ring.submit_and_wait(1).unwrap();

            let cq = unsafe {
                ring.completion_shared()
                    .next()
                    .expect("completion queue is empty")
            };
            assert!(cq.result() == 0);
        }

        let c_ring = ring.clone();
        let _handle = std::thread::spawn(move || {
            loop {
                let mut cq = unsafe { c_ring.completion_shared() };
                let mut sq = unsafe { c_ring.submission_shared() };
                while let Some(e) = cq.next() {
                    //println!("e: {:?}", e);

                    if e.user_data() == 0 {
                        println!("Zero-user-data entry: {:?}", e);
                        continue;
                    }

                    let user_data: &mut Box<UserData> =
                        unsafe { std::mem::transmute(&mut e.user_data()) };

                    let need_submit = match &mut **user_data {
                        UserData::ReadMulti(rx) => Reader::read_multi(&e, rx, &mut arena, &mut sq),
                    };

                    if need_submit {
                        sq.sync();
                        c_ring.submit().unwrap();
                        cq.sync();
                    }
                }
                drop(sq);
                drop(cq);

                c_ring.submit_and_wait(1).unwrap();
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

        Self { ring }
    }

    fn read_multi(
        e: &io_uring::cqueue::Entry,
        rx: &mut Rx,
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
            match cqueue::buffer_select(e.flags()) {
                Some(buf_id) => {
                    //println!("Read multishot entry: {:?}", e);

                    if !cqueue::more(e.flags()) {
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
                        (rx.callback)(data);

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
