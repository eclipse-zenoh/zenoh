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
use crate::{RCodec, WCodec, Zenoh060, Zenoh060Header};
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::imsg,
    core::ZInt,
    zenoh::{zmsg, RoutingContext},
};

impl<W> WCodec<&RoutingContext, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &RoutingContext) -> Self::Output {
        // Header
        let header = zmsg::id::ROUTING_CONTEXT;
        self.write(&mut *writer, header)?;

        // Body
        self.write(&mut *writer, x.tree_id)?;
        Ok(())
    }
}

impl<R> RCodec<RoutingContext, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<RoutingContext, Self::Error> {
        let codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<RoutingContext, &mut R> for Zenoh060Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<RoutingContext, Self::Error> {
        if imsg::mid(self.header) != zmsg::id::ROUTING_CONTEXT {
            return Err(DidntRead);
        }

        let tree_id: ZInt = self.codec.read(&mut *reader)?;
        Ok(RoutingContext { tree_id })
    }
}
