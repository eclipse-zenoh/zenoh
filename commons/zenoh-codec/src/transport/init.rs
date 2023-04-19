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
    ZSlice,
};
use zenoh_protocol::{
    common::imsg,
    core::{WhatAmI, ZInt, ZenohId},
    defaults::SEQ_NUM_RES,
    transport::{tmsg, InitAck, InitSyn},
};

// InitSyn
impl<W> WCodec<&InitSyn, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &InitSyn) -> Self::Output {
        fn has_options(x: &InitSyn) -> bool {
            x.is_qos
        }

        fn options(x: &InitSyn) -> ZInt {
            let mut options = 0;
            if x.is_qos {
                options |= tmsg::init_options::QOS;
            }
            options
        }

        // Header
        let mut header = tmsg::id::INIT;
        if x.sn_resolution != SEQ_NUM_RES {
            header |= tmsg::flag::S;
        }
        if has_options(x) {
            header |= tmsg::flag::O;
        }
        self.write(&mut *writer, header)?;

        // Body
        if has_options(x) {
            self.write(&mut *writer, options(x))?;
        }
        self.write(&mut *writer, x.version)?;
        let wai: ZInt = x.whatami.into();
        self.write(&mut *writer, wai)?;
        self.write(&mut *writer, &x.zid)?;
        if imsg::has_flag(header, tmsg::flag::S) {
            self.write(&mut *writer, x.sn_resolution)?;
        }
        Ok(())
    }
}

impl<R> RCodec<InitSyn, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<InitSyn, Self::Error> {
        let codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<InitSyn, &mut R> for Zenoh060Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<InitSyn, Self::Error> {
        if imsg::mid(self.header) != tmsg::id::INIT || imsg::has_flag(self.header, tmsg::flag::A) {
            return Err(DidntRead);
        }

        let options: ZInt = if imsg::has_flag(self.header, tmsg::flag::O) {
            self.codec.read(&mut *reader)?
        } else {
            0
        };
        let version: u8 = self.codec.read(&mut *reader)?;
        let wai: ZInt = self.codec.read(&mut *reader)?;
        let whatami = WhatAmI::try_from(wai).ok_or(DidntRead)?;
        let zid: ZenohId = self.codec.read(&mut *reader)?;
        let sn_resolution: ZInt = if imsg::has_flag(self.header, tmsg::flag::S) {
            self.codec.read(&mut *reader)?
        } else {
            SEQ_NUM_RES
        };
        let is_qos = imsg::has_option(options, tmsg::init_options::QOS);

        Ok(InitSyn {
            version,
            whatami,
            zid,
            sn_resolution,
            is_qos,
        })
    }
}

// InitAck
impl<W> WCodec<&InitAck, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &InitAck) -> Self::Output {
        fn has_options(x: &InitAck) -> bool {
            x.is_qos
        }

        fn options(x: &InitAck) -> ZInt {
            let mut options = 0;
            if x.is_qos {
                options |= tmsg::init_options::QOS;
            }
            options
        }

        // Header
        let mut header = tmsg::id::INIT;
        header |= tmsg::flag::A;
        if x.sn_resolution.is_some() {
            header |= tmsg::flag::S;
        }
        if has_options(x) {
            header |= tmsg::flag::O;
        }
        self.write(&mut *writer, header)?;

        // Body
        if has_options(x) {
            self.write(&mut *writer, options(x))?;
        }
        let wai: ZInt = x.whatami.into();
        self.write(&mut *writer, wai)?;
        self.write(&mut *writer, &x.zid)?;
        if let Some(snr) = x.sn_resolution {
            self.write(&mut *writer, snr)?;
        }
        self.write(&mut *writer, &x.cookie)?;
        Ok(())
    }
}

impl<R> RCodec<InitAck, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<InitAck, Self::Error> {
        let codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<InitAck, &mut R> for Zenoh060Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<InitAck, Self::Error> {
        if imsg::mid(self.header) != imsg::id::INIT || !imsg::has_flag(self.header, tmsg::flag::A) {
            return Err(DidntRead);
        }

        let options: ZInt = if imsg::has_flag(self.header, tmsg::flag::O) {
            self.codec.read(&mut *reader)?
        } else {
            0
        };
        let wai: ZInt = self.codec.read(&mut *reader)?;
        let whatami = WhatAmI::try_from(wai).ok_or(DidntRead)?;
        let zid: ZenohId = self.codec.read(&mut *reader)?;
        let sn_resolution = if imsg::has_flag(self.header, tmsg::flag::S) {
            let snr: ZInt = self.codec.read(&mut *reader)?;
            Some(snr)
        } else {
            None
        };
        let is_qos = imsg::has_option(options, tmsg::init_options::QOS);
        let cookie: ZSlice = self.codec.read(&mut *reader)?;

        Ok(InitAck {
            whatami,
            zid,
            sn_resolution,
            is_qos,
            cookie,
        })
    }
}
