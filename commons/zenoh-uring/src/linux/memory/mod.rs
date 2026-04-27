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
pub(crate) mod index;
pub(crate) mod reactor_cmd;
pub(crate) mod reservable_arena;
pub(crate) mod rx_context;
pub(crate) mod rx_context_storage;
pub(crate) mod submission;
pub(crate) mod window;

use std::sync::{atomic::AtomicBool, Arc};

use crate::reader::submission::SubmissionIface;

#[derive(Debug)]
pub(crate) struct ReaderInner {
    pub(crate) submitter: SubmissionIface,
    exit_flag: Arc<AtomicBool>,
}

impl ReaderInner {
    pub(crate) fn new(submitter: SubmissionIface, exit_flag: Arc<AtomicBool>) -> Self {
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
