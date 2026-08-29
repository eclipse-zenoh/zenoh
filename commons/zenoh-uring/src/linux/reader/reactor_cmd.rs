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

use std::{os::fd::RawFd, sync::Arc};

use tokio::sync::mpsc::UnboundedSender;

use crate::reader::{index::IndexGeneration, rx_context::RxCallback};

#[derive(Debug)]
pub(crate) enum ReactorCmd {
    StartRx(
        RawFd,
        RxCallback,
        Arc<tokio::sync::SetOnce<IndexGeneration>>,
        UnboundedSender<zenoh_result::Error>,
    ),
    StopRx(IndexGeneration),
}
