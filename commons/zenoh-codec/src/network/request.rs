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
        request::{ext, flag},
        Mapping, Request, RequestId,
    },
    zenoh::RequestBody,
};

use crate::{
    common::extension, RCodec, WCodec, Zenoh080, Zenoh080Bounded, Zenoh080Condition, Zenoh080Header,
};

// Target
impl<W> WCodec<(&ext::QueryTarget, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&ext::QueryTarget, bool)) -> Self::Output {
        let (x, more) = x;

        let v = match x {
            ext::QueryTarget::BestMatching => 0,
            ext::QueryTarget::All => 1,
            ext::QueryTarget::AllComplete => 2,
        };
        let ext = ext::Target::new(v);
        self.write(&mut *writer, (&ext, more))
    }
}

impl<R> RCodec<(ext::QueryTarget, bool), &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(ext::QueryTarget, bool), Self::Error> {
        let (ext, more): (ext::Target, bool) = self.read(&mut *reader)?;
        let rt = match ext.value {
            0 => ext::QueryTarget::BestMatching,
            1 => ext::QueryTarget::All,
            2 => ext::QueryTarget::AllComplete,
            _ => return Err(DidntRead),
        };
        Ok((rt, more))
    }
}

impl<W> WCodec<&Request, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Request) -> Self::Output {
        let Request {
            id,
            wire_expr,
            ext_qos,
            ext_tstamp,
            ext_nodeid,
            ext_target,
            ext_budget,
            ext_timeout,
            payload,
        } = x;

        // Header
        let mut header = id::REQUEST;
        let mut n_exts = ((ext_qos != &ext::QoSType::DEFAULT) as u8)
            + (ext_tstamp.is_some() as u8)
            + ((ext_target != &ext::QueryTarget::DEFAULT) as u8)
            + (ext_budget.is_some() as u8)
            + (ext_timeout.is_some() as u8)
            + ((ext_nodeid != &ext::NodeIdType::DEFAULT) as u8);
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
        self.write(&mut *writer, id)?;
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
        if ext_target != &ext::QueryTarget::DEFAULT {
            n_exts -= 1;
            self.write(&mut *writer, (ext_target, n_exts != 0))?;
        }
        if let Some(l) = ext_budget.as_ref() {
            n_exts -= 1;
            let e = ext::Budget::new(l.get() as u64);
            self.write(&mut *writer, (&e, n_exts != 0))?;
        }
        if let Some(to) = ext_timeout.as_ref() {
            n_exts -= 1;
            let e = ext::Timeout::new(to.as_millis() as u64);
            self.write(&mut *writer, (&e, n_exts != 0))?;
        }
        if ext_nodeid != &ext::NodeIdType::DEFAULT {
            n_exts -= 1;
            self.write(&mut *writer, (*ext_nodeid, n_exts != 0))?;
        }

        // Payload
        self.write(&mut *writer, payload)?;

        Ok(())
    }
}

impl<R> RCodec<Request, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Request, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R> RCodec<Request, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Request, Self::Error> {
        if imsg::mid(self.header) != id::REQUEST {
            return Err(DidntRead);
        }

        // Body
        let bodec = Zenoh080Bounded::<RequestId>::new();
        let id: RequestId = bodec.read(&mut *reader)?;
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
        let mut ext_nodeid = ext::NodeIdType::DEFAULT;
        let mut ext_target = ext::QueryTarget::DEFAULT;
        let mut ext_limit = None;
        let mut ext_timeout = None;

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
                ext::NodeId::ID => {
                    let (nid, ext): (ext::NodeIdType, bool) = eodec.read(&mut *reader)?;
                    ext_nodeid = nid;
                    has_ext = ext;
                }
                ext::Target::ID => {
                    let (rt, ext): (ext::QueryTarget, bool) = eodec.read(&mut *reader)?;
                    ext_target = rt;
                    has_ext = ext;
                }
                ext::Budget::ID => {
                    let (l, ext): (ext::Budget, bool) = eodec.read(&mut *reader)?;
                    ext_limit = ext::BudgetType::new(l.value as u32);
                    has_ext = ext;
                }
                ext::Timeout::ID => {
                    let (to, ext): (ext::Timeout, bool) = eodec.read(&mut *reader)?;
                    ext_timeout = Some(ext::TimeoutType::from_millis(to.value));
                    has_ext = ext;
                }
                _ => {
                    has_ext = extension::skip(reader, "Request", ext)?;
                }
            }
        }

        // Payload
        let payload: RequestBody = self.codec.read(&mut *reader)?;

        Ok(Request {
            id,
            wire_expr,
            payload,
            ext_qos,
            ext_tstamp,
            ext_nodeid,
            ext_target,
            ext_budget: ext_limit,
            ext_timeout,
        })
    }
}
