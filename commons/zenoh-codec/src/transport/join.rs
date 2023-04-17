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
use alloc::boxed::Box;
use core::time::Duration;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::imsg,
    core::{ConduitSn, ConduitSnList, Priority, WhatAmI, ZInt, ZenohId},
    defaults::SEQ_NUM_RES,
    transport::{tmsg, Join},
};

impl<W> WCodec<&Join, &mut W> for Zenoh060
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Join) -> Self::Output {
        fn options(x: &Join) -> ZInt {
            let mut options = 0;
            if x.is_qos() {
                options |= tmsg::join_options::QOS;
            }
            options
        }

        // Header
        let mut header = tmsg::id::JOIN;
        if x.lease.as_millis() % 1_000 == 0 {
            header |= tmsg::flag::T1;
        }
        if x.sn_resolution != SEQ_NUM_RES {
            header |= tmsg::flag::S;
        }
        let opts = options(x);
        if opts != 0 {
            header |= tmsg::flag::O;
        }
        self.write(&mut *writer, header)?;
        if opts != 0 {
            self.write(&mut *writer, options(x))?;
        }

        // Body
        self.write(&mut *writer, x.version)?;
        let wai: ZInt = x.whatami.into();
        self.write(&mut *writer, wai)?;
        self.write(&mut *writer, &x.zid)?;
        if imsg::has_flag(header, tmsg::flag::T1) {
            self.write(&mut *writer, x.lease.as_secs() as ZInt)?;
        } else {
            self.write(&mut *writer, x.lease.as_millis() as ZInt)?;
        }
        if imsg::has_flag(header, tmsg::flag::S) {
            self.write(&mut *writer, x.sn_resolution)?;
        }
        match &x.next_sns {
            ConduitSnList::Plain(sn) => {
                self.write(&mut *writer, sn.reliable)?;
                self.write(&mut *writer, sn.best_effort)?;
            }
            ConduitSnList::QoS(sns) => {
                for sn in sns.iter() {
                    self.write(&mut *writer, sn.reliable)?;
                    self.write(&mut *writer, sn.best_effort)?;
                }
            }
        }
        // true
        Ok(())
    }
}

impl<R> RCodec<Join, &mut R> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Join, Self::Error> {
        let codec = Zenoh060Header {
            header: self.read(&mut *reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<Join, &mut R> for Zenoh060Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Join, Self::Error> {
        if imsg::mid(self.header) != tmsg::id::JOIN {
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
        let lease: ZInt = self.codec.read(&mut *reader)?;
        let lease = if imsg::has_flag(self.header, tmsg::flag::T1) {
            Duration::from_secs(lease)
        } else {
            Duration::from_millis(lease)
        };
        let sn_resolution: ZInt = if imsg::has_flag(self.header, tmsg::flag::S) {
            self.codec.read(&mut *reader)?
        } else {
            SEQ_NUM_RES
        };
        let is_qos = imsg::has_option(options, tmsg::init_options::QOS);
        let next_sns = if is_qos {
            let mut sns = Box::new([ConduitSn::default(); Priority::NUM]);
            for i in 0..Priority::NUM {
                sns[i].reliable = self.codec.read(&mut *reader)?;
                sns[i].best_effort = self.codec.read(&mut *reader)?;
            }
            ConduitSnList::QoS(sns)
        } else {
            ConduitSnList::Plain(ConduitSn {
                reliable: self.codec.read(&mut *reader)?,
                best_effort: self.codec.read(&mut *reader)?,
            })
        };

        Ok(Join {
            version,
            whatami,
            zid,
            lease,
            sn_resolution,
            next_sns,
        })
    }
}
