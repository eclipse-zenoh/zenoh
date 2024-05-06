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
};
use zenoh_protocol::{
    common::{iext, imsg},
    core::WireExpr,
    network::{
        id,
        response::{ext, flag},
        Mapping, RequestId, Response, ResponseFinal,
    },
    zenoh::ResponseBody,
};

use crate::{
    common::extension, RCodec, WCodec, Zenoh080, Zenoh080Bounded, Zenoh080Condition, Zenoh080Header,
};

// Response
impl<W> WCodec<&Response, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Response) -> Self::Output {
        let Response {
            rid,
            wire_expr,
            payload,
            ext_qos,
            ext_tstamp,
            ext_respid,
        } = x;

        // Header
        let mut header = id::RESPONSE;
        let mut n_exts = ((ext_qos != &ext::QoSType::DEFAULT) as u8)
            + (ext_tstamp.is_some() as u8)
            + (ext_respid.is_some() as u8);
        if n_exts != 0 {
            header |= flag::Z;
        }
        if wire_expr.mapping != Mapping::DEFAULT {
            header |= flag::M;
        }
        if wire_expr.has_suffix() {
            header |= flag::N;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, rid)?;
        self.write(&mut *writer, wire_expr)?;

        // Extensions
        if ext_qos != &ext::QoSType::DEFAULT {
            n_exts -= 1;
            self.write(&mut *writer, (*ext_qos, n_exts != 0))?;
        }
        if let Some(ts) = ext_tstamp.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (ts, n_exts != 0))?;
        }
        if let Some(ri) = ext_respid.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (ri, n_exts != 0))?;
        }

        // Payload
        self.write(&mut *writer, payload)?;

        Ok(())
    }
}

impl<R> RCodec<Response, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Response, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R> RCodec<Response, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Response, Self::Error> {
        if imsg::mid(self.header) != id::RESPONSE {
            return Err(DidntRead);
        }

        // Body
        let bodec = Zenoh080Bounded::<RequestId>::new();
        let rid: RequestId = bodec.read(&mut *reader)?;
        let ccond = Zenoh080Condition::new(imsg::has_flag(self.header, flag::N));
        let mut wire_expr: WireExpr<'static> = ccond.read(&mut *reader)?;
        wire_expr.mapping = if imsg::has_flag(self.header, flag::M) {
            Mapping::Sender
        } else {
            Mapping::Receiver
        };

        // Extensions
        let mut ext_qos = ext::QoSType::DEFAULT;
        let mut ext_tstamp = None;
        let mut ext_respid = None;

        let mut has_ext = imsg::has_flag(self.header, flag::Z);
        while has_ext {
            let ext: u8 = self.codec.read(&mut *reader)?;
            let eodec = Zenoh080Header::new(ext);
            match iext::eid(ext) {
                ext::QoS::ID => {
                    let (q, ext): (ext::QoSType, bool) = eodec.read(&mut *reader)?;
                    ext_qos = q;
                    has_ext = ext;
                }
                ext::Timestamp::ID => {
                    let (t, ext): (ext::TimestampType, bool) = eodec.read(&mut *reader)?;
                    ext_tstamp = Some(t);
                    has_ext = ext;
                }
                ext::ResponderId::ID => {
                    let (t, ext): (ext::ResponderIdType, bool) = eodec.read(&mut *reader)?;
                    ext_respid = Some(t);
                    has_ext = ext;
                }
                _ => {
                    has_ext = extension::skip(reader, "Response", ext)?;
                }
            }
        }

        // Payload
        let payload: ResponseBody = self.codec.read(&mut *reader)?;

        Ok(Response {
            rid,
            wire_expr,
            payload,
            ext_qos,
            ext_tstamp,
            ext_respid,
        })
    }
}

// ResponseFinal
impl<W> WCodec<&ResponseFinal, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ResponseFinal) -> Self::Output {
        let ResponseFinal {
            rid,
            ext_qos,
            ext_tstamp,
        } = x;

        // Header
        let mut header = id::RESPONSE_FINAL;
        let mut n_exts = ((ext_qos != &ext::QoSType::DEFAULT) as u8) + (ext_tstamp.is_some() as u8);
        if n_exts != 0 {
            header |= flag::Z;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, rid)?;

        // Extensions
        if ext_qos != &ext::QoSType::DEFAULT {
            n_exts -= 1;
            self.write(&mut *writer, (*ext_qos, n_exts != 0))?;
        }
        if let Some(ts) = ext_tstamp.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (ts, n_exts != 0))?;
        }

        Ok(())
    }
}

impl<R> RCodec<ResponseFinal, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ResponseFinal, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R> RCodec<ResponseFinal, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ResponseFinal, Self::Error> {
        if imsg::mid(self.header) != id::RESPONSE_FINAL {
            return Err(DidntRead);
        }

        // Body
        let bodec = Zenoh080Bounded::<RequestId>::new();
        let rid: RequestId = bodec.read(&mut *reader)?;

        // Extensions
        let mut ext_qos = ext::QoSType::DEFAULT;
        let mut ext_tstamp = None;

        let mut has_ext = imsg::has_flag(self.header, flag::Z);
        while has_ext {
            let ext: u8 = self.codec.read(&mut *reader)?;
            let eodec = Zenoh080Header::new(ext);
            match iext::eid(ext) {
                ext::QoS::ID => {
                    let (q, ext): (ext::QoSType, bool) = eodec.read(&mut *reader)?;
                    ext_qos = q;
                    has_ext = ext;
                }
                ext::Timestamp::ID => {
                    let (t, ext): (ext::TimestampType, bool) = eodec.read(&mut *reader)?;
                    ext_tstamp = Some(t);
                    has_ext = ext;
                }
                _ => {
                    has_ext = extension::skip(reader, "ResponseFinal", ext)?;
                }
            }
        }

        Ok(ResponseFinal {
            rid,
            ext_qos,
            ext_tstamp,
        })
    }
}
