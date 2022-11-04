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
use crate::*;
use std::convert::TryInto;
use uhlc::Timestamp;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
    ZBuf,
};
use zenoh_protocol::{
    common::imsg,
    core::{CongestionControl, Encoding, SampleKind, WireExpr, ZInt, ZenohId},
    zenoh::{zmsg, Data, DataInfo, ReplierInfo, ReplyContext},
};

// ReplyContext
impl<W> WCodec<&mut W, &ReplyContext> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ReplyContext) -> Self::Output {
        // Header
        let mut header = zmsg::id::REPLY_CONTEXT;
        if x.is_final() {
            header |= zmsg::flag::F;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, x.qid)?;
        if let Some(replier) = x.replier.as_ref() {
            self.write(&mut *writer, &replier.id)?;
        }
        Ok(())
    }
}

impl<R> RCodec<&mut R, ReplyContext> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ReplyContext, Self::Error> {
        let codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<&mut R, ReplyContext> for Zenoh060Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ReplyContext, Self::Error> {
        if imsg::mid(self.header) != zmsg::id::REPLY_CONTEXT {
            return Err(DidntRead);
        }

        let qid: ZInt = self.codec.read(&mut *reader)?;
        let replier = if imsg::has_flag(self.header, zmsg::flag::F) {
            None
        } else {
            let id: ZenohId = self.codec.read(&mut *reader)?;
            Some(ReplierInfo { id })
        };
        Ok(ReplyContext { qid, replier })
    }
}

// DataInfo
impl<W> WCodec<&mut W, &DataInfo> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &DataInfo) -> Self::Output {
        // Options
        let mut options = 0;
        #[cfg(feature = "shared-memory")]
        if self.sliced {
            options |= zmsg::data::info::SLICED;
        }
        if x.kind != SampleKind::Put {
            options |= zmsg::data::info::KIND;
        }
        if x.encoding.is_some() {
            options |= zmsg::data::info::ENCODING;
        }
        if x.timestamp.is_some() {
            options |= zmsg::data::info::TIMESTAMP;
        }
        self.write(&mut *writer, options)?;

        if x.kind != SampleKind::Put {
            self.write(&mut *writer, x.kind as ZInt)?;
        }
        if let Some(enc) = x.encoding.as_ref() {
            self.write(&mut *writer, u8::from(*enc.prefix()))?;
            self.write(&mut *writer, enc.suffix())?;
        }
        if let Some(ts) = x.timestamp.as_ref() {
            self.write(&mut *writer, ts)?;
        }

        Ok(())
    }
}

impl<R> RCodec<&mut R, DataInfo> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<DataInfo, Self::Error> {
        let mut info = DataInfo::default();
        let options: ZInt = self.read(&mut *reader)?;
        #[cfg(feature = "shared-memory")]
        {
            info.sliced = imsg::has_option(options, zmsg::data::info::SLICED);
        }
        if imsg::has_option(options, zmsg::data::info::KIND) {
            let kind: ZInt = self.read(&mut *reader)?;
            info.kind = kind.try_into().map_err(|_| DidntRead)?;
        }
        if imsg::has_option(options, zmsg::data::info::ENCODING) {
            let prefix: ZInt = self.read(&mut *reader)?;
            let suffix: String = self.read(&mut *reader)?;
            info.encoding = Some(Encoding::new(prefix, suffix).ok_or(DidntRead)?);
        }
        if imsg::has_option(options, zmsg::data::info::TIMESTAMP) {
            let timestamp: Timestamp = self.read(&mut *reader)?;
            info.timestamp = Some(timestamp);
        }

        Ok(info)
    }
}

// Data
impl<W> WCodec<&mut W, &Data> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Data) -> Self::Output {
        if let Some(reply_context) = x.reply_context.as_ref() {
            self.write(&mut *writer, reply_context)?;
        }

        // Header
        let mut header = zmsg::id::DATA;
        if x.data_info.is_some() {
            header |= zmsg::flag::I;
        }
        if x.key.has_suffix() {
            header |= zmsg::flag::K;
        }
        if x.congestion_control == CongestionControl::Drop {
            header |= zmsg::flag::D;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, &x.key)?;

        #[cfg(feature = "shared-memory")]
        {
            unimplemented!();
            let mut sliced = false;
        }

        if let Some(data_info) = x.data_info.as_ref() {
            self.write(&mut *writer, data_info)?;
            #[cfg(feature = "shared-memory")]
            {
                unimplemented!();
                sliced = data_info.sliced;
            }
        }

        #[cfg(feature = "shared-memory")]
        {
            // Write ZBuf payload
            unimplemented!();
        }

        #[cfg(not(feature = "shared-memory"))]
        self.write(&mut *writer, &x.payload)?;

        Ok(())
    }
}

impl<R> RCodec<&mut R, Data> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Data, Self::Error> {
        let mut codec = Zenoh060HeaderReplyContext {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        if imsg::mid(codec.header) == zmsg::id::REPLY_CONTEXT {
            let hodec = Zenoh060Header {
                header: codec.header,
                ..Default::default()
            };
            codec.reply_context = Some(hodec.read(&mut *reader)?);
            codec.header = self.read(&mut *reader)?;
        }
        codec.read(reader)
    }
}

impl<R> RCodec<&mut R, Data> for Zenoh060HeaderReplyContext
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Data, Self::Error> {
        if imsg::mid(self.header) != zmsg::id::DATA {
            return Err(DidntRead);
        }

        let congestion_control = if imsg::has_flag(self.header, zmsg::flag::D) {
            CongestionControl::Drop
        } else {
            CongestionControl::Block
        };

        let ccond = Zenoh060Condition {
            condition: imsg::has_flag(self.header, zmsg::flag::K),
            codec: self.codec,
        };
        let key: WireExpr<'static> = ccond.read(&mut *reader)?;

        #[cfg(feature = "shared-memory")]
        let mut sliced = false;

        let data_info = if imsg::has_flag(self.header, zmsg::flag::I) {
            let di: DataInfo = self.codec.read(&mut *reader)?;
            #[cfg(feature = "shared-memory")]
            {
                sliced = di.sliced;
            }
            Some(di)
        } else {
            None
        };

        #[cfg(feature = "shared-memory")]
        unimplemented!(); // Read ZBuf

        #[cfg(not(feature = "shared-memory"))]
        let payload: ZBuf = self.codec.read(&mut *reader)?;

        Ok(Data {
            key,
            data_info,
            payload,
            congestion_control,
            reply_context: self.reply_context,
        })
    }
}
