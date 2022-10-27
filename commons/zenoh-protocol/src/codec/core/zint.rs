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
use crate::codec::Zenoh060;
use crate::codec::{RCodec, WCodec};
use std::convert::TryInto;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol_core::ZInt;

const VLE_LEN: usize = 10;

// ZInt
impl<W> WCodec<&mut W, ZInt> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, mut x: ZInt) -> Self::Output {
        writer.with_slot(VLE_LEN, move |buffer| {
            let mut len = 0;
            let mut b = x as u8;
            while x > 0x7f {
                buffer[len] = b | 0x80;
                len += 1;
                x >>= 7;
                b = x as u8;
            }
            buffer[len] = b;
            len + 1
        })
    }
}

impl<R> RCodec<&mut R, ZInt> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ZInt, Self::Error> {
        let mut b = reader.read_u8()?;

        let mut v = 0;
        let mut i = 0;
        let mut k = VLE_LEN;
        while b > 0x7f && k > 0 {
            v |= ((b & 0x7f) as ZInt) << i;
            i += 7;
            b = reader.read_u8()?;
            k -= 1;
        }
        if k > 0 {
            v |= ((b & 0x7f) as ZInt) << i;
            Ok(v)
        } else {
            Err(DidntRead)
        }
    }
}

// usize
impl<W> WCodec<&mut W, usize> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: usize) -> Self::Output {
        let x: ZInt = x.try_into().map_err(|_| DidntWrite)?;
        self.write(writer, x)
    }
}

impl<R> RCodec<&mut R, usize> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<usize, Self::Error> {
        let x: ZInt = <Self as RCodec<&mut R, ZInt>>::read(self, reader)?;
        x.try_into().map_err(|_| DidntRead)
    }
}

#[cfg(test)]
mod test {
    use crate::codec::core::zint::VLE_LEN;
    use crate::codec::*;
    use rand::Rng;
    use zenoh_buffers::reader::{HasReader, Reader};
    use zenoh_buffers::writer::HasWriter;
    use zenoh_buffers::ZBuf;
    use zenoh_protocol_core::ZInt;

    #[test]
    fn codec_zint() {
        let codec = Zenoh060::default();
        let mut rng = rand::thread_rng();

        macro_rules! run {
            ($buf:expr) => {
                for _ in 0..TEST_ITER {
                    $buf.clear();
                    let mut writer = $buf.writer();
                    let x: ZInt = rng.gen();
                    codec.write(&mut writer, x).unwrap();

                    let mut reader = $buf.reader();
                    let y: ZInt = codec.read(&mut reader).unwrap();
                    assert!(!reader.can_read());

                    assert_eq!(x, y);
                }
            };
        }

        println!("ZInt: encoding on Vec<u8>");
        let mut buffer = Vec::with_capacity(VLE_LEN);
        run!(buffer);

        println!("ZInt: encoding on ZBuf");
        let mut buffer = ZBuf::default();
        run!(buffer);
    }
}
