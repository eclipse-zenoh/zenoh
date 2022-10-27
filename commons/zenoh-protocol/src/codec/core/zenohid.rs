use std::convert::TryFrom;

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
use crate::codec::{RCodec, WCodec, Zenoh060};
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol_core::ZenohId;

impl<W> WCodec<&mut W, &ZenohId> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ZenohId) -> Self::Output {
        self.write(&mut *writer, x.as_slice())
    }
}

impl<R> RCodec<&mut R, ZenohId> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ZenohId, Self::Error> {
        let size: usize = self.read(&mut *reader)?;
        if size > ZenohId::MAX_SIZE {
            return Err(DidntRead);
        }

        let mut id = [0; ZenohId::MAX_SIZE];
        reader.read_exact(&mut id[..size])?;
        ZenohId::try_from(&id[..size]).map_err(|_| DidntRead)
    }
}

#[cfg(test)]
mod test {
    use crate::codec::*;
    use zenoh_buffers::reader::{HasReader, Reader};
    use zenoh_buffers::writer::HasWriter;
    use zenoh_buffers::ZBuf;
    use zenoh_protocol_core::ZenohId;

    #[test]
    fn codec_zid() {
        let codec = Zenoh060::default();

        macro_rules! run {
            ($buf:expr) => {
                for _ in 0..TEST_ITER {
                    $buf.clear();
                    let mut writer = $buf.writer();
                    let x = ZenohId::default();
                    codec.write(&mut writer, &x).unwrap();

                    let mut reader = $buf.reader();
                    let y: ZenohId = codec.read(&mut reader).unwrap();
                    assert!(!reader.can_read());

                    assert_eq!(x, y);
                }
            };
        }

        println!("ZInt: encoding on Vec<u8>");
        let mut buffer = Vec::with_capacity(ZenohId::MAX_SIZE);
        run!(buffer);

        println!("ZInt: encoding on ZBuf");
        let mut buffer = ZBuf::default();
        run!(buffer);
    }
}
