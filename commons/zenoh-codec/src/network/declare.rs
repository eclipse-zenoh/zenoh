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
use alloc::string::String;

use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, HasWriter, Writer},
    ZBuf,
};
use zenoh_protocol::{
    common::{iext, imsg},
    core::{ExprId, ExprLen, WireExpr},
    network::{
        declare::{self, common, keyexpr, queryable, subscriber, token, Declare, DeclareBody},
        id, Mapping,
    },
};

use crate::{common::extension, RCodec, WCodec, Zenoh080, Zenoh080Condition, Zenoh080Header};

// Declaration
impl<W> WCodec<&DeclareBody, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &DeclareBody) -> Self::Output {
        match x {
            DeclareBody::DeclareKeyExpr(r) => self.write(&mut *writer, r)?,
            DeclareBody::UndeclareKeyExpr(r) => self.write(&mut *writer, r)?,
            DeclareBody::DeclareSubscriber(r) => self.write(&mut *writer, r)?,
            DeclareBody::UndeclareSubscriber(r) => self.write(&mut *writer, r)?,
            DeclareBody::DeclareQueryable(r) => self.write(&mut *writer, r)?,
            DeclareBody::UndeclareQueryable(r) => self.write(&mut *writer, r)?,
            DeclareBody::DeclareToken(r) => self.write(&mut *writer, r)?,
            DeclareBody::UndeclareToken(r) => self.write(&mut *writer, r)?,
            DeclareBody::DeclareFinal(r) => self.write(&mut *writer, r)?,
        }

        Ok(())
    }
}

impl<R> RCodec<DeclareBody, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<DeclareBody, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);

        use declare::id::*;
        let d = match imsg::mid(codec.header) {
            D_KEYEXPR => DeclareBody::DeclareKeyExpr(codec.read(&mut *reader)?),
            U_KEYEXPR => DeclareBody::UndeclareKeyExpr(codec.read(&mut *reader)?),
            D_SUBSCRIBER => DeclareBody::DeclareSubscriber(codec.read(&mut *reader)?),
            U_SUBSCRIBER => DeclareBody::UndeclareSubscriber(codec.read(&mut *reader)?),
            D_QUERYABLE => DeclareBody::DeclareQueryable(codec.read(&mut *reader)?),
            U_QUERYABLE => DeclareBody::UndeclareQueryable(codec.read(&mut *reader)?),
            D_TOKEN => DeclareBody::DeclareToken(codec.read(&mut *reader)?),
            U_TOKEN => DeclareBody::UndeclareToken(codec.read(&mut *reader)?),
            D_FINAL => DeclareBody::DeclareFinal(codec.read(&mut *reader)?),
            _ => return Err(DidntRead),
        };

        Ok(d)
    }
}

// Declare
impl<W> WCodec<&Declare, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Declare) -> Self::Output {
        let Declare {
            interest_id,
            ext_qos,
            ext_tstamp,
            ext_nodeid,
            body,
        } = x;

        // Header
        let mut header = id::DECLARE;
        if x.interest_id.is_some() {
            header |= declare::flag::I;
        }
        let mut n_exts = ((ext_qos != &declare::ext::QoSType::DEFAULT) as u8)
            + (ext_tstamp.is_some() as u8)
            + ((ext_nodeid != &declare::ext::NodeIdType::DEFAULT) as u8);
        if n_exts != 0 {
            header |= declare::flag::Z;
        }
        self.write(&mut *writer, header)?;

        if let Some(interest_id) = interest_id {
            self.write(&mut *writer, interest_id)?;
        }

        // Extensions
        if ext_qos != &declare::ext::QoSType::DEFAULT {
            n_exts -= 1;
            self.write(&mut *writer, (*ext_qos, n_exts != 0))?;
        }
        if let Some(ts) = ext_tstamp.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (ts, n_exts != 0))?;
        }
        if ext_nodeid != &declare::ext::NodeIdType::DEFAULT {
            n_exts -= 1;
            self.write(&mut *writer, (*ext_nodeid, n_exts != 0))?;
        }

        // Body
        self.write(&mut *writer, body)?;

        Ok(())
    }
}

impl<R> RCodec<Declare, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Declare, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);

        codec.read(reader)
    }
}

impl<R> RCodec<Declare, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Declare, Self::Error> {
        if imsg::mid(self.header) != id::DECLARE {
            return Err(DidntRead);
        }

        let mut interest_id = None;
        if imsg::has_flag(self.header, declare::flag::I) {
            interest_id = Some(self.codec.read(&mut *reader)?);
        }

        // Extensions
        let mut ext_qos = declare::ext::QoSType::DEFAULT;
        let mut ext_tstamp = None;
        let mut ext_nodeid = declare::ext::NodeIdType::DEFAULT;

        let mut has_ext = imsg::has_flag(self.header, declare::flag::Z);
        while has_ext {
            let ext: u8 = self.codec.read(&mut *reader)?;
            let eodec = Zenoh080Header::new(ext);
            match iext::eid(ext) {
                declare::ext::QoS::ID => {
                    let (q, ext): (declare::ext::QoSType, bool) = eodec.read(&mut *reader)?;
                    ext_qos = q;
                    has_ext = ext;
                }
                declare::ext::Timestamp::ID => {
                    let (t, ext): (declare::ext::TimestampType, bool) = eodec.read(&mut *reader)?;
                    ext_tstamp = Some(t);
                    has_ext = ext;
                }
                declare::ext::NodeId::ID => {
                    let (nid, ext): (declare::ext::NodeIdType, bool) = eodec.read(&mut *reader)?;
                    ext_nodeid = nid;
                    has_ext = ext;
                }
                _ => {
                    has_ext = extension::skip(reader, "Declare", ext)?;
                }
            }
        }

        // Body
        let body: DeclareBody = self.codec.read(&mut *reader)?;

        Ok(Declare {
            interest_id,
            ext_qos,
            ext_tstamp,
            ext_nodeid,
            body,
        })
    }
}

// Final
impl<W> WCodec<&common::DeclareFinal, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &common::DeclareFinal) -> Self::Output {
        let common::DeclareFinal = x;

        // Header
        let header = declare::id::D_FINAL;
        self.write(&mut *writer, header)?;

        Ok(())
    }
}

impl<R> RCodec<common::DeclareFinal, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<common::DeclareFinal, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);

        codec.read(reader)
    }
}

impl<R> RCodec<common::DeclareFinal, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<common::DeclareFinal, Self::Error> {
        if imsg::mid(self.header) != declare::id::D_FINAL {
            return Err(DidntRead);
        }

        // Extensions
        let has_ext = imsg::has_flag(self.header, token::flag::Z);
        if has_ext {
            extension::skip_all(reader, "Final")?;
        }

        Ok(common::DeclareFinal)
    }
}

// DeclareKeyExpr
impl<W> WCodec<&keyexpr::DeclareKeyExpr, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &keyexpr::DeclareKeyExpr) -> Self::Output {
        let keyexpr::DeclareKeyExpr { id, wire_expr } = x;

        // Header
        let mut header = declare::id::D_KEYEXPR;
        if wire_expr.has_suffix() {
            header |= keyexpr::flag::N;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, id)?;
        self.write(&mut *writer, wire_expr)?;

        Ok(())
    }
}

impl<R> RCodec<keyexpr::DeclareKeyExpr, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<keyexpr::DeclareKeyExpr, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);

        codec.read(reader)
    }
}

impl<R> RCodec<keyexpr::DeclareKeyExpr, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<keyexpr::DeclareKeyExpr, Self::Error> {
        if imsg::mid(self.header) != declare::id::D_KEYEXPR {
            return Err(DidntRead);
        }

        let id: ExprId = self.codec.read(&mut *reader)?;
        let ccond = Zenoh080Condition::new(imsg::has_flag(self.header, keyexpr::flag::N));
        let wire_expr: WireExpr<'static> = ccond.read(&mut *reader)?;

        // Extensions
        let has_ext = imsg::has_flag(self.header, keyexpr::flag::Z);
        if has_ext {
            extension::skip_all(reader, "DeclareKeyExpr")?;
        }

        Ok(keyexpr::DeclareKeyExpr { id, wire_expr })
    }
}

// UndeclareKeyExpr
impl<W> WCodec<&keyexpr::UndeclareKeyExpr, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &keyexpr::UndeclareKeyExpr) -> Self::Output {
        let keyexpr::UndeclareKeyExpr { id } = x;

        // Header
        let header = declare::id::U_KEYEXPR;
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, id)?;

        Ok(())
    }
}

impl<R> RCodec<keyexpr::UndeclareKeyExpr, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<keyexpr::UndeclareKeyExpr, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);

        codec.read(reader)
    }
}

impl<R> RCodec<keyexpr::UndeclareKeyExpr, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<keyexpr::UndeclareKeyExpr, Self::Error> {
        if imsg::mid(self.header) != declare::id::U_KEYEXPR {
            return Err(DidntRead);
        }

        let id: ExprId = self.codec.read(&mut *reader)?;

        // Extensions
        let has_ext = imsg::has_flag(self.header, keyexpr::flag::Z);
        if has_ext {
            extension::skip_all(reader, "UndeclareKeyExpr")?;
        }

        Ok(keyexpr::UndeclareKeyExpr { id })
    }
}

// DeclareSubscriber
impl<W> WCodec<&subscriber::DeclareSubscriber, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &subscriber::DeclareSubscriber) -> Self::Output {
        let subscriber::DeclareSubscriber { id, wire_expr } = x;

        // Header
        let mut header = declare::id::D_SUBSCRIBER;
        if wire_expr.mapping != Mapping::DEFAULT {
            header |= subscriber::flag::M;
        }
        if wire_expr.has_suffix() {
            header |= subscriber::flag::N;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, id)?;
        self.write(&mut *writer, wire_expr)?;

        // Extensions

        Ok(())
    }
}

impl<R> RCodec<subscriber::DeclareSubscriber, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<subscriber::DeclareSubscriber, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);

        codec.read(reader)
    }
}

impl<R> RCodec<subscriber::DeclareSubscriber, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<subscriber::DeclareSubscriber, Self::Error> {
        if imsg::mid(self.header) != declare::id::D_SUBSCRIBER {
            return Err(DidntRead);
        }

        // Body
        let id: subscriber::SubscriberId = self.codec.read(&mut *reader)?;
        let ccond = Zenoh080Condition::new(imsg::has_flag(self.header, subscriber::flag::N));
        let mut wire_expr: WireExpr<'static> = ccond.read(&mut *reader)?;
        wire_expr.mapping = if imsg::has_flag(self.header, subscriber::flag::M) {
            Mapping::Sender
        } else {
            Mapping::Receiver
        };

        // Extensions
        let mut has_ext = imsg::has_flag(self.header, subscriber::flag::Z);
        while has_ext {
            let ext: u8 = self.codec.read(&mut *reader)?;
            has_ext = extension::skip(reader, "DeclareSubscriber", ext)?;
        }

        Ok(subscriber::DeclareSubscriber { id, wire_expr })
    }
}

// UndeclareSubscriber
impl<W> WCodec<&subscriber::UndeclareSubscriber, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &subscriber::UndeclareSubscriber) -> Self::Output {
        let subscriber::UndeclareSubscriber { id, ext_wire_expr } = x;

        // Header
        let mut header = declare::id::U_SUBSCRIBER;
        if !ext_wire_expr.is_null() {
            header |= subscriber::flag::Z;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, id)?;

        // Extension
        if !ext_wire_expr.is_null() {
            self.write(&mut *writer, (ext_wire_expr, false))?;
        }

        Ok(())
    }
}

impl<R> RCodec<subscriber::UndeclareSubscriber, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<subscriber::UndeclareSubscriber, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);

        codec.read(reader)
    }
}

impl<R> RCodec<subscriber::UndeclareSubscriber, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<subscriber::UndeclareSubscriber, Self::Error> {
        if imsg::mid(self.header) != declare::id::U_SUBSCRIBER {
            return Err(DidntRead);
        }

        // Body
        let id: subscriber::SubscriberId = self.codec.read(&mut *reader)?;

        // Extensions
        let mut ext_wire_expr = common::ext::WireExprType::null();

        let mut has_ext = imsg::has_flag(self.header, subscriber::flag::Z);
        while has_ext {
            let ext: u8 = self.codec.read(&mut *reader)?;
            let eodec = Zenoh080Header::new(ext);
            match iext::eid(ext) {
                common::ext::WireExprExt::ID => {
                    let (we, ext): (common::ext::WireExprType, bool) = eodec.read(&mut *reader)?;
                    ext_wire_expr = we;
                    has_ext = ext;
                }
                _ => {
                    has_ext = extension::skip(reader, "UndeclareSubscriber", ext)?;
                }
            }
        }

        Ok(subscriber::UndeclareSubscriber { id, ext_wire_expr })
    }
}

// QueryableInfo
impl<W> WCodec<(&queryable::ext::QueryableInfoType, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;
    fn write(self, writer: &mut W, x: (&queryable::ext::QueryableInfoType, bool)) -> Self::Output {
        let (x, more) = x;

        let mut flags: u8 = 0;
        if x.complete {
            flags |= queryable::ext::flag::C;
        }
        let v: u64 = (flags as u64) | ((x.distance as u64) << 8);
        let ext = queryable::ext::QueryableInfo::new(v);

        self.write(&mut *writer, (&ext, more))
    }
}

impl<R> RCodec<(queryable::ext::QueryableInfoType, bool), &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(
        self,
        reader: &mut R,
    ) -> Result<(queryable::ext::QueryableInfoType, bool), Self::Error> {
        let (ext, more): (queryable::ext::QueryableInfo, bool) = self.read(&mut *reader)?;

        let complete = imsg::has_flag(ext.value as u8, queryable::ext::flag::C);
        let distance = (ext.value >> 8) as u16;

        Ok((
            queryable::ext::QueryableInfoType { complete, distance },
            more,
        ))
    }
}

// DeclareQueryable
impl<W> WCodec<&queryable::DeclareQueryable, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &queryable::DeclareQueryable) -> Self::Output {
        let queryable::DeclareQueryable {
            id,
            wire_expr,
            ext_info,
        } = x;

        // Header
        let mut header = declare::id::D_QUERYABLE;
        let mut n_exts = (ext_info != &queryable::ext::QueryableInfoType::DEFAULT) as u8;
        if n_exts != 0 {
            header |= subscriber::flag::Z;
        }
        if wire_expr.mapping != Mapping::DEFAULT {
            header |= subscriber::flag::M;
        }
        if wire_expr.has_suffix() {
            header |= subscriber::flag::N;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, id)?;
        self.write(&mut *writer, wire_expr)?;
        if ext_info != &queryable::ext::QueryableInfoType::DEFAULT {
            n_exts -= 1;
            self.write(&mut *writer, (ext_info, n_exts != 0))?;
        }

        Ok(())
    }
}

impl<R> RCodec<queryable::DeclareQueryable, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<queryable::DeclareQueryable, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);

        codec.read(reader)
    }
}

impl<R> RCodec<queryable::DeclareQueryable, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<queryable::DeclareQueryable, Self::Error> {
        if imsg::mid(self.header) != declare::id::D_QUERYABLE {
            return Err(DidntRead);
        }

        // Body
        let id: queryable::QueryableId = self.codec.read(&mut *reader)?;
        let ccond = Zenoh080Condition::new(imsg::has_flag(self.header, queryable::flag::N));
        let mut wire_expr: WireExpr<'static> = ccond.read(&mut *reader)?;
        wire_expr.mapping = if imsg::has_flag(self.header, queryable::flag::M) {
            Mapping::Sender
        } else {
            Mapping::Receiver
        };

        // Extensions
        let mut ext_info = queryable::ext::QueryableInfoType::DEFAULT;

        let mut has_ext = imsg::has_flag(self.header, queryable::flag::Z);
        while has_ext {
            let ext: u8 = self.codec.read(&mut *reader)?;
            let eodec = Zenoh080Header::new(ext);
            match iext::eid(ext) {
                queryable::ext::QueryableInfo::ID => {
                    let (i, ext): (queryable::ext::QueryableInfoType, bool) =
                        eodec.read(&mut *reader)?;
                    ext_info = i;
                    has_ext = ext;
                }
                _ => {
                    has_ext = extension::skip(reader, "DeclareQueryable", ext)?;
                }
            }
        }

        Ok(queryable::DeclareQueryable {
            id,
            wire_expr,
            ext_info,
        })
    }
}

// UndeclareQueryable
impl<W> WCodec<&queryable::UndeclareQueryable, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &queryable::UndeclareQueryable) -> Self::Output {
        let queryable::UndeclareQueryable { id, ext_wire_expr } = x;

        // Header
        let header = declare::id::U_QUERYABLE | queryable::flag::Z;
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, id)?;

        // Extension
        self.write(&mut *writer, (ext_wire_expr, false))?;

        Ok(())
    }
}

impl<R> RCodec<queryable::UndeclareQueryable, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<queryable::UndeclareQueryable, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);

        codec.read(reader)
    }
}

impl<R> RCodec<queryable::UndeclareQueryable, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<queryable::UndeclareQueryable, Self::Error> {
        if imsg::mid(self.header) != declare::id::U_QUERYABLE {
            return Err(DidntRead);
        }

        // Body
        let id: queryable::QueryableId = self.codec.read(&mut *reader)?;

        // Extensions
        let mut ext_wire_expr = common::ext::WireExprType::null();

        let mut has_ext = imsg::has_flag(self.header, queryable::flag::Z);
        while has_ext {
            let ext: u8 = self.codec.read(&mut *reader)?;
            let eodec = Zenoh080Header::new(ext);
            match iext::eid(ext) {
                common::ext::WireExprExt::ID => {
                    let (we, ext): (common::ext::WireExprType, bool) = eodec.read(&mut *reader)?;
                    ext_wire_expr = we;
                    has_ext = ext;
                }
                _ => {
                    has_ext = extension::skip(reader, "UndeclareQueryable", ext)?;
                }
            }
        }

        Ok(queryable::UndeclareQueryable { id, ext_wire_expr })
    }
}

// DeclareToken
impl<W> WCodec<&token::DeclareToken, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &token::DeclareToken) -> Self::Output {
        let token::DeclareToken { id, wire_expr } = x;

        // Header
        let mut header = declare::id::D_TOKEN;
        if wire_expr.mapping != Mapping::DEFAULT {
            header |= subscriber::flag::M;
        }
        if wire_expr.has_suffix() {
            header |= subscriber::flag::N;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, id)?;
        self.write(&mut *writer, wire_expr)?;

        Ok(())
    }
}

impl<R> RCodec<token::DeclareToken, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<token::DeclareToken, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);
        codec.read(reader)
    }
}

impl<R> RCodec<token::DeclareToken, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<token::DeclareToken, Self::Error> {
        if imsg::mid(self.header) != declare::id::D_TOKEN {
            return Err(DidntRead);
        }

        // Body
        let id: token::TokenId = self.codec.read(&mut *reader)?;
        let ccond = Zenoh080Condition::new(imsg::has_flag(self.header, token::flag::N));
        let mut wire_expr: WireExpr<'static> = ccond.read(&mut *reader)?;
        wire_expr.mapping = if imsg::has_flag(self.header, token::flag::M) {
            Mapping::Sender
        } else {
            Mapping::Receiver
        };

        // Extensions
        let has_ext = imsg::has_flag(self.header, token::flag::Z);
        if has_ext {
            extension::skip_all(reader, "DeclareToken")?;
        }

        Ok(token::DeclareToken { id, wire_expr })
    }
}

// UndeclareToken
impl<W> WCodec<&token::UndeclareToken, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &token::UndeclareToken) -> Self::Output {
        let token::UndeclareToken { id, ext_wire_expr } = x;

        // Header
        let header = declare::id::U_TOKEN | token::flag::Z;
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, id)?;

        // Extension
        self.write(&mut *writer, (ext_wire_expr, false))?;

        Ok(())
    }
}

impl<R> RCodec<token::UndeclareToken, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<token::UndeclareToken, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);

        codec.read(reader)
    }
}

impl<R> RCodec<token::UndeclareToken, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<token::UndeclareToken, Self::Error> {
        if imsg::mid(self.header) != declare::id::U_TOKEN {
            return Err(DidntRead);
        }

        // Body
        let id: token::TokenId = self.codec.read(&mut *reader)?;

        // Extensions
        let mut ext_wire_expr = common::ext::WireExprType::null();

        let mut has_ext = imsg::has_flag(self.header, token::flag::Z);
        while has_ext {
            let ext: u8 = self.codec.read(&mut *reader)?;
            let eodec = Zenoh080Header::new(ext);
            match iext::eid(ext) {
                common::ext::WireExprExt::ID => {
                    let (we, ext): (common::ext::WireExprType, bool) = eodec.read(&mut *reader)?;
                    ext_wire_expr = we;
                    has_ext = ext;
                }
                _ => {
                    has_ext = extension::skip(reader, "UndeclareToken", ext)?;
                }
            }
        }

        Ok(token::UndeclareToken { id, ext_wire_expr })
    }
}

// WARNING: this is a temporary extension used for undeclarations
impl<W> WCodec<(&common::ext::WireExprType, bool), &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: (&common::ext::WireExprType, bool)) -> Self::Output {
        let (x, more) = x;
        let common::ext::WireExprType { wire_expr } = x;

        let codec = Zenoh080::new();
        let mut value = ZBuf::empty();
        let mut zriter = value.writer();

        let mut flags: u8 = 0;
        if x.wire_expr.has_suffix() {
            flags |= 1;
        }
        if let Mapping::Sender = wire_expr.mapping {
            flags |= 1 << 1;
        }
        codec.write(&mut zriter, flags)?;

        codec.write(&mut zriter, wire_expr.scope)?;
        if wire_expr.has_suffix() {
            zriter.write_exact(wire_expr.suffix.as_bytes())?;
        }

        let ext = common::ext::WireExprExt { value };
        codec.write(&mut *writer, (&ext, more))?;

        Ok(())
    }
}

impl<R> RCodec<(common::ext::WireExprType, bool), &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<(common::ext::WireExprType, bool), Self::Error> {
        use zenoh_buffers::reader::HasReader;

        let (ext, more): (common::ext::WireExprExt, bool) = self.read(&mut *reader)?;

        let mut zeader = ext.value.reader();
        let flags: u8 = self.codec.read(&mut zeader)?;

        let scope: ExprLen = self.codec.read(&mut zeader)?;
        let suffix = if imsg::has_flag(flags, 1) {
            let mut buff = zenoh_buffers::vec::uninit(zeader.remaining());
            zeader.read_exact(&mut buff)?;
            String::from_utf8(buff).map_err(|_| DidntRead)?
        } else {
            String::new()
        };
        let mapping = if imsg::has_flag(flags, 1 << 1) {
            Mapping::Sender
        } else {
            Mapping::Receiver
        };

        Ok((
            common::ext::WireExprType {
                wire_expr: WireExpr {
                    scope,
                    suffix: suffix.into(),
                    mapping,
                },
            },
            more,
        ))
    }
}
