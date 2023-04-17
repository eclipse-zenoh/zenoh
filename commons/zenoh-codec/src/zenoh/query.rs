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
use alloc::string::String;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
    ZBuf,
};
use zenoh_protocol::{
    common::imsg,
    core::{ConsolidationMode, QueryTarget, WireExpr, ZInt},
    zenoh::{zmsg, DataInfo, Query, QueryBody},
};

// QueryTarget
impl<W> WCodec<&QueryTarget, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &QueryTarget) -> Self::Output {
        #![allow(clippy::unnecessary_cast)]
        match x {
            QueryTarget::BestMatching => self.write(&mut *writer, 0 as ZInt)?,
            QueryTarget::All => self.write(&mut *writer, 1 as ZInt)?,
            QueryTarget::AllComplete => self.write(&mut *writer, 2 as ZInt)?,
            #[cfg(feature = "complete_n")]
            QueryTarget::Complete(n) => {
                self.write(&mut *writer, 3 as ZInt)?;
                self.write(&mut *writer, *n)?;
            }
        }
        Ok(())
    }
}

impl<R> RCodec<QueryTarget, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<QueryTarget, Self::Error> {
        let t: ZInt = self.read(&mut *reader)?;
        let t = match t {
            0 => QueryTarget::BestMatching,
            1 => QueryTarget::All,
            2 => QueryTarget::AllComplete,
            #[cfg(feature = "complete_n")]
            3 => {
                let n: ZInt = self.read(&mut *reader)?;
                QueryTarget::Complete(n)
            }
            _ => return Err(DidntRead),
        };
        Ok(t)
    }
}

// ConsolidationMode
impl<W> WCodec<&ConsolidationMode, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ConsolidationMode) -> Self::Output {
        let cm: ZInt = match x {
            ConsolidationMode::None => 0,
            ConsolidationMode::Monotonic => 1,
            ConsolidationMode::Latest => 2,
        };
        self.write(&mut *writer, cm)?;
        Ok(())
    }
}

impl<R> RCodec<ConsolidationMode, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ConsolidationMode, Self::Error> {
        let cm: ZInt = self.read(&mut *reader)?;
        let cm = match cm {
            0 => ConsolidationMode::None,
            1 => ConsolidationMode::Monotonic,
            2 => ConsolidationMode::Latest,
            _ => return Err(DidntRead),
        };
        Ok(cm)
    }
}

// QueryBody
impl<W> WCodec<&QueryBody, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &QueryBody) -> Self::Output {
        self.write(&mut *writer, &x.data_info)?;
        self.write(&mut *writer, &x.payload)?;
        Ok(())
    }
}

impl<R> RCodec<QueryBody, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<QueryBody, Self::Error> {
        let data_info: DataInfo = self.read(&mut *reader)?;
        let payload: ZBuf = self.read(&mut *reader)?;
        Ok(QueryBody { data_info, payload })
    }
}

// Query
impl<W> WCodec<&Query, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Query) -> Self::Output {
        // Header
        let mut header = zmsg::id::QUERY;
        if x.target.is_some() {
            header |= zmsg::flag::T;
        }
        if x.body.is_some() {
            header |= zmsg::flag::B;
        }
        if x.key.has_suffix() {
            header |= zmsg::flag::K;
        }
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, &x.key)?;
        self.write(&mut *writer, x.parameters.as_str())?;
        self.write(&mut *writer, x.qid)?;
        if let Some(t) = x.target.as_ref() {
            self.write(&mut *writer, t)?;
        }
        self.write(&mut *writer, &x.consolidation)?;
        if let Some(b) = x.body.as_ref() {
            self.write(&mut *writer, b)?;
        }

        Ok(())
    }
}

impl<R> RCodec<Query, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Query, Self::Error> {
        let codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<Query, &mut R> for Zenoh060Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Query, Self::Error> {
        if imsg::mid(self.header) != zmsg::id::QUERY {
            return Err(DidntRead);
        }

        let ccond = Zenoh060Condition {
            condition: imsg::has_flag(self.header, zmsg::flag::K),
            codec: self.codec,
        };
        let key: WireExpr<'static> = ccond.read(&mut *reader)?;

        let parameters: String = self.codec.read(&mut *reader)?;
        let qid: ZInt = self.codec.read(&mut *reader)?;
        let target = if imsg::has_flag(self.header, zmsg::flag::T) {
            let qt: QueryTarget = self.codec.read(&mut *reader)?;
            Some(qt)
        } else {
            None
        };
        let consolidation: ConsolidationMode = self.codec.read(&mut *reader)?;
        let body = if imsg::has_flag(self.header, zmsg::flag::B) {
            let qb: QueryBody = self.codec.read(&mut *reader)?;
            Some(qb)
        } else {
            None
        };

        Ok(Query {
            key,
            parameters,
            qid,
            target,
            consolidation,
            body,
        })
    }
}
