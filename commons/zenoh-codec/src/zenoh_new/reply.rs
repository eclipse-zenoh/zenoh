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
use crate::{common::extension, RCodec, WCodec, Zenoh080, Zenoh080Bounded, Zenoh080Header};
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
    ZBuf,
};
use zenoh_protocol::{
    common::{iext, imsg},
    core::Encoding,
    zenoh_new::{
        id,
        reply::{ext, flag, Reply},
    },
};

impl<W> WCodec<&Reply, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Reply) -> Self::Output {
        // Header
        let mut header = id::REPLY;
        if x.timestamp.is_some() {
            header |= flag::T;
        }
        if x.encoding != Encoding::default() {
            header |= flag::E;
        }
        let mut n_exts = (x.ext_sinfo.is_some()) as u8
            + ((x.ext_consolidation != ext::ConsolidationType::default()) as u8);
        if n_exts != 0 {
            header |= flag::Z;
        }
        self.write(&mut *writer, header)?;

        // Body
        if let Some(ts) = x.timestamp.as_ref() {
            self.write(&mut *writer, ts)?;
        }
        if x.encoding != Encoding::default() {
            self.write(&mut *writer, &x.encoding)?;
        }

        // Extensions
        if let Some(sinfo) = x.ext_sinfo.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (sinfo, n_exts != 0))?;
        }
        if x.ext_consolidation != ext::ConsolidationType::default() {
            n_exts -= 1;
            self.write(&mut *writer, (x.ext_consolidation, n_exts != 0))?;
        }

        // Payload
        let bodec = Zenoh080Bounded::<u32>::new();
        bodec.write(&mut *writer, &x.payload)?;

        Ok(())
    }
}

impl<R> RCodec<Reply, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Reply, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R> RCodec<Reply, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Reply, Self::Error> {
        if imsg::mid(self.header) != id::REPLY {
            return Err(DidntRead);
        }

        // Body
        let mut timestamp: Option<uhlc::Timestamp> = None;
        if imsg::has_flag(self.header, flag::T) {
            timestamp = Some(self.codec.read(&mut *reader)?);
        }

        let mut encoding = Encoding::default();
        if imsg::has_flag(self.header, flag::E) {
            encoding = self.codec.read(&mut *reader)?;
        }

        // Extensions
        let mut ext_sinfo: Option<ext::SourceInfoType> = None;
        let mut ext_consolidation = ext::ConsolidationType::default();

        let mut has_ext = imsg::has_flag(self.header, flag::Z);
        while has_ext {
            let ext: u8 = self.codec.read(&mut *reader)?;
            let eodec = Zenoh080Header::new(ext);
            match iext::eid(ext) {
                ext::SourceInfo::ID => {
                    let (s, ext): (ext::SourceInfoType, bool) = eodec.read(&mut *reader)?;
                    ext_sinfo = Some(s);
                    has_ext = ext;
                }
                ext::Consolidation::ID => {
                    let (c, ext): (ext::ConsolidationType, bool) = eodec.read(&mut *reader)?;
                    ext_consolidation = c;
                    has_ext = ext;
                }
                _ => {
                    has_ext = extension::skip(reader, "Reply", ext)?;
                }
            }
        }

        // Payload
        let bodec = Zenoh080Bounded::<u32>::new();
        let payload: ZBuf = bodec.read(&mut *reader)?;

        Ok(Reply {
            timestamp,
            encoding,
            ext_sinfo,
            ext_consolidation,
            payload,
        })
    }
}
