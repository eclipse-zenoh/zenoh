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
use core::time::Duration;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
    ZSlice,
};
use zenoh_protocol::{
    common::imsg,
    core::ZInt,
    transport::{tmsg, OpenAck, OpenSyn},
};

// OpenSyn
impl<W> WCodec<&OpenSyn, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &OpenSyn) -> Self::Output {
        // Header
        let mut header = tmsg::id::OPEN;
        if x.lease.as_millis() % 1_000 == 0 {
            header |= tmsg::flag::T2;
        }
        self.write(&mut *writer, header)?;

        // Body
        if imsg::has_flag(header, tmsg::flag::T2) {
            self.write(&mut *writer, x.lease.as_secs() as ZInt)?;
        } else {
            self.write(&mut *writer, x.lease.as_millis() as ZInt)?;
        }
        self.write(&mut *writer, x.initial_sn)?;
        self.write(&mut *writer, &x.cookie)?;
        Ok(())
    }
}

impl<R> RCodec<OpenSyn, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<OpenSyn, Self::Error> {
        let codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<OpenSyn, &mut R> for Zenoh060Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<OpenSyn, Self::Error> {
        if imsg::mid(self.header) != imsg::id::OPEN || imsg::has_flag(self.header, tmsg::flag::A) {
            return Err(DidntRead);
        }

        let lease: ZInt = self.codec.read(&mut *reader)?;
        let lease = if imsg::has_flag(self.header, tmsg::flag::T2) {
            Duration::from_secs(lease)
        } else {
            Duration::from_millis(lease)
        };
        let initial_sn: ZInt = self.codec.read(&mut *reader)?;
        let cookie: ZSlice = self.codec.read(&mut *reader)?;

        Ok(OpenSyn {
            lease,
            initial_sn,
            cookie,
        })
    }
}

// OpenAck
impl<W> WCodec<&OpenAck, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &OpenAck) -> Self::Output {
        // Header
        let mut header = tmsg::id::OPEN;
        header |= tmsg::flag::A;
        // Verify that the timeout is expressed in seconds, i.e. subsec part is 0.
        if x.lease.subsec_nanos() == 0 {
            header |= tmsg::flag::T2;
        }
        self.write(&mut *writer, header)?;

        // Body
        if imsg::has_flag(header, tmsg::flag::T2) {
            self.write(&mut *writer, x.lease.as_secs() as ZInt)?;
        } else {
            self.write(&mut *writer, x.lease.as_millis() as ZInt)?;
        }
        self.write(&mut *writer, x.initial_sn)?;
        Ok(())
    }
}

impl<R> RCodec<OpenAck, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<OpenAck, Self::Error> {
        let codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<OpenAck, &mut R> for Zenoh060Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<OpenAck, Self::Error> {
        if imsg::mid(self.header) != tmsg::id::OPEN || !imsg::has_flag(self.header, tmsg::flag::A) {
            return Err(DidntRead);
        }

        let lease: ZInt = self.codec.read(&mut *reader)?;
        let lease = if imsg::has_flag(self.header, tmsg::flag::T2) {
            Duration::from_secs(lease)
        } else {
            Duration::from_millis(lease)
        };
        let initial_sn: ZInt = self.codec.read(&mut *reader)?;

        Ok(OpenAck { lease, initial_sn })
    }
}
