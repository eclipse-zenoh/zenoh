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

use std::sync::Arc;

use crate::api::handlers::{callback::Callback, IntoHandler, API_DATA_RECEPTION_CHANNEL_SIZE};

/// An handler implementing FIFO semantics.
///
/// Note that pushing on a full [`FifoChannel`] that is full will block until a slot is available.
/// E.g., a slow subscriber could block the underlying Zenoh thread because is not emptying the
/// [`FifoChannel`] fast enough. In this case, you may want to look into [`crate::api::handlers::RingChannel`] that
/// will drop samples when full.
pub struct FifoChannel {
    capacity: usize,
}

impl FifoChannel {
    /// Initialize the [`FifoChannel`] with the capacity size.
    pub fn new(capacity: usize) -> Self {
        Self { capacity }
    }
}

impl Default for FifoChannel {
    fn default() -> Self {
        Self::new(*API_DATA_RECEPTION_CHANNEL_SIZE)
    }
}

impl<T: Send + 'static> IntoHandler<T> for FifoChannel {
    type Handler = flume::Receiver<T>;

    fn into_handler(self) -> (Callback<T>, Self::Handler) {
        flume::bounded(self.capacity).into_handler()
    }
}

impl<T: Clone + Send + Sync + 'static> IntoHandler<T>
    for (std::sync::mpsc::SyncSender<T>, std::sync::mpsc::Receiver<T>)
{
    type Handler = std::sync::mpsc::Receiver<T>;

    fn into_handler(self) -> (Callback<T>, Self::Handler) {
        let (sender, receiver) = self;
        (
            Callback::new(Arc::new(move |t| {
                if let Err(error) = sender.send(t.clone()) {
                    tracing::error!(%error)
                }
            })),
            receiver,
        )
    }
}
