use std::io::Read;

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
use crate::{Compress, Decompress, RCodec, WCodec, Zenoh060, ZenohCompress};
use zenoh_buffers::{
    reader::{DidntRead, HasReader},
    writer::{DidntWrite, HasWriter},
    BBuf,
};

impl Compress<BBuf> for ZenohCompress {
    type Error = DidntWrite;

    fn compress(&self, buffer: &BBuf) -> Result<BBuf, Self::Error> {
        let reader = buffer.reader();
        let compression = lz4_flex::block::compress_prepend_size(reader);

        println!("Compression len: {}", compression.len());
        println!("{:?}", compression);

        let codec = Zenoh060::default();
        let mut bbuf = BBuf::with_capacity(u16::MAX as usize);
        let mut writer = bbuf.writer();
        codec.write(&mut writer, compression.as_slice()).unwrap();
        Ok(bbuf)
    }
}

impl Decompress<BBuf> for ZenohCompress {
    type Error = DidntRead;

    fn decompress(&self, buffer: &BBuf) -> Result<BBuf, Self::Error> {
        let mut reader = buffer.reader();
        let codec = Zenoh060::default();
        let decoded_buffer: Vec<u8> = codec.read(&mut reader).unwrap();
        let mut decompression =
            lz4_flex::block::decompress_size_prepended(&decoded_buffer).unwrap();
        decompression.drain(0..2); // remove prepended size
        let mut bbuf = BBuf::with_capacity(u16::MAX as usize);
        let mut writer = bbuf.writer();
        codec.write(&mut writer, decompression.as_slice()).unwrap();
        Ok(bbuf)
    }
}
