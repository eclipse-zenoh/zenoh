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

use std::{cell::UnsafeCell, os::fd::RawFd, sync::Arc};

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
        };
        tracing::debug!("RX context created: {:?}", rx);
        rx
    }

    pub(crate) fn buffer_group(&self) -> &BufferGroup {
        &self.buffer_group
    }

    pub(crate) fn run_callback(&self, buffer: Arc<RxBuffer>) {
        tracing::trace!("CB begin....");
        let callback = unsafe { &mut *self.cb.callback.get() };
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

impl Drop for Rx {
    fn drop(&mut self) {
        tracing::debug!("Destroy RX context: {:?}", self);
    }
}
