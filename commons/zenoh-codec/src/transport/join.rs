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
use crate::*;
use std::time::Duration;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::imsg,
    core::{ConduitSn, ConduitSnList, Priority, WhatAmI, ZInt, ZenohId},
    proto::defaults::SEQ_NUM_RES,
    transport::{tmsg, Join},
};

impl<W> WCodec<&mut W, &Join> for Zenoh060
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
        zcwrite!(self, writer, header)?;
        if opts != 0 {
            zcwrite!(self, writer, options(x))?;
        }

        // Body
        zcwrite!(self, writer, x.version)?;
        let wai: ZInt = x.whatami.into();
        zcwrite!(self, writer, wai)?;
        zcwrite!(self, writer, &x.zid)?;
        if imsg::has_flag(header, tmsg::flag::T1) {
            zcwrite!(self, writer, x.lease.as_secs() as ZInt)?;
        } else {
            zcwrite!(self, writer, x.lease.as_millis() as ZInt)?;
        }
        if imsg::has_flag(header, tmsg::flag::S) {
            zcwrite!(self, writer, x.sn_resolution)?;
        }
        match &x.next_sns {
            ConduitSnList::Plain(sn) => {
                zcwrite!(self, writer, sn.reliable)?;
                zcwrite!(self, writer, sn.best_effort)?;
            }
            ConduitSnList::QoS(sns) => {
                for sn in sns.iter() {
                    zcwrite!(self, writer, sn.reliable)?;
                    zcwrite!(self, writer, sn.best_effort)?;
                }
            }
        }
        // true
        Ok(())
    }
}

impl<R> RCodec<&mut R, Join> for Zenoh060
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Join, Self::Error> {
        let codec = Zenoh060RCodec {
            header: zcread!(self, reader)?,
            ..Default::default()
        };
        codec.read(reader)
    }
}

impl<R> RCodec<&mut R, Join> for Zenoh060RCodec
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Join, Self::Error> {
        if imsg::mid(self.header) != tmsg::id::JOIN {
            return Err(DidntRead);
        }

        let options: ZInt = if imsg::has_flag(self.header, tmsg::flag::O) {
            zcread!(self.codec, reader)?
        } else {
            0
        };
        let version: u8 = zcread!(self.codec, reader)?;
        let wai: ZInt = zcread!(self.codec, reader)?;
        let whatami = WhatAmI::try_from(wai).ok_or(DidntRead)?;
        let zid: ZenohId = zcread!(self.codec, reader)?;
        let lease: ZInt = zcread!(self.codec, reader)?;
        let lease = if imsg::has_flag(self.header, tmsg::flag::T1) {
            Duration::from_secs(lease)
        } else {
            Duration::from_millis(lease)
        };
        let sn_resolution: ZInt = if imsg::has_flag(self.header, tmsg::flag::S) {
            zcread!(self.codec, reader)?
        } else {
            SEQ_NUM_RES
        };
        let is_qos = imsg::has_option(options, tmsg::init_options::QOS);
        let next_sns = if is_qos {
            let mut sns = Box::new([ConduitSn::default(); Priority::NUM]);
            for i in 0..Priority::NUM {
                sns[i].reliable = zcread!(self.codec, reader)?;
                sns[i].best_effort = zcread!(self.codec, reader)?;
            }
            ConduitSnList::QoS(sns)
        } else {
            ConduitSnList::Plain(ConduitSn {
                reliable: zcread!(self.codec, reader)?,
                best_effort: zcread!(self.codec, reader)?,
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
