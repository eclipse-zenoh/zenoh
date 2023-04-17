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
mod data;
mod declare;
mod linkstate;
mod pull;
mod query;
mod routing;
mod unit;

use crate::{
    RCodec, WCodec, Zenoh060, Zenoh060Header, Zenoh060HeaderReplyContext, Zenoh060Reliability,
};
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::{imsg, Attachment},
    core::{Channel, Priority, Reliability},
    zenoh::{zmsg, ReplyContext, RoutingContext, ZenohBody, ZenohMessage},
};

impl<W> WCodec<&ZenohMessage, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &ZenohMessage) -> Self::Output {
        if let Some(a) = x.attachment.as_ref() {
            self.write(&mut *writer, a)?;
        }
        if let Some(r) = x.routing_context.as_ref() {
            self.write(&mut *writer, r)?;
        }
        if x.channel.priority != Priority::default() {
            self.write(&mut *writer, &x.channel.priority)?;
        }

        match &x.body {
            ZenohBody::Data(d) => self.write(&mut *writer, d),
            ZenohBody::Unit(u) => self.write(&mut *writer, u),
            ZenohBody::Pull(p) => self.write(&mut *writer, p),
            ZenohBody::Query(q) => self.write(&mut *writer, q),
            ZenohBody::Declare(d) => self.write(&mut *writer, d),
            ZenohBody::LinkStateList(l) => self.write(&mut *writer, l),
        }
    }
}

impl<R> RCodec<ZenohMessage, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ZenohMessage, Self::Error> {
        let codec = Zenoh060Reliability {
            reliability: Reliability::default(),
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<ZenohMessage, &mut R> for Zenoh060Reliability
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<ZenohMessage, Self::Error> {
        let mut codec = Zenoh060Header {
            header: self.codec.read(&mut *reader)?,
            ..Default::default()
        };

        let attachment = if imsg::mid(codec.header) == imsg::id::ATTACHMENT {
            let a: Attachment = codec.read(&mut *reader)?;
            codec.header = self.codec.read(&mut *reader)?;
            Some(a)
        } else {
            None
        };
        let routing_context = if imsg::mid(codec.header) == zmsg::id::ROUTING_CONTEXT {
            let r: RoutingContext = codec.read(&mut *reader)?;
            codec.header = self.codec.read(&mut *reader)?;
            Some(r)
        } else {
            None
        };
        let priority = if imsg::mid(codec.header) == zmsg::id::PRIORITY {
            let p: Priority = codec.read(&mut *reader)?;
            codec.header = self.codec.read(&mut *reader)?;
            p
        } else {
            Priority::default()
        };

        let body = match imsg::mid(codec.header) {
            zmsg::id::REPLY_CONTEXT => {
                let rc: ReplyContext = codec.read(&mut *reader)?;
                let rodec = Zenoh060HeaderReplyContext {
                    header: self.codec.read(&mut *reader)?,
                    reply_context: Some(rc),
                    ..Default::default()
                };
                match imsg::mid(rodec.header) {
                    zmsg::id::DATA => ZenohBody::Data(rodec.read(&mut *reader)?),
                    zmsg::id::UNIT => ZenohBody::Unit(rodec.read(&mut *reader)?),
                    _ => return Err(DidntRead),
                }
            }
            zmsg::id::DATA => {
                let rodec = Zenoh060HeaderReplyContext {
                    header: codec.header,
                    ..Default::default()
                };
                ZenohBody::Data(rodec.read(&mut *reader)?)
            }
            zmsg::id::UNIT => {
                let rodec = Zenoh060HeaderReplyContext {
                    header: codec.header,
                    ..Default::default()
                };
                ZenohBody::Unit(rodec.read(&mut *reader)?)
            }
            zmsg::id::PULL => ZenohBody::Pull(codec.read(&mut *reader)?),
            zmsg::id::QUERY => ZenohBody::Query(codec.read(&mut *reader)?),
            zmsg::id::DECLARE => ZenohBody::Declare(codec.read(&mut *reader)?),
            zmsg::id::LINK_STATE_LIST => ZenohBody::LinkStateList(codec.read(&mut *reader)?),
            _ => return Err(DidntRead),
        };

        Ok(ZenohMessage {
            body,
            attachment,
            channel: Channel {
                priority,
                reliability: self.reliability,
            },
            routing_context,
        })
    }
}
