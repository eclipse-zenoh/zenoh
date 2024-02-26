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
use crate::{LCodec, RCodec, WCodec, Zenoh080, Zenoh080Bounded};
use alloc::string::String;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::imsg,
    core::encoding::{flag, Encoding, EncodingPrefix},
};

impl LCodec<&Encoding> for Zenoh080 {
    fn w_len(self, x: &Encoding) -> usize {
        let (prefix, suffix) = (x.prefix(), x.suffix());
        let mut len = self.w_len((prefix as u32) << 1);
        if !suffix.is_empty() {
            len += self.w_len(x.suffix());
        }
        len
    }
}

impl<W> WCodec<&Encoding, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Encoding) -> Self::Output {
        let mut prefix = (x.prefix() as u32) << 1;
        let suffix = x.suffix();

        if !suffix.is_empty() {
            prefix |= flag::S;
        }
        let zodec = Zenoh080Bounded::<u32>::new();
        zodec.write(&mut *writer, prefix)?;
        if !suffix.is_empty() {
            let zodec = Zenoh080Bounded::<u8>::new();
            zodec.write(&mut *writer, suffix)?;
        }
        Ok(())
    }
}

impl<R> RCodec<Encoding, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Encoding, Self::Error> {
        let zodec = Zenoh080Bounded::<u32>::new();
        let prefix: u32 = zodec.read(&mut *reader)?;
        let (prefix, has_suffix) = (
            (prefix >> 1) as EncodingPrefix,
            imsg::has_flag(prefix as u8, flag::S as u8),
        );

        let mut suffix = String::new();
        if has_suffix {
            let zodec = Zenoh080Bounded::<u8>::new();
            suffix = zodec.read(&mut *reader)?;
        }

        let mut encoding: Encoding = Encoding::new(prefix);
        if !suffix.is_empty() {
            encoding = encoding.with_suffix(suffix).map_err(|_| DidntRead)?;
        }

        Ok(encoding)
    }
}
