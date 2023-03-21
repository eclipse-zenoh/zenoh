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
use crate::{RCodec, WCodec, Zenoh080, Zenoh080Condition, Zenoh080Header};
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::{imsg, ZExtUnknown, ZExtZ64},
    core::{ExprId, WireExpr},
    network::{
        declare::{self, keyexpr, queryable, subscriber, token, Declare, DeclareBody},
        id, Mapping,
    },
};

// Declaration
impl<W> WCodec<&DeclareBody, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &DeclareBody) -> Self::Output {
        match x {
            DeclareBody::DeclareKeyExpr(r) => self.write(&mut *writer, r)?,
            DeclareBody::ForgetKeyExpr(r) => self.write(&mut *writer, r)?,
            DeclareBody::DeclareSubscriber(r) => self.write(&mut *writer, r)?,
            DeclareBody::ForgetSubscriber(r) => self.write(&mut *writer, r)?,
            DeclareBody::DeclareQueryable(r) => self.write(&mut *writer, r)?,
            DeclareBody::ForgetQueryable(r) => self.write(&mut *writer, r)?,
            DeclareBody::DeclareToken(r) => self.write(&mut *writer, r)?,
            DeclareBody::ForgetToken(r) => self.write(&mut *writer, r)?,
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
            F_KEYEXPR => DeclareBody::ForgetKeyExpr(codec.read(&mut *reader)?),
            D_SUBSCRIBER => DeclareBody::DeclareSubscriber(codec.read(&mut *reader)?),
            F_SUBSCRIBER => DeclareBody::ForgetSubscriber(codec.read(&mut *reader)?),
            D_QUERYABLE => DeclareBody::DeclareQueryable(codec.read(&mut *reader)?),
            F_QUERYABLE => DeclareBody::ForgetQueryable(codec.read(&mut *reader)?),
            D_TOKEN => DeclareBody::DeclareToken(codec.read(&mut *reader)?),
            F_TOKEN => DeclareBody::ForgetToken(codec.read(&mut *reader)?),
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
        // Header
        let mut header = id::DECLARE;
        let mut n_exts =
            ((x.ext_qos != declare::ext::QoS::default()) as u8) + (x.ext_tstamp.is_some() as u8);
        if n_exts != 0 {
            header |= declare::flag::Z;
        }
        self.write(&mut *writer, header)?;

        // Extensions
        if x.ext_qos != declare::ext::QoS::default() {
            n_exts -= 1;
            self.write(&mut *writer, (x.ext_qos, n_exts != 0))?;
        }
        if let Some(ts) = x.ext_tstamp.as_ref() {
            n_exts -= 1;
            self.write(&mut *writer, (ts, n_exts != 0))?;
        }

        // Body
        self.write(&mut *writer, &x.body)?;

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

        // Extensions
        let mut ext_qos = declare::ext::QoS::default();
        let mut ext_tstamp = None;

        let mut has_ext = imsg::has_flag(self.header, declare::flag::Z);
        while has_ext {
            let ext: u8 = self.codec.read(&mut *reader)?;
            let eodec = Zenoh080Header::new(ext);
            match imsg::mid(ext) {
                declare::ext::QOS => {
                    let (q, ext): (declare::ext::QoS, bool) = eodec.read(&mut *reader)?;
                    ext_qos = q;
                    has_ext = ext;
                }
                declare::ext::TSTAMP => {
                    let (t, ext): (declare::ext::Timestamp, bool) = eodec.read(&mut *reader)?;
                    ext_tstamp = Some(t);
                    has_ext = ext;
                }
                _ => {
                    let (_, ext): (ZExtUnknown, bool) = eodec.read(&mut *reader)?;
                    has_ext = ext;
                }
            }
        }

        // Body
        let body: DeclareBody = self.codec.read(&mut *reader)?;

        Ok(Declare {
            body,
            ext_qos,
            ext_tstamp,
        })
    }
}

// DeclareKeyExpr
impl<W> WCodec<&keyexpr::DeclareKeyExpr, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &keyexpr::DeclareKeyExpr) -> Self::Output {
        // Header
        let mut header = declare::id::D_KEYEXPR;
        if x.wire_expr.has_suffix() {
            header |= keyexpr::flag::N;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, x.id)?;
        self.write(&mut *writer, &x.wire_expr)?;

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
        let ccond = Zenoh080Condition {
            condition: imsg::has_flag(self.header, keyexpr::flag::N),
            codec: self.codec,
        };
        let wire_expr: WireExpr<'static> = ccond.read(&mut *reader)?;

        Ok(keyexpr::DeclareKeyExpr { id, wire_expr })
    }
}

// ForgetKeyExpr
impl<W> WCodec<&keyexpr::ForgetKeyExpr, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &keyexpr::ForgetKeyExpr) -> Self::Output {
        // Header
        let header = declare::id::F_KEYEXPR;
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, x.id)?;

        Ok(())
    }
}

impl<R> RCodec<keyexpr::ForgetKeyExpr, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<keyexpr::ForgetKeyExpr, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);

        codec.read(reader)
    }
}

impl<R> RCodec<keyexpr::ForgetKeyExpr, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<keyexpr::ForgetKeyExpr, Self::Error> {
        if imsg::mid(self.header) != declare::id::F_KEYEXPR {
            return Err(DidntRead);
        }

        let id: ExprId = self.codec.read(&mut *reader)?;

        Ok(keyexpr::ForgetKeyExpr { id })
    }
}

// SubscriberInfo
crate::impl_zextz64!(subscriber::ext::SubscriberInfo, subscriber::ext::INFO);

// DeclareSubscriber
impl<W> WCodec<&subscriber::DeclareSubscriber, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &subscriber::DeclareSubscriber) -> Self::Output {
        // Header
        let mut header = declare::id::D_SUBSCRIBER;
        let mut n_exts = (x.ext_info != subscriber::ext::SubscriberInfo::default()) as u8;
        if n_exts != 0 {
            header |= subscriber::flag::Z;
        }
        if x.mapping != Mapping::default() {
            header |= subscriber::flag::M;
        }
        if x.wire_expr.has_suffix() {
            header |= subscriber::flag::N;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, x.id)?;
        self.write(&mut *writer, &x.wire_expr)?;

        // Extensions
        if x.ext_info != subscriber::ext::SubscriberInfo::default() {
            n_exts -= 1;
            self.write(&mut *writer, (x.ext_info, n_exts != 0))?;
        }

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
        let wire_expr: WireExpr<'static> = ccond.read(&mut *reader)?;
        let mapping = if imsg::has_flag(self.header, subscriber::flag::M) {
            Mapping::Sender
        } else {
            Mapping::Receiver
        };

        // Extensions
        let mut ext_info = subscriber::ext::SubscriberInfo::default();

        let mut has_ext = imsg::has_flag(self.header, subscriber::flag::Z);
        while has_ext {
            let ext: u8 = self.codec.read(&mut *reader)?;
            let eodec = Zenoh080Header::new(ext);
            match imsg::mid(ext) {
                subscriber::ext::INFO => {
                    let (i, ext): (subscriber::ext::SubscriberInfo, bool) =
                        eodec.read(&mut *reader)?;
                    ext_info = i;
                    has_ext = ext;
                }
                _ => {
                    let (_, ext): (ZExtUnknown, bool) = eodec.read(&mut *reader)?;
                    has_ext = ext;
                }
            }
        }

        Ok(subscriber::DeclareSubscriber {
            id,
            wire_expr,
            mapping,
            ext_info,
        })
    }
}

// ForgetSubscriber
impl<W> WCodec<&subscriber::ForgetSubscriber, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &subscriber::ForgetSubscriber) -> Self::Output {
        // Header
        let header = declare::id::F_SUBSCRIBER;
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, x.id)?;

        Ok(())
    }
}

impl<R> RCodec<subscriber::ForgetSubscriber, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<subscriber::ForgetSubscriber, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);

        codec.read(reader)
    }
}

impl<R> RCodec<subscriber::ForgetSubscriber, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<subscriber::ForgetSubscriber, Self::Error> {
        if imsg::mid(self.header) != declare::id::F_SUBSCRIBER {
            return Err(DidntRead);
        }

        // Body
        let id: subscriber::SubscriberId = self.codec.read(&mut *reader)?;

        Ok(subscriber::ForgetSubscriber { id })
    }
}

// QueryableInfo
crate::impl_zextz64!(queryable::ext::QueryableInfo, queryable::ext::INFO);

// DeclareQueryable
impl<W> WCodec<&queryable::DeclareQueryable, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &queryable::DeclareQueryable) -> Self::Output {
        // Header
        let mut header = declare::id::D_QUERYABLE;
        let mut n_exts = (x.ext_info != queryable::ext::QueryableInfo::default()) as u8;
        if n_exts != 0 {
            header |= subscriber::flag::Z;
        }
        if x.mapping != Mapping::default() {
            header |= subscriber::flag::M;
        }
        if x.wire_expr.has_suffix() {
            header |= subscriber::flag::N;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, x.id)?;
        self.write(&mut *writer, &x.wire_expr)?;
        if x.ext_info != queryable::ext::QueryableInfo::default() {
            n_exts -= 1;
            self.write(&mut *writer, (x.ext_info, n_exts != 0))?;
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
        let wire_expr: WireExpr<'static> = ccond.read(&mut *reader)?;
        let mapping = if imsg::has_flag(self.header, queryable::flag::M) {
            Mapping::Sender
        } else {
            Mapping::Receiver
        };

        // Extensions
        let mut ext_info = queryable::ext::QueryableInfo::default();

        let mut has_ext = imsg::has_flag(self.header, queryable::flag::Z);
        while has_ext {
            let ext: u8 = self.codec.read(&mut *reader)?;
            let eodec = Zenoh080Header::new(ext);
            match imsg::mid(ext) {
                queryable::ext::INFO => {
                    let (i, ext): (queryable::ext::QueryableInfo, bool) =
                        eodec.read(&mut *reader)?;
                    ext_info = i;
                    has_ext = ext;
                }
                _ => {
                    let (_, ext): (ZExtUnknown, bool) = eodec.read(&mut *reader)?;
                    has_ext = ext;
                }
            }
        }

        Ok(queryable::DeclareQueryable {
            id,
            wire_expr,
            mapping,
            ext_info,
        })
    }
}

// ForgetQueryable
impl<W> WCodec<&queryable::ForgetQueryable, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &queryable::ForgetQueryable) -> Self::Output {
        // Header
        let header = declare::id::F_QUERYABLE;
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, x.id)?;

        Ok(())
    }
}

impl<R> RCodec<queryable::ForgetQueryable, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<queryable::ForgetQueryable, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);

        codec.read(reader)
    }
}

impl<R> RCodec<queryable::ForgetQueryable, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<queryable::ForgetQueryable, Self::Error> {
        if imsg::mid(self.header) != declare::id::F_QUERYABLE {
            return Err(DidntRead);
        }

        // Body
        let id: subscriber::SubscriberId = self.codec.read(&mut *reader)?;

        Ok(queryable::ForgetQueryable { id })
    }
}

// DeclareToken
impl<W> WCodec<&token::DeclareToken, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &token::DeclareToken) -> Self::Output {
        // Header
        let mut header = declare::id::D_TOKEN;
        if x.mapping != Mapping::default() {
            header |= subscriber::flag::M;
        }
        if x.wire_expr.has_suffix() {
            header |= subscriber::flag::N;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, x.id)?;
        self.write(&mut *writer, &x.wire_expr)?;

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
        let wire_expr: WireExpr<'static> = ccond.read(&mut *reader)?;
        let mapping = if imsg::has_flag(self.header, token::flag::M) {
            Mapping::Sender
        } else {
            Mapping::Receiver
        };

        // Extensions
        let mut has_ext = imsg::has_flag(self.header, subscriber::flag::Z);
        while has_ext {
            let (_, ext): (ZExtUnknown, bool) = self.codec.read(&mut *reader)?;
            has_ext = ext;
        }

        Ok(token::DeclareToken {
            id,
            wire_expr,
            mapping,
        })
    }
}

// ForgetToken
impl<W> WCodec<&token::ForgetToken, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &token::ForgetToken) -> Self::Output {
        // Header
        let header = declare::id::F_TOKEN;
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, x.id)?;

        Ok(())
    }
}

impl<R> RCodec<token::ForgetToken, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<token::ForgetToken, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);

        codec.read(reader)
    }
}

impl<R> RCodec<token::ForgetToken, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<token::ForgetToken, Self::Error> {
        if imsg::mid(self.header) != declare::id::F_TOKEN {
            return Err(DidntRead);
        }

        // Body
        let id: token::TokenId = self.codec.read(&mut *reader)?;

        Ok(token::ForgetToken { id })
    }
}
