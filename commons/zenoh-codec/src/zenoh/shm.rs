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
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
    ZBuf,
};

#[cfg(feature = "shared-memory")]
use crate::Zenoh080Sliced;
use crate::{LCodec, RCodec, WCodec, Zenoh080Bounded};

pub trait ZBufWCodec {
    fn write_zbuf<W: Writer>(writer: &mut W, zbuf: &ZBuf) -> Result<(), DidntWrite>;
    fn write_zbuf_nolen<W: Writer>(writer: &mut W, zbuf: &ZBuf) -> Result<(), DidntWrite>;
    fn calc_wlen(zbuf: &ZBuf, len: &mut usize);
}

#[cfg(feature = "shared-memory")]
pub(crate) struct ZBufShmWriter {}

#[cfg(feature = "shared-memory")]
impl ZBufWCodec for ZBufShmWriter {
    fn write_zbuf<W: Writer>(writer: &mut W, zbuf: &ZBuf) -> Result<(), DidntWrite> {
        let codec = Zenoh080Sliced::<u32>::new(true);
        codec.write(writer, zbuf)
    }

    fn write_zbuf_nolen<W: Writer>(writer: &mut W, zbuf: &ZBuf) -> Result<(), DidntWrite> {
        let codec = Zenoh080Sliced::<u32>::new(true);
        codec.write(writer, zbuf)
    }

    fn calc_wlen(zbuf: &ZBuf, len: &mut usize) {
        let codec = Zenoh080Sliced::<u32>::new(true);
        *len += codec.w_len(zbuf);
    }
}

pub(crate) struct ZBufRawWriter {}

impl ZBufWCodec for ZBufRawWriter {
    fn write_zbuf<W: Writer>(writer: &mut W, zbuf: &ZBuf) -> Result<(), DidntWrite> {
        let codec = Zenoh080Bounded::<u32>::new();
        codec.write(writer, zbuf)
    }

    fn write_zbuf_nolen<W: Writer>(writer: &mut W, zbuf: &ZBuf) -> Result<(), DidntWrite> {
        // Write payload
        fn write<W>(writer: &mut W, payload: &ZBuf) -> Result<(), DidntWrite>
        where
            W: Writer,
        {
            // Don't write the length since it is already included in the header
            for s in payload.zslices() {
                writer.write_zslice(s)?;
            }
            Ok(())
        }

        write(writer, zbuf)
    }

    fn calc_wlen(zbuf: &ZBuf, len: &mut usize) {
        let codec = Zenoh080Bounded::<u32>::new();
        *len += codec.w_len(zbuf);
    }
}

pub trait ZBufRCodec: Default {
    fn read_zbuf<R: Reader>(reader: &mut R) -> Result<ZBuf, DidntRead>;
    fn read_zbuf_nolen<R: Reader>(reader: &mut R, len: usize) -> Result<ZBuf, DidntRead>;
}

#[derive(Default)]
#[cfg(feature = "shared-memory")]
pub(crate) struct ZBufShmReader {}

#[cfg(feature = "shared-memory")]
impl ZBufRCodec for ZBufShmReader {
    fn read_zbuf<R: Reader>(reader: &mut R) -> Result<ZBuf, DidntRead> {
        let codec = Zenoh080Sliced::<u32>::new(true);
        codec.read(reader)
    }

    fn read_zbuf_nolen<R: Reader>(reader: &mut R, _len: usize) -> Result<ZBuf, DidntRead> {
        let codec = Zenoh080Sliced::<u32>::new(true);
        codec.read(reader)
    }
}

#[derive(Default)]
pub(crate) struct ZBufRawReader {}

impl ZBufRCodec for ZBufRawReader {
    fn read_zbuf<R: Reader>(reader: &mut R) -> Result<ZBuf, DidntRead> {
        let codec = Zenoh080Bounded::<u32>::new();
        codec.read(reader)
    }

    fn read_zbuf_nolen<R: Reader>(reader: &mut R, len: usize) -> Result<ZBuf, DidntRead> {
        // Read payload
        fn read<R>(reader: &mut R, len: usize) -> Result<ZBuf, DidntRead>
        where
            R: Reader,
        {
            let mut payload = ZBuf::empty();
            reader.read_zslices(len, |s| payload.push_zslice(s))?;
            Ok(payload)
        }

        read(reader, len)
    }
}
