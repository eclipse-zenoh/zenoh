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

use zenoh_buffers::{
    reader::DidntRead,
    writer::{DidntWrite, Writer},
};
use zenoh_codec::{RCodec, WCodec};

#[derive(Clone, Copy, Default)]
/// Compression codec using [Lz4_flex](https://docs.rs/lz4_flex/latest/lz4_flex/).
pub struct ZenohCompress;

/// [WCodec] Implementation for ZenohCompress.
impl<W> WCodec<(&[u8], &mut [u8]), &mut W> for ZenohCompress
where
    W: Writer,
{
    type Output = Result<usize, DidntWrite>;

    /// Compresses the input and writes it down into the writer.
    ///
    /// Arguments:
    /// * output_writer: the [Writer] into which the compression is going to be written.
    /// * args: tupple consisting of
    ///   - a slice containing the actual input to be compressed
    ///   - an auxiliary slice to be used by the compression algorithm for storing the compressed
    ///     output until it's written into the writer. Note: the reason for this buffer to be
    ///     passed as mutable reference instead of being constructed inside the function is to
    ///     allow the recycling of the buffer, as this function is intended to be used within a
    ///     loop.
    /// Returns:
    /// * the number of bytes of the compressed output or an [DidntWrite] error if the compression
    ///   failed.
    ///
    fn write(
        self,
        output_writer: &mut W,
        args: (/*input=*/ &[u8], /*aux_buffer=*/ &mut [u8]),
    ) -> Self::Output {
        let (slice, buffer) = args;
        let compressed_bytes = lz4_flex::block::compress_into(slice, buffer).map_err(|e| {
            log::error!("Compression error: {:}", e);
            DidntWrite
        })?;
        output_writer.write(&buffer[0..compressed_bytes])?;
        Ok(compressed_bytes)
    }
}

/// Implementation of [RCodec] for [ZenohCompress].
impl RCodec<usize, (&[u8], &mut [u8])> for ZenohCompress {
    type Error = DidntRead;

    /// Reads a compressed input and writes it down into the specified buffer.
    ///
    /// Arguments:
    /// * args: tupple consisting of
    ///   - a slice of memory containing the compressed input to be decompressed
    ///   - a slice of memory where the decompression is going to be written. Note that similarly
    ///     to the Writer implementation, passing the buffer as a mutable reference allows the
    ///     recycling of the buffer, as this function is intended to be used within a loop.
    ///
    /// Returns:
    /// * the number of bytes of the decompressed output or a [DidntRead] error in case the
    ///   decompression failed.
    ///
    fn read(
        self,
        args: (/*input=*/ &[u8], /*output_buffer=*/ &mut [u8]),
    ) -> Result<usize, DidntRead> {
        let (compression, buffer) = args;
        lz4_flex::block::decompress_into(compression, buffer).map_err(|e| {
            log::error!("Decompression error: {:}", e);
            DidntRead
        })
    }
}
