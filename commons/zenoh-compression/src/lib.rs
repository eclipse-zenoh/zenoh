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
pub struct ZenohCompress;

/// [WCodec] Implementation for ZenohCompress.
///
/// Allows to compress a slice using lz4_flex. Can be used in conjunction with
/// ZenohCompress::read(&mut [u8]).
impl<W> WCodec<(&[u8], &mut [u8]), &mut W> for ZenohCompress
where
    W: Writer,
{
    type Output = Result<usize, DidntWrite>;

    fn write(self, output_writer: &mut W, input: (&[u8], &mut [u8])) -> Self::Output {
        let (slice, buffer) = input;
        let compressed_bytes = lz4_flex::block::compress_into(slice, buffer).map_err(|e| {
            log::debug!("Compression error: {e}");
            DidntWrite
        })?;
        output_writer.write(&buffer[0..compressed_bytes])?;
        Ok(compressed_bytes)
    }
}

/// Implementation of [RCodec] for [ZenohCompress]. Allows to decompress the message previously
/// compressed with [ZenohCompress]'s write function.
///
impl RCodec<(), (&[u8], &mut [u8])> for ZenohCompress {
    type Error = DidntRead;

    /// Decompresses the payload from the [Reader] passed as a parameter, returning a result with
    /// the uncompressed buffer or a [DidntRead] error.
    fn read(self, input: (&[u8], &mut [u8])) -> Result<(), DidntRead> {
        let (compression, buffer) = input;
        lz4_flex::block::decompress_into(compression, buffer).map_err(|e| {
            log::debug!("Decompression error: {}", e);
            DidntRead
        })?;
        Ok(())
    }
}
