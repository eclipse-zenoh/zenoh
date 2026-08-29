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

use std::sync::Arc;

use flume::Sender;
use nix::sys::eventfd::EventFd;
use zenoh_result::ZResult;

use crate::reader::reactor_cmd::ReactorCmd;

#[derive(Debug, Clone)]
pub(crate) struct SubmissionIface {
    waker: Arc<EventFd>,
    sender: Sender<ReactorCmd>,
}

impl SubmissionIface {
    pub fn new(waker: Arc<EventFd>, sender: Sender<ReactorCmd>) -> Self {
        Self { waker, sender }
    }

    pub fn submit(&self, cmd: ReactorCmd) -> ZResult<()> {
        self.submit_quiet(cmd)?;
        self.wake_reader_thread()
    }

    fn submit_quiet(&self, cmd: ReactorCmd) -> ZResult<()> {
        tracing::debug!("Submit cmd: {:?}", cmd);
        self.sender.send(cmd).map_err(|e| e.to_string().into())
    }

    pub fn wake_reader_thread(&self) -> ZResult<()> {
        // A write() to an eventfd can (rarely) block if the internal counter is near overflow.
        // With non-blocking mode (EfdFlags::EFD_NONBLOCK), you’ll get EAGAIN instead of risking
        // a hang during shutdown logic.
        // TODO: handle EAGAIN or switch to blocking mode
        tracing::debug!("Waking reader thread");
        self.waker.write(1).map(|_| ()).map_err(|e| e.into())
    }
}
