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
use super::transport::TransportUnicastUniversal;
#[cfg(feature = "stats")]
use {crate::common::stats::TransportStats, std::sync::Arc};
use crate::{
    common::{
        batch::{BatchConfig, RBatch},
        pipeline::{
            TransmissionPipeline, TransmissionPipelineConf, TransmissionPipelineConsumer,
            TransmissionPipelineProducer,
        },
        priority::TransportPriorityTx,
    },
    unicast::link::{TransportLinkUnicast, TransportLinkUnicastRx, TransportLinkUnicastTx},
};
use std::time::Duration;
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use zenoh_buffers::ZSliceBuffer;
use zenoh_protocol::transport::{KeepAlive, TransportMessage};
use zenoh_result::{zerror, ZResult};
use zenoh_sync::{RecyclingObject, RecyclingObjectPool};

#[derive(Clone)]
pub(super) struct TransportLinkUnicastUniversal {
    // The underlying link
    pub(super) link: TransportLinkUnicast,
    // The transmission pipeline
    pub(super) pipeline: TransmissionPipelineProducer,
    // The task handling substruct
    tracker: TaskTracker,
    token: CancellationToken,
}

impl TransportLinkUnicastUniversal {
    pub(super) fn new(
        transport: &TransportUnicastUniversal,
        link: TransportLinkUnicast,
        priority_tx: &[TransportPriorityTx],
    ) -> (Self, TransmissionPipelineConsumer) {
        assert!(!priority_tx.is_empty());

        let config = TransmissionPipelineConf {
            batch: BatchConfig {
                mtu: link.config.batch.mtu,
                is_streamed: link.link.is_streamed(),
                #[cfg(feature = "transport_compression")]
                is_compression: link.config.batch.is_compression,
            },
            queue_size: transport.manager.config.queue_size,
            backoff: transport.manager.config.queue_backoff,
        };

        // The pipeline
        let (producer, consumer) = TransmissionPipeline::make(config, priority_tx);

        let result = Self {
            link,
            pipeline: producer,
            tracker: TaskTracker::new(),
            token: CancellationToken::new(),
        };

        (result, consumer)
    }

    // TODO: Not yet guaranteed is called at most once
    pub(super) fn start_tx(
        &mut self,
        transport: TransportUnicastUniversal,
        consumer: TransmissionPipelineConsumer,
        keep_alive: Duration,
    ) {
        // Spawn the TX task
        let mut tx = self.link.tx();
        let token = self.token.child_token();
        let task = async move {
            let res = tx_task(
                consumer,
                &mut tx,
                keep_alive,
                token,
                #[cfg(feature = "stats")]
                transport.stats.clone(),
            )
            .await;

            // TODO: improve this callback
            if let Err(e) = res {
                log::debug!("{}", e);
                // Spawn a task to avoid a deadlock waiting for this same task
                // to finish in the close() joining its handle
                zenoh_runtime::ZRuntime::TX
                    .spawn(async move { transport.del_link(tx.inner.link()).await });
            }
        };
        self.tracker.spawn_on(task, &zenoh_runtime::ZRuntime::TX);
    }

    // TODO: Not yet guaranteed is called at most once
    pub(super) fn start_rx(&mut self, transport: TransportUnicastUniversal, lease: Duration) {
        let mut rx = self.link.rx();
        let token = self.token.child_token();
        let task = async move {
            // Start the consume task
            let res = rx_task(
                &mut rx,
                transport.clone(),
                lease,
                transport.manager.config.link_rx_buffer_size,
                token,
            )
            .await;

            // TODO: improve this callback
            if let Err(e) = res {
                log::debug!("{}", e);
                // Spawn a task to avoid a deadlock waiting for this same task
                // to finish in the close() joining its handle
                zenoh_runtime::ZRuntime::RX
                    .spawn(async move { transport.del_link((&rx.link).into()).await });
            }
        };
        self.tracker.spawn_on(task, &zenoh_runtime::ZRuntime::RX);
    }

    pub(super) async fn close(self) -> ZResult<()> {
        log::trace!("{}: closing", self.link);

        self.tracker.close();
        self.token.cancel();
        self.pipeline.disable();
        self.tracker.wait().await;

        self.link.close(None).await
    }
}

/*************************************/
/*              TASKS                */
/*************************************/
async fn tx_task(
    mut pipeline: TransmissionPipelineConsumer,
    link: &mut TransportLinkUnicastTx,
    keep_alive: Duration,
    token: CancellationToken,
    #[cfg(feature = "stats")] stats: Arc<TransportStats>,
) -> ZResult<()> {
    let mut interval = tokio::time::interval(keep_alive);
    loop {
        tokio::select! {
            res = pipeline.pull() => {
                if let Some((mut batch, priority)) = res {
                    link.send_batch(&mut batch).await?;

                    #[cfg(feature = "stats")]
                    {
                        stats.inc_tx_t_msgs(batch.stats.t_msgs);
                        stats.inc_tx_bytes(batch.len() as usize);
                    }

                    // Reinsert the batch into the queue
                    pipeline.refill(batch, priority);
                } else {
                    break
                }
            }

            _ = interval.tick() => {
                let message: TransportMessage = KeepAlive.into();

                #[allow(unused_variables)] // Used when stats feature is enabled
                let n = link.send(&message).await?;

                #[cfg(feature = "stats")]
                {
                    stats.inc_tx_t_msgs(1);
                    stats.inc_tx_bytes(n);
                }
            }

            _ = token.cancelled() => { break }
        }
    }

    // Drain the transmission pipeline and write remaining bytes on the wire
    let mut batches = pipeline.drain();
    for (mut b, _) in batches.drain(..) {
        tokio::time::timeout(keep_alive, link.send_batch(&mut b))
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

async fn rx_task(
    link: &mut TransportLinkUnicastRx,
    transport: TransportUnicastUniversal,
    lease: Duration,
    rx_buffer_size: usize,
    token: CancellationToken,
) -> ZResult<()> {
    async fn read<T, F>(
        link: &mut TransportLinkUnicastRx,
        pool: &RecyclingObjectPool<T, F>,
    ) -> ZResult<RBatch>
    where
        T: ZSliceBuffer + 'static,
        F: Fn() -> T,
        RecyclingObject<T>: ZSliceBuffer,
    {
        let batch = link
            .recv_batch(|| pool.try_take().unwrap_or_else(|| pool.alloc()))
            .await?;
        Ok(batch)
    }

    // The pool of buffers
    let mtu = link.batch.max_buffer_size();
    let mut n = rx_buffer_size / mtu;
    if rx_buffer_size % mtu != 0 {
        n += 1;
    }

    let pool = RecyclingObjectPool::new(n, || vec![0_u8; mtu].into_boxed_slice());
    let l = (&link.link).into();

    loop {
        tokio::select! {
            batch = tokio::time::timeout(lease, read(link, &pool)) => {
                let batch = batch.map_err(|_| zerror!("{}: expired after {} milliseconds", link, lease.as_millis()))??;
                #[cfg(feature = "stats")]
                {
                    transport.stats.inc_rx_bytes(2 + n); // Account for the batch len encoding (16 bits)
                }
                transport.read_messages(batch, &l)?;
            }

            _ = token.cancelled() => { break }
        }
    }

    Ok(())
}

async fn rx_task_dgram(
    link: LinkUnicast,
    transport: TransportUnicastUniversal,
    lease: Duration,
    signal: Signal,
    rx_batch_size: BatchSize,
    rx_buffer_size: usize,
) -> ZResult<()> {
    async fn read(link: &LinkUnicast, buffer: &mut [u8]) -> ZResult<usize> {
        let n = link.read(buffer).await?;
        Ok(n)
    }

    // The pool of buffers
    let mtu = link.get_mtu().min(rx_batch_size) as usize;
    let mut n = rx_buffer_size / mtu;
    if rx_buffer_size % mtu != 0 {
        n += 1;
    }
    let pool = RecyclingObjectPool::new(n, || vec![0_u8; mtu].into_boxed_slice());

    loop {
        // Retrieve one buffer
        let mut buffer = pool.try_take().unwrap_or_else(|| pool.alloc());

        // Async read from the underlying link
        tokio::select! {
            _ = signal.wait() => { break },
            res = timeout(lease, read(&link, &mut buffer)) => {
                let n = res.map_err(|_| zerror!("{}: expired after {} milliseconds", link, lease.as_millis()))??;
                if n == 0 {
                    // Reading 0 bytes means error
                    bail!("{}: zero bytes reading", link)
                }

                #[cfg(feature = "stats")]
                {
                    transport.stats.inc_rx_bytes(n);
                }

                #[allow(unused_mut)]
                let mut end_pos = n;

                #[allow(unused_mut)]
                let mut start_pos = 0;

                #[cfg(all(feature = "unstable", feature = "transport_compression"))]
                rx_decompress(&mut buffer, &pool, n, &mut start_pos, &mut end_pos)?;

                // Deserialize all the messages from the current ZBuf
                let zslice = ZSlice::make(Arc::new(buffer), start_pos, end_pos)
                    .map_err(|_| zerror!("Read {} bytes but buffer is {} bytes", n, mtu))?;
                transport.read_messages(zslice, &link)?;
            }
        }
    }
    Ok(())
}

async fn rx_task(
    link: LinkUnicast,
    transport: TransportUnicastUniversal,
    lease: Duration,
    signal: Signal,
    rx_batch_size: u16,
    rx_buffer_size: usize,
) -> ZResult<()> {
    if link.is_streamed() {
        rx_task_stream(
            link,
            transport,
            lease,
            signal,
            rx_batch_size,
            rx_buffer_size,
        )
        .await
    } else {
        rx_task_dgram(
            link,
            transport,
            lease,
            signal,
            rx_batch_size,
            rx_buffer_size,
        )
        .await
    }
}

#[cfg(all(feature = "unstable", feature = "transport_compression"))]
/// Decompresses the received contents contained in the buffer.
fn rx_decompress(
    buffer: &mut zenoh_sync::RecyclingObject<Box<[u8]>>,
    pool: &RecyclingObjectPool<Box<[u8]>, impl Fn() -> Box<[u8]>>,
    read_bytes: usize,
    start_pos: &mut usize,
    end_pos: &mut usize,
) -> ZResult<()> {
    let is_compressed: bool = buffer[COMPRESSION_BYTE_INDEX] == COMPRESSION_ENABLED;
    if is_compressed {
        let mut aux_buff = pool.try_take().unwrap_or_else(|| pool.alloc());
        *end_pos = lz4_flex::block::decompress_into(
            &buffer[BATCH_PAYLOAD_START_INDEX..read_bytes],
            &mut aux_buff,
        )
        .map_err(|e| zerror!("Decompression error: {:}", e))?;
        *buffer = aux_buff;
    } else {
        *start_pos = BATCH_PAYLOAD_START_INDEX;
        *end_pos = read_bytes;
    }
    Ok(())
}

#[cfg(all(feature = "unstable", feature = "transport_compression"))]
/// Compresses the batch into the output buffer.
///
/// If the batch is streamed, the output contains a header of two bytes representing the size of
/// the resulting batch, otherwise it is not included. In any case, an extra byte is added (before
/// the payload and considered in the header value) representing if the batch is compressed or not.
/// If the resulting size of the compression no smaller than the original batch size, then
/// we send the original one.
///
/// Returns a tuple containing the size of the resulting batch, along with a boolean representing
/// if the batch was indeed compressed or not.
fn tx_compressed(
    is_compressed: bool,
    is_streamed: bool,
    batch: &[u8],
    output: &mut [u8],
) -> ZResult<(/*batch_size=*/ usize, /*was_compressed=*/ bool)> {
    if is_compressed {
        let s_pos = if is_streamed { 3 } else { 1 };
        let payload = &batch[s_pos - 1..];
        let payload_size = payload.len();
        let compression_size = lz4_flex::block::compress_into(payload, &mut output[s_pos..])
            .map_err(|e| zerror!("Compression error: {:}", e))?;
        if compression_size >= payload_size {
            log::debug!(
                "Compression discarded due to the original batch size being smaller than the compressed batch."
            );
            return Ok((
                set_uncompressed_batch_header(batch, output, is_streamed)?,
                false,
            ));
        }
        Ok((
            set_compressed_batch_header(output, compression_size, is_streamed)?,
            true,
        ))
    } else {
        Ok((
            set_uncompressed_batch_header(batch, output, is_streamed)?,
            false,
        ))
    }
}

#[cfg(all(feature = "unstable", feature = "transport_compression"))]
/// Inserts the compresion byte for batches WITH compression.
/// The buffer is expected to contain the compression starting from byte 3 (if streamed) or 1
/// (if not streamed).
///
/// Arguments:
/// - buff: the buffer with the compression, with 3 or 1 bytes reserved at the beginning in case of
///     being streamed or not respectively.
/// - compression_size: the size of the compression
/// - is_streamed: if the batch is intended to be streamed or not
///
/// Returns: the size of the compressed batch considering the header.
fn set_compressed_batch_header(
    buff: &mut [u8],
    compression_size: usize,
    is_streamed: bool,
) -> ZResult<usize> {
    let final_batch_size: usize;
    let payload_size = 1 + compression_size;
    if is_streamed {
        let payload_size_u16: u16 = payload_size.try_into().map_err(|e| {
            zerror!(
                "Compression error: unable to convert batch size into u16: {}",
                e
            )
        })?;
        buff[0..HEADER_BYTES_SIZE].copy_from_slice(&payload_size_u16.to_le_bytes());
        buff[COMPRESSION_BYTE_INDEX_STREAMED] = COMPRESSION_ENABLED;
        final_batch_size = payload_size + HEADER_BYTES_SIZE;
    } else {
        buff[COMPRESSION_BYTE_INDEX] = COMPRESSION_ENABLED;
        final_batch_size = payload_size;
    }
    if final_batch_size > MAX_BATCH_SIZE {
        // May happen when the payload size is itself the MTU and adding the header exceeds it.
        Err(zerror!("Failed to send uncompressed batch, batch size ({}) exceeds the maximum batch size of {}.", final_batch_size, MAX_BATCH_SIZE))?
    }
    Ok(final_batch_size)
}

#[cfg(all(feature = "unstable", feature = "transport_compression"))]
/// Inserts the compression byte for batches without compression, that is inserting a 0 byte on the
/// third position of the buffer and increasing the batch size from the header by 1.
///
/// Arguments:
/// - bytes: the source slice
/// - buff: the output slice
/// - is_streamed: if the batch is meant to be streamed or not, thus considering or not the 2 bytes
///     header specifying the size of the batch.
///
/// Returns: the size of the batch considering the header.
fn set_uncompressed_batch_header(
    bytes: &[u8],
    buff: &mut [u8],
    is_streamed: bool,
) -> ZResult<usize> {
    let final_batch_size: usize;
    if is_streamed {
        let mut header = [0_u8, 0_u8];
        header[..HEADER_BYTES_SIZE].copy_from_slice(&bytes[..HEADER_BYTES_SIZE]);
        let batch_size = if let Some(size) = u16::from_le_bytes(header).checked_add(1) {
            size
        } else {
            bail!("Compression error: unable to convert compression size into u16",)
        };
        buff[0..HEADER_BYTES_SIZE].copy_from_slice(&batch_size.to_le_bytes());
        buff[COMPRESSION_BYTE_INDEX_STREAMED] = COMPRESSION_DISABLED;
        let batch_size: usize = batch_size.into();
        buff[3..batch_size + 2].copy_from_slice(&bytes[2..batch_size + 1]);
        final_batch_size = batch_size + 2;
    } else {
        buff[COMPRESSION_BYTE_INDEX] = COMPRESSION_DISABLED;
        let len = 1 + bytes.len();
        buff[1..1 + bytes.len()].copy_from_slice(bytes);
        final_batch_size = len;
    }
    if final_batch_size > MAX_BATCH_SIZE {
        // May happen when the payload size is itself the MTU and adding the header exceeds it.
        Err(zerror!("Failed to send uncompressed batch, batch size ({}) exceeds the maximum batch size of {}.", final_batch_size, MAX_BATCH_SIZE))?;
    }
    Ok(final_batch_size)
}

#[cfg(all(feature = "transport_compression", feature = "unstable"))]
#[test]
fn tx_compression_test() {
    const COMPRESSION_BYTE: usize = 1;
    let payload = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4];
    let mut buff: Box<[u8]> =
        vec![0; lz4_flex::block::get_maximum_output_size(MAX_BATCH_SIZE) + 3].into_boxed_slice();

    // Compression done for the sake of comparing the result.
    let payload_compression_size = lz4_flex::block::compress_into(&payload, &mut buff).unwrap();

    fn get_header_value(buff: &[u8]) -> u16 {
        let mut header = [0_u8, 0_u8];
        header[..HEADER_BYTES_SIZE].copy_from_slice(&buff[..HEADER_BYTES_SIZE]);
        u16::from_le_bytes(header)
    }

    // Streamed with compression enabled
    let batch = [16, 0, 1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4];
    let (batch_size, was_compressed) = tx_compressed(true, true, &batch, &mut buff).unwrap();
    let header = get_header_value(&buff);
    assert!(was_compressed);
    assert_eq!(header as usize, payload_compression_size + COMPRESSION_BYTE);
    assert!(batch_size < batch.len() + COMPRESSION_BYTE);
    assert_eq!(batch_size, payload_compression_size + 3);

    // Not streamed with compression enabled
    let batch = payload;
    let (batch_size, was_compressed) = tx_compressed(true, false, &batch, &mut buff).unwrap();
    assert!(was_compressed);
    assert!(batch_size < batch.len() + COMPRESSION_BYTE);
    assert_eq!(batch_size, payload_compression_size + COMPRESSION_BYTE);

    // Streamed with compression disabled
    let batch = [16, 0, 1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4];
    let (batch_size, was_compressed) = tx_compressed(false, true, &batch, &mut buff).unwrap();
    let header = get_header_value(&buff);
    assert!(!was_compressed);
    assert_eq!(header as usize, payload.len() + COMPRESSION_BYTE);
    assert_eq!(batch_size, batch.len() + COMPRESSION_BYTE);

    // Not streamed and compression disabled
    let batch = payload;
    let (batch_size, was_compressed) = tx_compressed(false, false, &batch, &mut buff).unwrap();
    assert!(!was_compressed);
    assert_eq!(batch_size, payload.len() + COMPRESSION_BYTE);

    // Verify that if the compression result is bigger than the original payload size, then the non compressed payload is returned.
    let batch = [16, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]; // a non compressable payload with no repetitions
    let (batch_size, was_compressed) = tx_compressed(true, true, &batch, &mut buff).unwrap();
    assert!(!was_compressed);
    assert_eq!(batch_size, batch.len() + COMPRESSION_BYTE);
}

#[cfg(all(feature = "transport_compression", feature = "unstable"))]
#[test]
fn rx_compression_test() {
    let pool = RecyclingObjectPool::new(2, || vec![0_u8; MAX_BATCH_SIZE].into_boxed_slice());
    let mut buffer = pool.try_take().unwrap_or_else(|| pool.alloc());

    // Compressed batch
    let payload: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
    let compression_size = lz4_flex::block::compress_into(&payload, &mut buffer[1..]).unwrap();
    buffer[0] = 1; // is compressed byte

    let mut start_pos: usize = 0;
    let mut end_pos: usize = 0;

    rx_decompress(
        &mut buffer,
        &pool,
        compression_size + 1,
        &mut start_pos,
        &mut end_pos,
    )
    .unwrap();

    assert_eq!(start_pos, 0);
    assert_eq!(end_pos, payload.len());
    assert_eq!(buffer[start_pos..end_pos], payload);

    // Non compressed batch
    let mut start_pos: usize = 0;
    let mut end_pos: usize = 0;

    buffer[0] = 0;
    buffer[1..payload.len() + 1].copy_from_slice(&payload[..]);
    rx_decompress(
        &mut buffer,
        &pool,
        payload.len() + 1,
        &mut start_pos,
        &mut end_pos,
    )
    .unwrap();

    assert_eq!(start_pos, 1);
    assert_eq!(end_pos, payload.len() + 1);
    assert_eq!(buffer[start_pos..end_pos], payload);
}
