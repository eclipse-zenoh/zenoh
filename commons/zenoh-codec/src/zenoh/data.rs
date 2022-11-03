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
};
use zenoh_protocol::{
    common::imsg,
    core::{Encoding, SampleKind, ZInt},
    proto::Data,
    zenoh::{zmsg, DataInfo},
};

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

// impl Options for DataInfo {
//     fn options(&self) -> ZInt {
//         let mut options = 0;
//         #[cfg(feature = "shared-memory")]
//         if self.sliced {
//             options |= zmsg::data::info::SLICED;
//         }
//         if self.kind != SampleKind::Put {
//             options |= zmsg::data::info::KIND;
//         }
//         if self.encoding.is_some() {
//             options |= zmsg::data::info::ENCODING;
//         }
//         if self.timestamp.is_some() {
//             options |= zmsg::data::info::TIMESTAMP;
//         }
//         options
//     }

//     fn has_options(&self) -> bool {
//         macro_rules! sliced {
//             ($info:expr) => {{
//                 #[cfg(feature = "shared-memory")]
//                 {
//                     $info.sliced
//                 }
//                 #[cfg(not(feature = "shared-memory"))]
//                 {
//                     false
//                 }
//             }};
//         }

//         sliced!(self)
//             || self.kind != SampleKind::Put
//             || self.encoding.is_some()
//             || self.timestamp.is_some()
//     }
// }

// impl Header for Data {
//     #[inline(always)]
//     fn header(&self) -> u8 {
//         let mut header = zmsg::id::DATA;
//         if self.data_info.is_some() {
//             header |= zmsg::flag::I;
//         }
//         if self.key.has_suffix() {
//             header |= zmsg::flag::K;
//         }
//         if self.congestion_control == CongestionControl::Drop {
//             header |= zmsg::flag::D;
//         }
//         header
//     }
// }
