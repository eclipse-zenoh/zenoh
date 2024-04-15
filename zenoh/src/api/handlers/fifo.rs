//
// Copyright (c) 2023 ZettaScale Technology
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

//! Callback handler trait.
use super::{callback::Callback, Dyn, IntoHandler, API_DATA_RECEPTION_CHANNEL_SIZE};

/// The default handler in Zenoh is a FIFO queue.

pub struct FifoChannel {
    capacity: usize,
}

impl FifoChannel {
    /// Initialize the RingBuffer with the capacity size.
    pub fn new(capacity: usize) -> Self {
        Self { capacity }
    }
}

impl Default for FifoChannel {
    fn default() -> Self {
        Self::new(*API_DATA_RECEPTION_CHANNEL_SIZE)
    }
}

impl<T: Send + 'static> IntoHandler<'static, T> for FifoChannel {
    type Handler = flume::Receiver<T>;

    fn into_handler(self) -> (Callback<'static, T>, Self::Handler) {
        flume::bounded(self.capacity).into_handler()
    }
}

impl<T: Send + Sync + 'static> IntoHandler<'static, T>
    for (std::sync::mpsc::SyncSender<T>, std::sync::mpsc::Receiver<T>)
{
    type Handler = std::sync::mpsc::Receiver<T>;

    fn into_handler(self) -> (Callback<'static, T>, Self::Handler) {
        let (sender, receiver) = self;
        (
            Dyn::new(move |t| {
                if let Err(e) = sender.send(t) {
                    log::error!("{}", e)
                }
            }),
            receiver,
        )
    }
}
