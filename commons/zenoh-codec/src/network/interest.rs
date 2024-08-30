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
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_protocol::{
    common::{
        iext,
        imsg::{self, HEADER_BITS},
    },
    core::WireExpr,
    network::{
        declare, id,
        interest::{self, Interest, InterestMode, InterestOptions},
        Mapping,
    },
};

use crate::{common::extension, RCodec, WCodec, Zenoh080, Zenoh080Condition, Zenoh080Header};

// Interest
impl<W> WCodec<&Interest, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &Interest) -> Self::Output {
        let Interest {
            id,
            mode,
            options: _, // Compute the options on-the-fly according to Interest fields
            wire_expr,
            ext_qos,
            ext_tstamp,
            ext_nodeid,
        } = x;

        // Header
        let mut header = id::INTEREST;
        header |= match mode {
            InterestMode::Final => 0b00,
            InterestMode::Current => 0b01,
            InterestMode::Future => 0b10,
            InterestMode::CurrentFuture => 0b11,
        } << HEADER_BITS;
        let mut n_exts = ((ext_qos != &declare::ext::QoSType::DEFAULT) as u8)
            + (ext_tstamp.is_some() as u8)
            + ((ext_nodeid != &declare::ext::NodeIdType::DEFAULT) as u8);
        if n_exts != 0 {
            header |= declare::flag::Z;
        }
        self.write(&mut *writer, header)?;

        self.write(&mut *writer, id)?;

        if *mode != InterestMode::Final {
            self.write(&mut *writer, x.options())?;
            if let Some(we) = wire_expr.as_ref() {
                self.write(&mut *writer, we)?;
            }
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

        Ok(())
    }
}

impl<R> RCodec<Interest, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Interest, Self::Error> {
        let header: u8 = self.read(&mut *reader)?;
        let codec = Zenoh080Header::new(header);

        codec.read(reader)
    }
}

impl<R> RCodec<Interest, &mut R> for Zenoh080Header
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<Interest, Self::Error> {
        if imsg::mid(self.header) != id::INTEREST {
            return Err(DidntRead);
        }

        let id = self.codec.read(&mut *reader)?;
        let mode = match (self.header >> HEADER_BITS) & 0b11 {
            0b00 => InterestMode::Final,
            0b01 => InterestMode::Current,
            0b10 => InterestMode::Future,
            0b11 => InterestMode::CurrentFuture,
            _ => return Err(DidntRead),
        };

        let mut options = InterestOptions::empty();
        let mut wire_expr = None;
        if mode != InterestMode::Final {
            let options_byte: u8 = self.codec.read(&mut *reader)?;
            options = InterestOptions::from(options_byte);
            if options.restricted() {
                let ccond = Zenoh080Condition::new(options.named());
                let mut we: WireExpr<'static> = ccond.read(&mut *reader)?;
                we.mapping = if options.mapping() {
                    Mapping::Sender
                } else {
                    Mapping::Receiver
                };
                wire_expr = Some(we);
            }
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
                    let (q, ext): (interest::ext::QoSType, bool) = eodec.read(&mut *reader)?;
                    ext_qos = q;
                    has_ext = ext;
                }
                declare::ext::Timestamp::ID => {
                    let (t, ext): (interest::ext::TimestampType, bool) =
                        eodec.read(&mut *reader)?;
                    ext_tstamp = Some(t);
                    has_ext = ext;
                }
                declare::ext::NodeId::ID => {
                    let (nid, ext): (interest::ext::NodeIdType, bool) = eodec.read(&mut *reader)?;
                    ext_nodeid = nid;
                    has_ext = ext;
                }
                _ => {
                    has_ext = extension::skip(reader, "Declare", ext)?;
                }
            }
        }

        Ok(Interest {
            id,
            mode,
            options,
            wire_expr,
            ext_qos,
            ext_tstamp,
            ext_nodeid,
        })
    }
}
