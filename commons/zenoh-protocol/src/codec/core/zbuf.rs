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
use crate::codec::*;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
    SplitBuffer, ZBuf,
};

impl<W> WCodec<&mut W, &ZBuf> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ZBuf) -> Self::Output {
        self.write(&mut *writer, x.len())?;
        for s in x.zslices() {
            writer.write_zslice(s)?;
        }
        Ok(())
    }
}

impl<R> RCodec<&mut R, ZBuf> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ZBuf, Self::Error> {
        let len: usize = self.read(&mut *reader)?;
        let mut zbuf = ZBuf::default();
        reader.read_zslices(len, |s| zbuf.push_zslice(s))?;
        Ok(zbuf)
    }
}

#[cfg(test)]
mod test {
    use crate::codec::*;
    use zenoh_buffers::reader::{HasReader, Reader};
    use zenoh_buffers::writer::{HasWriter, Writer};
    use zenoh_buffers::ZBuf;

    #[test]
    fn codec_zbuf() {
        let codec = Zenoh060::default();
        let bytes = (0..=u8::MAX).into_iter().collect::<Vec<u8>>();
        let mut x = ZBuf::default();
        let mut writer = x.writer();
        writer.write_exact(bytes.as_slice()).unwrap();

        // macro_rules! run {
        //     ($buf:expr) => {
        //         for _ in 0..TEST_ITER {
        //             $buf.clear();
        //             let mut writer = $buf.writer();
        //             codec.write(&mut writer, &x).unwrap();

        //             println!("{:?}", x);
        //             println!("{:?}", writer);

        //             let mut reader = $buf.reader();
        //             let y: ZBuf = codec.read(&mut reader).unwrap();
        //             println!("{:?}", y);
        //             println!("{:?}", reader);
        //             println!("Remaining: {}", reader.remaining());
        //             assert!(!reader.can_read());

        //             assert_eq!(x, y);
        //         }
        //     };
        // }

        // println!("ZBuf: encoding on Vec<u8>");
        // let mut buffer = Vec::with_capacity(ZenohId::MAX_SIZE);
        // run!(buffer);

        println!("ZBuf: encoding on ZBuf");
        let mut buffer = ZBuf::default();
        // run!(buffer);
        for _ in 0..TEST_ITER {
            buffer.clear();
            let mut writer = buffer.writer();
            codec.write(&mut writer, &x).unwrap();

            println!("{:?}", x);
            println!("{:?}", writer);

            let mut reader = buffer.reader();
            let y: ZBuf = codec.read(&mut reader).unwrap();
            println!("{:?}", y);
            println!("{:?}", reader);
            println!("Remaining: {}", reader.remaining());
            assert!(!reader.can_read());

            assert_eq!(x, y);
        }
    }
}
