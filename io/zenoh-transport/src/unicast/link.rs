//
// Copyright (c) 2022 ZettaScale Technology
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
use super::common::conduit::TransportConduitTx;
use super::transport::TransportUnicastInner;
#[cfg(feature = "stats")]
use super::TransportUnicastStatsAtomic;
use crate::common::pipeline::{
    TransmissionPipeline, TransmissionPipelineConf, TransmissionPipelineConsumer,
    TransmissionPipelineProducer,
};
use crate::TransportExecutor;
use async_std::prelude::FutureExt;
use async_std::task;
use async_std::task::JoinHandle;

#[cfg(feature = "transport_compression")]
use std::convert::TryInto;
use std::sync::Arc;
use std::time::Duration;
use std::vec;
use zenoh_buffers::reader::{HasReader, Reader};
#[cfg(feature = "transport_compression")]
use zenoh_buffers::writer::HasWriter;
use zenoh_buffers::ZSlice;
#[cfg(feature = "transport_compression")]
use zenoh_codec::WCodec;
use zenoh_codec::{RCodec, Zenoh060};
#[cfg(feature = "transport_compression")]
use zenoh_compression::ZenohCompress;
use zenoh_link::{LinkUnicast, LinkUnicastDirection};
use zenoh_protocol::transport::TransportMessage;
use zenoh_result::{bail, zerror, ZResult};
use zenoh_sync::{RecyclingObjectPool, Signal};

#[derive(Clone)]
pub(super) struct TransportLinkUnicast {
    // Inbound / outbound
    pub(super) direction: LinkUnicastDirection,
    // The underlying link
    pub(super) link: LinkUnicast,
    // The transmission pipeline
    pub(super) pipeline: Option<TransmissionPipelineProducer>,
    // The transport this link is associated to
    transport: TransportUnicastInner,
    // The signals to stop TX/RX tasks
    handle_tx: Option<Arc<async_executor::Task<()>>>,
    signal_rx: Signal,
    handle_rx: Option<Arc<JoinHandle<()>>>,
}

impl TransportLinkUnicast {
    pub(super) fn new(
        transport: TransportUnicastInner,
        link: LinkUnicast,
        direction: LinkUnicastDirection,
    ) -> TransportLinkUnicast {
        TransportLinkUnicast {
            direction,
            transport,
            link,
            pipeline: None,
            handle_tx: None,
            signal_rx: Signal::new(),
            handle_rx: None,
        }
    }
}

impl TransportLinkUnicast {
    pub(super) fn start_tx(
        &mut self,
        executor: &TransportExecutor,
        keep_alive: Duration,
        batch_size: u16,
        conduit_tx: &[TransportConduitTx],
    ) {
        if self.handle_tx.is_none() {
            let config = TransmissionPipelineConf {
                is_streamed: self.link.is_streamed(),
                batch_size: batch_size.min(self.link.get_mtu()),
                queue_size: self.transport.config.manager.config.queue_size,
                backoff: self.transport.config.manager.config.queue_backoff,
            };

            #[cfg(feature = "transport_compression")]
            let compression_enabled = self.transport.config.manager.config.compression_enabled;

            // The pipeline
            let (producer, consumer) = TransmissionPipeline::make(config, conduit_tx);
            self.pipeline = Some(producer);

            // Spawn the TX task
            let c_link = self.link.clone();
            let c_transport = self.transport.clone();
            let handle = executor.spawn(async move {
                let res = tx_task(
                    consumer,
                    c_link.clone(),
                    keep_alive,
                    #[cfg(feature = "stats")]
                    c_transport.stats.clone(),
                    #[cfg(feature = "transport_compression")]
                    compression_enabled,
                )
                .await;
                if let Err(e) = res {
                    log::debug!("{}", e);
                    // Spawn a task to avoid a deadlock waiting for this same task
                    // to finish in the close() joining its handle
                    task::spawn(async move { c_transport.del_link(&c_link).await });
                }
            });
            self.handle_tx = Some(Arc::new(handle));
        }
    }

    pub(super) fn stop_tx(&mut self) {
        if let Some(pl) = self.pipeline.as_ref() {
            pl.disable();
        }
    }

    pub(super) fn start_rx(&mut self, lease: Duration) {
        if self.handle_rx.is_none() {
            // Spawn the RX task
            let c_link = self.link.clone();
            let c_transport = self.transport.clone();
            let c_signal = self.signal_rx.clone();
            let c_rx_buffer_size = self.transport.config.manager.config.link_rx_buffer_size;

            // let compression_enabled = self.transport.config.manager.config.compression_enabled;

            let handle = task::spawn(async move {
                // Start the consume task
                let res = rx_task(
                    c_link.clone(),
                    c_transport.clone(),
                    lease,
                    c_signal.clone(),
                    c_rx_buffer_size,
                )
                .await;
                c_signal.trigger();
                if let Err(e) = res {
                    log::debug!("{}", e);
                    // Spawn a task to avoid a deadlock waiting for this same task
                    // to finish in the close() joining its handle
                    task::spawn(async move { c_transport.del_link(&c_link).await });
                }
            });
            self.handle_rx = Some(Arc::new(handle));
        }
    }

    pub(super) fn stop_rx(&mut self) {
        self.signal_rx.trigger();
    }

    pub(super) async fn close(mut self) -> ZResult<()> {
        log::trace!("{}: closing", self.link);
        self.stop_rx();
        if let Some(handle) = self.handle_rx.take() {
            // Safety: it is safe to unwrap the Arc since we have the ownership of the whole link
            let handle_rx = Arc::try_unwrap(handle).unwrap();
            handle_rx.await;
        }

        self.stop_tx();
        if let Some(handle) = self.handle_tx.take() {
            // Safety: it is safe to unwrap the Arc since we have the ownership of the whole link
            let handle_tx = Arc::try_unwrap(handle).unwrap();
            handle_tx.await;
        }

        self.link.close().await
    }
}

/*************************************/
/*              TASKS                */
/*************************************/
async fn tx_task(
    mut pipeline: TransmissionPipelineConsumer,
    link: LinkUnicast,
    keep_alive: Duration,
    #[cfg(feature = "stats")] stats: Arc<TransportUnicastStatsAtomic>,
    #[cfg(feature = "transport_compression")] compression_enabled: bool,
) -> ZResult<()> {
    #[cfg(feature = "transport_compression")]
    let mut compression_aux_buff: Box<[u8]> = vec![0; usize::pow(2, 16)].into_boxed_slice();

    loop {
        match pipeline.pull().timeout(keep_alive).await {
            Ok(res) => match res {
                Some((batch, priority)) => {
                    // Send the buffer on the link
                    #[allow(unused_mut)]
                    let mut bytes = batch.as_bytes();

                    #[cfg(feature = "transport_compression")]
                    let compression: Vec<u8>;

                    #[cfg(feature = "transport_compression")]
                    {
                        compression = tx_compressed(
                            compression_enabled,
                            &bytes,
                            &mut compression_aux_buff,
                            &link,
                        )?;
                        bytes = &compression;
                    }

                    link.write_all(bytes).await?;

                    #[cfg(feature = "stats")]
                    {
                        stats.inc_tx_t_msgs(batch.stats.t_msgs);
                        stats.inc_tx_bytes(bytes.len());
                    }

                    // Reinsert the batch into the queue
                    pipeline.refill(batch, priority);
                }
                None => break,
            },
            Err(_) => {
                let zid = None;
                let attachment = None;
                let message = TransportMessage::make_keep_alive(zid, attachment);

                #[allow(unused_variables)] // Used when stats feature is enabled
                let n = link.write_transport_message(&message).await?;
                #[cfg(feature = "stats")]
                {
                    stats.inc_tx_t_msgs(1);
                    stats.inc_tx_bytes(n);
                }
            }
        }
    }

    // Drain the transmission pipeline and write remaining bytes on the wire
    let mut batches = pipeline.drain();
    for (b, _) in batches.drain(..) {
        link.write_all(b.as_bytes())
            .timeout(keep_alive)
            .await
            .map_err(|_| zerror!("{}: flush failed after {} ms", link, keep_alive.as_millis()))??;

        #[cfg(feature = "stats")]
        {
            stats.inc_tx_t_msgs(b.stats.t_msgs);
            stats.inc_tx_bytes(b.len() as usize);
        }
    }

    Ok(())
}

async fn rx_task_stream(
    link: LinkUnicast,
    transport: TransportUnicastInner,
    lease: Duration,
    signal: Signal,
    rx_buffer_size: usize,
) -> ZResult<()> {
    enum Action {
        Read(usize),
        Stop,
    }

    async fn read(link: &LinkUnicast, buffer: &mut [u8]) -> ZResult<Action> {
        // 16 bits for reading the batch length
        let mut length = [0_u8, 0_u8];
        link.read_exact(&mut length).await?;
        let n = u16::from_le_bytes(length) as usize;
        link.read_exact(&mut buffer[0..n]).await?;
        Ok(Action::Read(n))
    }

    async fn stop(signal: Signal) -> ZResult<Action> {
        signal.wait().await;
        Ok(Action::Stop)
    }

    #[cfg(feature = "transport_compression")]
    let zenoh_compress = ZenohCompress::default();

    let codec = Zenoh060::default();

    // The pool of buffers
    let mtu = link.get_mtu() as usize;
    let mut n = rx_buffer_size / mtu;
    if rx_buffer_size % mtu != 0 {
        n += 1;
    }

    let pool = RecyclingObjectPool::new(n, || vec![0_u8; mtu].into_boxed_slice());
    while !signal.is_triggered() {
        // Retrieve one buffer
        let mut buffer = pool.try_take().unwrap_or_else(|| pool.alloc());
        // Async read from the underlying link
        let action = read(&link, &mut buffer)
            .race(stop(signal.clone()))
            .timeout(lease)
            .await
            .map_err(|_| zerror!("{}: expired after {} milliseconds", link, lease.as_millis()))??;
        match action {
            Action::Read(n) => {
                #[cfg(feature = "stats")]
                {
                    transport.stats.inc_rx_bytes(2 + n); // Account for the batch len encoding (16 bits)
                }

                #[allow(unused_mut)]
                let mut start_pos = 0;
                #[allow(unused_mut)]
                let mut end_pos = n;
                #[cfg(feature = "transport_compression")]
                {
                    let is_compressed: bool = buffer[0] == 1_u8;
                    if is_compressed {
                        let mut aux_buff = pool.try_take().unwrap_or_else(|| pool.alloc());
                        let compression = &buffer[1..n];
                        end_pos = rx_decompress(&zenoh_compress, compression, &mut aux_buff)?;
                        buffer = aux_buff;
                        start_pos += 2;
                    } else {
                        start_pos = 1;
                    }
                }

                // Deserialize all the messages from the current ZBuf
                let mut zslice = ZSlice::make(Arc::new(buffer), start_pos, end_pos).unwrap();
                let mut reader = zslice.reader();
                while reader.can_read() {
                    let msg: TransportMessage = codec
                        .read(&mut reader)
                        .map_err(|_| zerror!("{}: decoding error", link))?;

                    #[cfg(feature = "stats")]
                    {
                        transport.stats.inc_rx_t_msgs(1);
                    }

                    transport.receive_message(msg, &link)?
                }
            }
            Action::Stop => break,
        }
    }
    Ok(())
}

async fn rx_task_dgram(
    link: LinkUnicast,
    transport: TransportUnicastInner,
    lease: Duration,
    signal: Signal,
    rx_buffer_size: usize,
) -> ZResult<()> {
    enum Action {
        Read(usize),
        Stop,
    }

    async fn read(link: &LinkUnicast, buffer: &mut [u8]) -> ZResult<Action> {
        let n = link.read(buffer).await?;
        Ok(Action::Read(n))
    }

    async fn stop(signal: Signal) -> ZResult<Action> {
        signal.wait().await;
        Ok(Action::Stop)
    }

    let codec = Zenoh060::default();

    #[cfg(feature = "transport_compression")]
    let zenoh_compress = ZenohCompress::default();

    // The pool of buffers
    let mtu = link.get_mtu() as usize;
    let mut n = rx_buffer_size / mtu;
    if rx_buffer_size % mtu != 0 {
        n += 1;
    }
    let pool = RecyclingObjectPool::new(n, || vec![0_u8; mtu].into_boxed_slice());

    while !signal.is_triggered() {
        // Retrieve one buffer
        let mut buffer = pool.try_take().unwrap_or_else(|| pool.alloc());
        // Async read from the underlying link
        let action = read(&link, &mut buffer)
            .race(stop(signal.clone()))
            .timeout(lease)
            .await
            .map_err(|_| zerror!("{}: expired after {} milliseconds", link, lease.as_millis()))??;
        match action {
            Action::Read(n) => {
                if n == 0 {
                    // Reading 0 bytes means error
                    bail!("{}: zero bytes reading", link)
                }

                #[cfg(feature = "stats")]
                {
                    transport.stats.inc_rx_bytes(n);
                }

                #[allow(unused_mut)]
                let mut start_pos = 0;
                #[allow(unused_mut)]
                let mut end_pos = n;
                #[cfg(feature = "transport_compression")]
                {
                    let is_compressed: bool = buffer[0] == 1_u8;
                    if is_compressed {
                        let mut aux_buffer = pool.try_take().unwrap_or_else(|| pool.alloc());
                        let compression = &buffer[1..n];
                        end_pos = rx_decompress(&zenoh_compress, compression, &mut aux_buffer)?;
                        buffer = aux_buffer;
                    } else {
                        start_pos = 1;
                    }
                }

                // Deserialize all the messages from the current ZBuf
                let mut zslice = ZSlice::make(Arc::new(buffer), start_pos, end_pos).unwrap();
                let mut reader = zslice.reader();
                while reader.can_read() {
                    let msg: TransportMessage = codec
                        .read(&mut reader)
                        .map_err(|_| zerror!("{}: decoding error", link))?;

                    #[cfg(feature = "stats")]
                    {
                        transport.stats.inc_rx_t_msgs(1);
                    }

                    transport.receive_message(msg, &link)?
                }
            }
            Action::Stop => break,
        }
    }
    Ok(())
}

async fn rx_task(
    link: LinkUnicast,
    transport: TransportUnicastInner,
    lease: Duration,
    signal: Signal,
    rx_buffer_size: usize,
) -> ZResult<()> {
    if link.is_streamed() {
        rx_task_stream(link, transport, lease, signal, rx_buffer_size).await
    } else {
        rx_task_dgram(link, transport, lease, signal, rx_buffer_size).await
    }
}

#[cfg(feature = "transport_compression")]
fn tx_compressed(
    compression_enabled: bool,
    bytes: &[u8],
    compression_aux_buff: &mut Box<[u8]>,
    link: &LinkUnicast,
) -> ZResult<Vec<u8>> {
    let mut buff = vec![];
    if compression_enabled {
        let zenoh_compress = ZenohCompress::default();
        let compression_size = zenoh_compress
            .write(&mut buff.writer(), (&bytes, compression_aux_buff))
            .map_err(|e| zerror!("Compression error: {:?}", e))?;
        let mut compressed_batch: Vec<u8> = vec![];
        if link.is_streamed() {
            let compression_size_u16: u16 = compression_size.try_into().map_err(|e| {
                zerror!(
                    "Compression error: unable to convert compression size into u16: {}",
                    e
                )
            })?;
            // including is compressed byte
            let batch_size_le = (compression_size_u16 + 1).to_le_bytes();
            compressed_batch.append(&mut batch_size_le.to_vec());
            compressed_batch.push(true as u8);
            compressed_batch.append(&mut buff);
        } else {
            compressed_batch.push(true as u8);
            compressed_batch.append(&mut buff);
        }
        buff = compressed_batch;
    } else {
        // Add compression byte as false
        // Increment the batch size number from the 2 initial bytes
        buff = bytes.to_vec();
        buff.insert(2, false as u8);
        let (batch_size, batch_payload) = buff.split_at_mut(2);
        let batch_size = u16::from_le_bytes(batch_size.try_into().unwrap()) + 1;
        buff = [batch_size.to_le_bytes().to_vec(), batch_payload.to_vec()].concat();
    }
    Ok(buff)
}

#[cfg(feature = "transport_compression")]
fn rx_decompress(
    zenoh_compress: &ZenohCompress,
    compression: &[u8],
    aux_buffer: &mut [u8],
) -> ZResult<usize> {
    Ok(zenoh_compress
        .read((&compression, aux_buffer))
        .map_err(|e| {
            zerror!(
                "Decompression error {:?}. Unable to decompress batch. {:?}",
                e,
                compression.to_vec()
            )
        })?)
}
