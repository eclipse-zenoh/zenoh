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

use tokio::sync::mpsc::{error::TryRecvError, UnboundedReceiver};
use zenoh_core::zerror;
use zenoh_result::ZResult;

use crate::{
    api::reader::rx_buffer::RxBuffer,
    reader::{
        index::IndexGeneration, reactor_cmd::ReactorCmd, rx_context::RxCallback,
        submission::SubmissionIface,
    },
};

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

        // wait for the retirement of the task's buffers, or the reactor exit
        while self.error_receiver.recv().await.is_some() {}
    }

    pub(crate) async fn new<Cb>(
        fd: RawFd,
        callback: Cb,
        submitter: SubmissionIface,
    ) -> ZResult<Self>
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

#[cfg(test)]
mod tests {
    use std::{num::NonZeroU32, sync::Arc, time::Duration};

    use nix::sys::eventfd::{EfdFlags, EventFd};

    use super::ReadTask;
    use crate::reader::{
        index::IndexGeneration, reactor_cmd::ReactorCmd, submission::SubmissionIface,
    };

    /// `stop` returns once the reactor has retired the task, however long
    /// that takes.
    #[test]
    fn stop_waits_for_retirement() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let waker = Arc::new(EventFd::from_value_and_flags(0, EfdFlags::EFD_CLOEXEC).unwrap());
            let (cmd_tx, cmd_rx) = flume::unbounded();
            let (retire_tx, retire_rx) = std::sync::mpsc::channel::<()>();
            // registers the task, and retires it when told to
            let reactor = std::thread::spawn(move || {
                let mut retiring = Vec::new();
                while let Ok(cmd) = cmd_rx.recv() {
                    match cmd {
                        ReactorCmd::StartRx(_, _, index, error_sender) => {
                            index.set(IndexGeneration::new(0, NonZeroU32::MIN)).unwrap();
                            retiring.push(error_sender);
                        }
                        ReactorCmd::StopRx(_) => {
                            let _ = retire_rx.recv();
                            retiring.clear();
                        }
                    }
                }
            });
            let mut task = ReadTask::new(0, |_| Ok(()), SubmissionIface::new(waker, cmd_tx))
                .await
                .unwrap();
            let mut stop = Box::pin(task.stop());
            assert!(
                tokio::time::timeout(Duration::from_millis(1500), &mut stop)
                    .await
                    .is_err(),
                "stop() returned before the retirement"
            );
            retire_tx.send(()).unwrap();
            tokio::time::timeout(Duration::from_secs(5), stop)
                .await
                .expect("stop() did not return after the retirement");
            drop(retire_tx);
            drop(task);
            reactor.join().unwrap();
        });
    }
}
