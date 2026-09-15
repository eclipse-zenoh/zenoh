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

use std::{
    cell::{Cell, UnsafeCell},
    os::fd::RawFd,
    sync::Arc,
};

use tokio::sync::mpsc::UnboundedSender;
use zenoh_result::ZResult;

use crate::{api::reader::rx_buffer::RxBuffer, reader::buffer_group::BufferGroup};

type RxCallbackImpl = dyn FnMut(Arc<RxBuffer>) -> ZResult<()> + Send + 'static;

#[derive(Debug)]
pub(crate) struct RxCallback {
    pub callback: UnsafeCell<Box<RxCallbackImpl>>,
}

impl RxCallback {
    pub fn new(callback: UnsafeCell<Box<RxCallbackImpl>>) -> Self {
        Self { callback }
    }
}

#[derive(Debug)]
pub(crate) struct Rx {
    cb: RxCallback,
    pub fd: RawFd,
    error_sender: UnboundedSender<zenoh_result::Error>,
    buffer_group: BufferGroup,
    /// Whether a multishot receive of this task is still in flight.
    pub recv_live: Cell<bool>,
}

/// Buffer group of a stopped task, kept until the kernel has handed back
/// every buffer. Dropping it closes the error channel, which is what
/// `ReadTask::stop` waits for.
#[derive(Debug)]
pub(crate) struct Retiring {
    pub buffer_group: BufferGroup,
    _error_sender: UnboundedSender<zenoh_result::Error>,
}

impl Rx {
    pub fn new(
        fd: RawFd,
        cb: RxCallback,
        error_sender: UnboundedSender<zenoh_result::Error>,
        buffer_group: BufferGroup,
    ) -> Self {
        let rx = Self {
            cb,
            error_sender,
            fd,
            buffer_group,
            recv_live: Cell::new(true),
        };
        tracing::debug!("RX context created: {:?}", rx);
        rx
    }

    pub(crate) fn buffer_group(&self) -> &BufferGroup {
        &self.buffer_group
    }

    /// Stop the task: the callback is dropped now, the buffer group outlives
    /// the context until its buffers are reclaimed.
    pub(crate) fn into_retiring(self) -> Retiring {
        tracing::debug!("Destroy RX context: {:?}", self);
        Retiring {
            buffer_group: self.buffer_group,
            _error_sender: self.error_sender,
        }
    }

    pub(crate) fn run_callback(&self, buffer: Arc<RxBuffer>) {
        #[cfg(feature = "uring_trace")]
        tracing::trace!("CB begin....");
        let callback = unsafe { &mut *self.cb.callback.get() };
        #[cfg(feature = "uring_trace")]
        tracing::trace!("CB end....");

        if let Err(e) = (callback)(buffer) {
            self.post_error(e);
        }
    }

    pub(crate) fn post_error(&self, error: zenoh_result::Error) {
        tracing::error!("Read task error: {error}");
        let _ = self.error_sender.send(error);
    }
}
