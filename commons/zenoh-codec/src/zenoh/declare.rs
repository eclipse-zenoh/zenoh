//
// Copyright (c) 2023 ZettaScale Technology
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
use crate::{RCodec, WCodec, Zenoh060, Zenoh060Condition, Zenoh060Header};
use alloc::vec::Vec;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::imsg,
    core::{QueryableInfo, Reliability, SubInfo, SubMode, WireExpr, ZInt},
    zenoh::{
        zmsg, Declaration, Declare, ForgetPublisher, ForgetQueryable, ForgetResource,
        ForgetSubscriber, Publisher, Queryable, Resource, Subscriber,
    },
};

// Declaration
impl<W> WCodec<&Declaration, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Declaration) -> Self::Output {
        match x {
            Declaration::Resource(r) => self.write(&mut *writer, r)?,
            Declaration::ForgetResource(r) => self.write(&mut *writer, r)?,
            Declaration::Publisher(r) => self.write(&mut *writer, r)?,
            Declaration::ForgetPublisher(r) => self.write(&mut *writer, r)?,
            Declaration::Subscriber(r) => self.write(&mut *writer, r)?,
            Declaration::ForgetSubscriber(r) => self.write(&mut *writer, r)?,
            Declaration::Queryable(r) => self.write(&mut *writer, r)?,
            Declaration::ForgetQueryable(r) => self.write(&mut *writer, r)?,
        }

        Ok(())
    }
}

impl<R> RCodec<Declaration, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Declaration, Self::Error> {
        use super::zmsg::declaration::id::*;

        let codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };

        let d = match imsg::mid(codec.header) {
            RESOURCE => Declaration::Resource(codec.read(&mut *reader)?),
            FORGET_RESOURCE => Declaration::ForgetResource(codec.read(&mut *reader)?),
            PUBLISHER => Declaration::Publisher(codec.read(&mut *reader)?),
            FORGET_PUBLISHER => Declaration::ForgetPublisher(codec.read(&mut *reader)?),
            SUBSCRIBER => Declaration::Subscriber(codec.read(&mut *reader)?),
            FORGET_SUBSCRIBER => Declaration::ForgetSubscriber(codec.read(&mut *reader)?),
            QUERYABLE => Declaration::Queryable(codec.read(&mut *reader)?),
            FORGET_QUERYABLE => Declaration::ForgetQueryable(codec.read(&mut *reader)?),
            _ => return Err(DidntRead),
        };

        Ok(d)
    }
}

// Declare
impl<W> WCodec<&Declare, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Declare) -> Self::Output {
        // Header
        let header = zmsg::id::DECLARE;
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, x.declarations.len())?;
        for d in x.declarations.iter() {
            self.write(&mut *writer, d)?;
        }

        Ok(())
    }
}

impl<R> RCodec<Declare, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Declare, Self::Error> {
        let codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<Declare, &mut R> for Zenoh060Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Declare, Self::Error> {
        if imsg::mid(self.header) != zmsg::id::DECLARE {
            return Err(DidntRead);
        }

        let len: usize = self.codec.read(&mut *reader)?;
        let mut declarations = Vec::with_capacity(len);
        for _ in 0..len {
            let d: Declaration = self.codec.read(&mut *reader)?;
            declarations.push(d);
        }

        Ok(Declare { declarations })
    }
}

// Resource
impl<W> WCodec<&Resource, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Resource) -> Self::Output {
        // Header
        let mut header = zmsg::declaration::id::RESOURCE;
        if x.key.has_suffix() {
            header |= zmsg::flag::K;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, x.expr_id)?;
        self.write(&mut *writer, &x.key)?;

        Ok(())
    }
}

impl<R> RCodec<Resource, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Resource, Self::Error> {
        let codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<Resource, &mut R> for Zenoh060Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Resource, Self::Error> {
        if imsg::mid(self.header) != zmsg::declaration::id::RESOURCE {
            return Err(DidntRead);
        }

        let expr_id: ZInt = self.codec.read(&mut *reader)?;
        let ccond = Zenoh060Condition {
            condition: imsg::has_flag(self.header, zmsg::flag::K),
            codec: self.codec,
        };
        let key: WireExpr<'static> = ccond.read(&mut *reader)?;

        Ok(Resource { expr_id, key })
    }
}

// ForgetResource
impl<W> WCodec<&ForgetResource, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ForgetResource) -> Self::Output {
        // Header
        let header = zmsg::declaration::id::FORGET_RESOURCE;
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, x.expr_id)?;

        Ok(())
    }
}

impl<R> RCodec<ForgetResource, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ForgetResource, Self::Error> {
        let codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<ForgetResource, &mut R> for Zenoh060Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ForgetResource, Self::Error> {
        if imsg::mid(self.header) != zmsg::declaration::id::FORGET_RESOURCE {
            return Err(DidntRead);
        }

        let expr_id: ZInt = self.codec.read(&mut *reader)?;

        Ok(ForgetResource { expr_id })
    }
}

// Publisher
impl<W> WCodec<&Publisher, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Publisher) -> Self::Output {
        // Header
        let mut header = zmsg::declaration::id::PUBLISHER;
        if x.key.has_suffix() {
            header |= zmsg::flag::K;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, &x.key)?;

        Ok(())
    }
}

impl<R> RCodec<Publisher, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Publisher, Self::Error> {
        let codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<Publisher, &mut R> for Zenoh060Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Publisher, Self::Error> {
        if imsg::mid(self.header) != zmsg::declaration::id::PUBLISHER {
            return Err(DidntRead);
        }

        let ccond = Zenoh060Condition {
            condition: imsg::has_flag(self.header, zmsg::flag::K),
            codec: self.codec,
        };
        let key: WireExpr<'static> = ccond.read(&mut *reader)?;

        Ok(Publisher { key })
    }
}

// ForgetPublisher
impl<W> WCodec<&ForgetPublisher, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ForgetPublisher) -> Self::Output {
        // Header
        let mut header = zmsg::declaration::id::FORGET_PUBLISHER;
        if x.key.has_suffix() {
            header |= zmsg::flag::K;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, &x.key)?;

        Ok(())
    }
}

impl<R> RCodec<ForgetPublisher, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ForgetPublisher, Self::Error> {
        let codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<ForgetPublisher, &mut R> for Zenoh060Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ForgetPublisher, Self::Error> {
        if imsg::mid(self.header) != zmsg::declaration::id::FORGET_PUBLISHER {
            return Err(DidntRead);
        }

        let ccond = Zenoh060Condition {
            condition: imsg::has_flag(self.header, zmsg::flag::K),
            codec: self.codec,
        };
        let key: WireExpr<'static> = ccond.read(&mut *reader)?;

        Ok(ForgetPublisher { key })
    }
}

// SubMode
impl<W> WCodec<&SubMode, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &SubMode) -> Self::Output {
        // Header
        let header = match x {
            SubMode::Push => zmsg::declaration::id::MODE_PUSH,
            SubMode::Pull => zmsg::declaration::id::MODE_PULL,
        };
        self.write(&mut *writer, header)?;

        // Body

        Ok(())
    }
}

impl<R> RCodec<SubMode, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<SubMode, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;

        let mode = match header {
            zmsg::declaration::id::MODE_PUSH => SubMode::Push,
            zmsg::declaration::id::MODE_PULL => SubMode::Pull,
            _ => return Err(DidntRead),
        };

        Ok(mode)
    }
}

// Subscriber
impl<W> WCodec<&Subscriber, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Subscriber) -> Self::Output {
        // Header
        let mut header = zmsg::declaration::id::SUBSCRIBER;
        if x.info.reliability == Reliability::Reliable {
            header |= zmsg::flag::R;
        }
        if x.info.mode != SubMode::Push {
            header |= zmsg::flag::S;
        }
        if x.key.has_suffix() {
            header |= zmsg::flag::K;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, &x.key)?;
        if imsg::has_flag(header, zmsg::flag::S) {
            self.write(&mut *writer, &x.info.mode)?;
        }

        Ok(())
    }
}

impl<R> RCodec<Subscriber, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Subscriber, Self::Error> {
        let codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<Subscriber, &mut R> for Zenoh060Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Subscriber, Self::Error> {
        if imsg::mid(self.header) != zmsg::declaration::id::SUBSCRIBER {
            return Err(DidntRead);
        }

        let reliability = if imsg::has_flag(self.header, zmsg::flag::R) {
            Reliability::Reliable
        } else {
            Reliability::BestEffort
        };

        let ccond = Zenoh060Condition {
            condition: imsg::has_flag(self.header, zmsg::flag::K),
            codec: self.codec,
        };
        let key: WireExpr<'static> = ccond.read(&mut *reader)?;

        let mode: SubMode = if imsg::has_flag(self.header, zmsg::flag::S) {
            self.codec.read(&mut *reader)?
        } else {
            SubMode::Push
        };

        Ok(Subscriber {
            key,
            info: SubInfo { reliability, mode },
        })
    }
}

// ForgetSubscriber
impl<W> WCodec<&ForgetSubscriber, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ForgetSubscriber) -> Self::Output {
        // Header
        let mut header = zmsg::declaration::id::FORGET_SUBSCRIBER;
        if x.key.has_suffix() {
            header |= zmsg::flag::K;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, &x.key)?;

        Ok(())
    }
}

impl<R> RCodec<ForgetSubscriber, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ForgetSubscriber, Self::Error> {
        let codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<ForgetSubscriber, &mut R> for Zenoh060Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ForgetSubscriber, Self::Error> {
        if imsg::mid(self.header) != zmsg::declaration::id::FORGET_SUBSCRIBER {
            return Err(DidntRead);
        }

        let ccond = Zenoh060Condition {
            condition: imsg::has_flag(self.header, zmsg::flag::K),
            codec: self.codec,
        };
        let key: WireExpr<'static> = ccond.read(&mut *reader)?;

        Ok(ForgetSubscriber { key })
    }
}

// QueryableInfo
impl<W> WCodec<&QueryableInfo, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &QueryableInfo) -> Self::Output {
        self.write(&mut *writer, x.complete)?;
        self.write(&mut *writer, x.distance)?;

        Ok(())
    }
}

impl<R> RCodec<QueryableInfo, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<QueryableInfo, Self::Error> {
        let complete: ZInt = self.read(&mut *reader)?;
        let distance: ZInt = self.read(&mut *reader)?;

        Ok(QueryableInfo { complete, distance })
    }
}

// Queryable
impl<W> WCodec<&Queryable, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Queryable) -> Self::Output {
        // Header
        let mut header = zmsg::declaration::id::QUERYABLE;
        if x.info != QueryableInfo::default() {
            header |= zmsg::flag::Q;
        }
        if x.key.has_suffix() {
            header |= zmsg::flag::K;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, &x.key)?;
        if imsg::has_flag(header, zmsg::flag::Q) {
            self.write(&mut *writer, &x.info)?;
        }

        Ok(())
    }
}

impl<R> RCodec<Queryable, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Queryable, Self::Error> {
        let codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<Queryable, &mut R> for Zenoh060Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Queryable, Self::Error> {
        if imsg::mid(self.header) != zmsg::declaration::id::QUERYABLE {
            return Err(DidntRead);
        }

        let ccond = Zenoh060Condition {
            condition: imsg::has_flag(self.header, zmsg::flag::K),
            codec: self.codec,
        };
        let key: WireExpr<'static> = ccond.read(&mut *reader)?;

        let info: QueryableInfo = if imsg::has_flag(self.header, zmsg::flag::Q) {
            self.codec.read(&mut *reader)?
        } else {
            QueryableInfo::default()
        };

        Ok(Queryable { key, info })
    }
}

// ForgetQueryable
impl<W> WCodec<&ForgetQueryable, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ForgetQueryable) -> Self::Output {
        // Header
        let mut header = zmsg::declaration::id::FORGET_QUERYABLE;
        if x.key.has_suffix() {
            header |= zmsg::flag::K
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, &x.key)?;

        Ok(())
    }
}

impl<R> RCodec<ForgetQueryable, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ForgetQueryable, Self::Error> {
        let codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<ForgetQueryable, &mut R> for Zenoh060Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ForgetQueryable, Self::Error> {
        if imsg::mid(self.header) != zmsg::declaration::id::FORGET_QUERYABLE {
            return Err(DidntRead);
        }

        let ccond = Zenoh060Condition {
            condition: imsg::has_flag(self.header, zmsg::flag::K),
            codec: self.codec,
        };
        let key: WireExpr<'static> = ccond.read(&mut *reader)?;

        Ok(ForgetQueryable { key })
    }
}
