//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
mod batch;
mod rx;
mod tx;

use super::{SeqNumGenerator, SessionTransport};
use crate::core::ZInt;
use crate::io::{ArcSlice, RBuf};
use crate::link::Link;
use crate::proto::{SessionMessage, ZenohMessage};
use crate::session::defaults::QUEUE_PRIO_DATA;
use async_std::channel::{bounded, Receiver, Sender};
use async_std::prelude::*;
use async_std::sync::{Arc, Mutex};
use async_std::task;
use batch::*;
use std::convert::TryInto;
use std::time::Duration;
use tx::*;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::{zasynclock, zerror};

#[derive(Clone)]
pub(crate) struct SessionTransportLink {
    // The underlying link
    pub(super) inner: Link,
    // The session lease in seconds
    pub(super) lease: ZInt,
    // Keep alive interval
    pub(super) keep_alive: ZInt,
    // The transport this link is associated to
    transport: SessionTransport,
    // The transmission pipeline
    pipeline: Arc<TransmissionPipeline>,
    // The signals to stop TX/RX tasks
    signal_tx: Arc<Mutex<Option<Sender<ZResult<()>>>>>,
    signal_rx: Arc<Mutex<Option<Sender<ZResult<()>>>>>,
}

impl SessionTransportLink {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        transport: SessionTransport,
        link: Link,
        batch_size: usize,
        keep_alive: ZInt,
        lease: ZInt,
        sn_reliable: Arc<Mutex<SeqNumGenerator>>,
        sn_best_effort: Arc<Mutex<SeqNumGenerator>>,
    ) -> SessionTransportLink {
        // The queue
        let pipeline = Arc::new(TransmissionPipeline::new(
            batch_size.min(link.get_mtu()),
            link.is_streamed(),
            sn_reliable,
            sn_best_effort,
        ));

        SessionTransportLink {
            transport,
            inner: link,
            lease,
            keep_alive,
            pipeline,
            signal_tx: Arc::new(Mutex::new(None)),
            signal_rx: Arc::new(Mutex::new(None)),
        }
    }
}

impl SessionTransportLink {
    pub(crate) async fn start_tx(&self) {
        let mut guard = zasynclock!(self.signal_tx);
        if guard.is_none() {
            // SessionTransport for signal the termination of TX task
            let (sender_tx, receiver_tx) = bounded::<ZResult<()>>(1);
            *guard = Some(sender_tx);
            // Spawn the TX task
            let c_link = self.clone();
            task::spawn(async move {
                // Start the consume task
                let res = tx_task(c_link.clone(), receiver_tx).await;
                if let Err(e) = res {
                    log::debug!("{}", e);
                    let _ = c_link.transport.del_link(&c_link.inner).await;
                }
            });
        }
    }

    pub(crate) async fn stop_tx(&self) {
        let mut guard = zasynclock!(self.signal_tx);
        if let Some(signal_tx) = guard.take() {
            let _ = signal_tx.send(Ok(())).await;
        }
    }

    pub(crate) async fn start_rx(&self) {
        let mut guard = zasynclock!(self.signal_rx);
        if guard.is_none() {
            let (sender_rx, receiver_rx) = bounded::<ZResult<()>>(1);
            *guard = Some(sender_rx);
            // Spawn the RX task
            let c_link = self.clone();
            task::spawn(async move {
                // Start the consume task
                let res = rx_task(c_link.clone(), receiver_rx).await;
                if let Err(e) = res {
                    log::debug!("{}", e);
                    let _ = c_link.transport.del_link(&c_link.inner).await;
                }
            });
        }
    }

    pub(crate) async fn stop_rx(&self) {
        let mut guard = zasynclock!(self.signal_tx);
        if let Some(signal_rx) = guard.take() {
            let _ = signal_rx.send(Ok(())).await;
        }
    }

    #[inline]
    pub(crate) fn get_link(&self) -> &Link {
        &self.inner
    }

    #[inline]
    pub(crate) async fn schedule_zenoh_message(&self, msg: ZenohMessage, priority: usize) {
        self.pipeline.push_zenoh_message(msg, priority).await;
    }

    #[inline]
    pub(crate) async fn schedule_session_message(&self, msg: SessionMessage, priority: usize) {
        self.pipeline.push_session_message(msg, priority).await;
    }

    pub(crate) async fn close(self) -> ZResult<()> {
        // Send the signal
        let _ = self.stop_tx().await;
        let _ = self.stop_rx().await;

        // Drain what remains in the queue before exiting
        while let Some(batch) = self.pipeline.drain().await {
            let _ = self.inner.write_all(batch.get_buffer()).await;
        }

        // Close the underlying link
        self.inner.close().await
    }
}

/*************************************/
/*              TASKS                */
/*************************************/
async fn tx_task(link: SessionTransportLink, stop: Receiver<ZResult<()>>) -> ZResult<()> {
    let mut result: ZResult<()> = Ok(());

    // Keep draining the queue
    let consume = async {
        let keep_alive = Duration::from_millis(link.keep_alive);
        loop {
            // Pull a serialized batch from the queue
            match link.pipeline.pull().timeout(keep_alive).await {
                Ok((batch, index)) => {
                    // Send the buffer on the link
                    result = link.inner.write_all(batch.get_buffer()).await;
                    if result.is_err() {
                        break Ok(Ok(()));
                    }
                    // Reinsert the batch into the queue
                    link.pipeline.refill(batch, index).await;
                }
                Err(_) => {
                    let pid = None;
                    let attachment = None;
                    let message = SessionMessage::make_keep_alive(pid, attachment);
                    link.pipeline
                        .push_session_message(message, QUEUE_PRIO_DATA)
                        .await;
                }
            }
        }
    };
    let _ = consume.race(stop.recv()).await;

    result
}

async fn rx_task(link: SessionTransportLink, stop: Receiver<ZResult<()>>) -> ZResult<()> {
    let read_loop = async {
        // The RBuf to read a message batch onto
        let mut rbuf = RBuf::new();

        // The buffer allocated to read from a single syscall
        let mut buffer = vec![0u8; link.inner.get_mtu()];

        // The vector for storing the deserialized messages
        let mut messages: Vec<SessionMessage> = Vec::with_capacity(1);

        // An example of the received buffer and the correspoding indexes is:
        //
        //  0 1 2 3 4 5 6 7  ..  n 0 1 2 3 4 5 6 7      k 0 1 2 3 4 5 6 7      x
        // +-+-+-+-+-+-+-+-+ .. +-+-+-+-+-+-+-+-+-+ .. +-+-+-+-+-+-+-+-+-+ .. +-+
        // | L | First batch      | L | Second batch     | L | Incomplete batch |
        // +-+-+-+-+-+-+-+-+ .. +-+-+-+-+-+-+-+-+-+ .. +-+-+-+-+-+-+-+-+-+ .. +-+
        //
        // - Decoding Iteration 0:
        //      r_l_pos = 0; r_s_pos = 2; r_e_pos = n;
        //
        // - Decoding Iteration 1:
        //      r_l_pos = n; r_s_pos = n+2; r_e_pos = n+k;
        //
        // - Decoding Iteration 2:
        //      r_l_pos = n+k; r_s_pos = n+k+2; r_e_pos = n+k+x;
        //
        // In this example, Iteration 2 will fail since the batch is incomplete and
        // fewer bytes than the ones indicated in the length are read. The incomplete
        // batch is hence stored in a RBuf in order to read more bytes from the socket
        // and deserialize a complete batch. In case it is not possible to read at once
        // the 2 bytes indicating the message batch length (i.e., only the first byte is
        // available), the first byte is copied at the beginning of the buffer and more
        // bytes are read in the next iteration.

        // The read position of the length bytes in the buffer
        let mut r_l_pos: usize;
        // The start read position of the message bytes in the buffer
        let mut r_s_pos: usize;
        // The end read position of the messages bytes in the buffer
        let mut r_e_pos: usize;
        // The write position in the buffer
        let mut w_pos: usize = 0;

        // Keep track of the number of bytes still to read for incomplete message batches
        let mut left_to_read: usize = 0;

        // Macro to add a slice to the RBuf
        macro_rules! zaddslice {
            ($start:expr, $end:expr) => {
                let tot = $end - $start;
                let mut slice = Vec::with_capacity(tot);
                slice.extend_from_slice(&buffer[$start..$end]);
                rbuf.add_slice(ArcSlice::new(Arc::new(slice), 0, tot));
            };
        }

        // Macro for deserializing the messages
        macro_rules! zdeserialize {
            () => {
                // Deserialize all the messages from the current RBuf
                while rbuf.can_read() {
                    match rbuf.read_session_message() {
                        Some(msg) => messages.push(msg),
                        None => {
                            let e = format!("Decoding error on link: {}", link.inner);
                            return Ok(zerror!(ZErrorKind::IOError { descr: e }));
                        }
                    }
                }

                for msg in messages.drain(..) {
                    link.receive_message(msg).await;
                }
            };
        }

        let lease = Duration::from_secs(link.lease);
        loop {
            // Async read from the underlying link
            let res = match link.inner.read(&mut buffer[w_pos..]).timeout(lease).await {
                Ok(res) => res,
                Err(_) => {
                    // Link lease has expired
                    let e = format!(
                        "Link has expired after {} milliseconds: {}",
                        link.lease, link.inner
                    );
                    return Ok(zerror!(ZErrorKind::IOError { descr: e }));
                }
            };

            match res {
                Ok(mut n) => {
                    if n == 0 {
                        // Reading 0 bytes means error
                        let e = format!("Zero bytes reading on link: {}", link.inner);
                        return Ok(zerror!(ZErrorKind::IOError { descr: e }));
                    }

                    // If we had a w_pos different from 0, it means we add an incomplete length reading
                    // in the previous iteration: we have only read 1 byte instead of 2.
                    if w_pos != 0 {
                        // Update the number of read bytes by adding the bytes we have already read in
                        // the previous iteration. "n" now is the index pointing to the last valid
                        // position in the buffer.
                        n += w_pos;
                        // Reset the write index
                        w_pos = 0;
                    }

                    // Reset the read length index
                    r_l_pos = 0;

                    // Check if we had an incomplete message batch
                    if left_to_read > 0 {
                        // Check if still we haven't read enough bytes
                        if n < left_to_read {
                            // Update the number of bytes still to read;
                            left_to_read -= n;
                            // Copy the relevant buffer slice in the RBuf
                            zaddslice!(0, n);
                            // Keep reading from the socket
                            continue;
                        }
                        // We are ready to decode a complete message batch
                        // Copy the relevant buffer slice in the RBuf
                        zaddslice!(0, left_to_read);
                        // Read the batch
                        zdeserialize!();
                        // Update the read length index
                        r_l_pos = left_to_read;
                        // Reset the remaining bytes to read
                        left_to_read = 0;

                        // Check if we have completely read the batch
                        if buffer[r_l_pos..n].is_empty() {
                            // Reset the RBuf
                            rbuf.clear();
                            // Keep reading from the socket
                            continue;
                        }
                    }

                    // Loop over all the buffer which may contain multiple message batches
                    loop {
                        // Compute the total number of bytes we have read
                        let read = buffer[r_l_pos..n].len();
                        // Check if we have read the 2 bytes necessary to decode the message length
                        if read < 2 {
                            // Copy the bytes at the beginning of the buffer
                            buffer.copy_within(r_l_pos..n, 0);
                            // Update the write index
                            w_pos = read;
                            // Keep reading from the socket
                            break;
                        }
                        // We have read at least two bytes in the buffer, update the read start index
                        r_s_pos = r_l_pos + 2;
                        // Read the length as litlle endian from the buffer (array of 2 bytes)
                        let length: [u8; 2] = buffer[r_l_pos..r_s_pos].try_into().unwrap();
                        // Decode the total amount of bytes that we are expected to read
                        let to_read = u16::from_le_bytes(length) as usize;

                        // Check if we have really something to read
                        if to_read == 0 {
                            // Keep reading from the socket
                            break;
                        }
                        // Compute the number of useful bytes we have actually read
                        let read = buffer[r_s_pos..n].len();

                        if read == 0 {
                            // The buffer might be empty in case of having read only the two bytes
                            // of the length and no additional bytes are left in the reading buffer
                            left_to_read = to_read;
                            // Keep reading from the socket
                            break;
                        } else if read < to_read {
                            // We haven't read enough bytes for a complete batch, so
                            // we need to store the bytes read so far and keep reading

                            // Update the number of bytes we still have to read to
                            // obtain a complete message batch for decoding
                            left_to_read = to_read - read;

                            // Copy the buffer in the RBuf if not empty
                            zaddslice!(r_s_pos, n);

                            // Keep reading from the socket
                            break;
                        }

                        // We have at least one complete message batch we can deserialize
                        // Compute the read end index of the message batch in the buffer
                        r_e_pos = r_s_pos + to_read;

                        // Copy the relevant buffer slice in the RBuf
                        zaddslice!(r_s_pos, r_e_pos);
                        // Deserialize the batch
                        zdeserialize!();

                        // Reset the current RBuf
                        rbuf.clear();
                        // Reset the remaining bytes to read
                        left_to_read = 0;

                        // Check if we are done with the current reading buffer
                        if buffer[r_e_pos..n].is_empty() {
                            // Keep reading from the socket
                            break;
                        }

                        // Update the read length index to read the next message batch
                        r_l_pos = r_e_pos;
                    }
                }
                Err(e) => {
                    let e = format!("Reading error on link {}: {}", link.inner, e);
                    return Ok(zerror!(ZErrorKind::IOError { descr: e }));
                }
            }
        }
    };

    // Execute the read loop
    let race_res = read_loop.race(stop.recv()).await;
    match race_res {
        Ok(read_res) => read_res,
        Err(_) => Ok(()),
    }
}
