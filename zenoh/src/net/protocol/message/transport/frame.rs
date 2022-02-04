//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use super::{TransportId, TransportProto};
use crate::net::protocol::core::{Priority, Reliability, ZInt};
use crate::net::protocol::io::{WBuf, ZBuf};
use crate::net::protocol::message::extensions::{eid, has_more, ZExt, ZExtUnknown, ZExtension};
use crate::net::protocol::message::{has_flag, ZMessage, ZenohMessage};
use std::convert::TryFrom;

/// # Frame message
///
/// The [`Frame`] message is used to transmit one ore more complete serialized
/// [`crate::net::protocol::message::ZenohMessage`]. I.e., the total length of the
/// serialized [`crate::net::protocol::message::ZenohMessage`] (s) MUST be smaller
/// than the maximum batch size (i.e. 2^16-1) and the link MTU.
/// The [`Frame`] message is used as means to aggreate multiple
/// [`crate::net::protocol::message::ZenohMessage`] in a single atomic message that
/// goes on the wire. By doing so, many small messages can be batched together and
/// share common information like the sequence number.
///
/// The [`Frame`] message flow is the following:
///
/// ```text
///     A                   B
///     |      FRAME        |
///     |------------------>|
///     |                   |
/// ```
///
/// The [`Frame`] message structure is defined as follows:
///
/// ```text
/// Flags:
/// - R: Reliable       If R==1 it concerns the reliable channel, else the best-effort channel
/// - X: Reserved
/// - Z: Extensions     If Z==1 then zenoh extensions will follow.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|X|R|  FRAME  |
/// +-+-+-+---------+
/// %    seq num    %
/// +---------------+
/// ~  [FrameExts]  ~ if Flag(Z)==1
/// +---------------+
/// ~  [NetworkMsg] ~
/// +---------------+
/// ```
///
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65535 bytes.
///
#[derive(Clone, PartialEq, Debug)]
pub struct Frame {
    pub reliability: Reliability,
    pub sn: ZInt,
    pub payload: Vec<ZenohMessage>,
    pub exts: FrameExts,
}

impl Frame {
    // Header flags
    pub const FLAG_R: u8 = 1 << 5;
    // pub const FLAG_X: u8 = 1 << 6; // Reserved for future use
    pub const FLAG_Z: u8 = 1 << 7;

    pub fn new(reliability: Reliability, sn: ZInt, payload: Vec<ZenohMessage>) -> Self {
        Self {
            reliability,
            sn,
            payload,
            exts: FrameExts::default(),
        }
    }

    pub fn write_header(
        wbuf: &mut WBuf,
        reliability: Reliability,
        sn: ZInt,
        exts: &FrameExts,
    ) -> bool {
        // Compute extensions
        let has_exts = !exts.is_empty();

        // Build header
        let mut header = Self::ID;
        if let Reliability::Reliable = reliability {
            header |= Frame::FLAG_R;
        }
        if has_exts {
            header |= Frame::FLAG_Z;
        }

        // Write header
        zcheck!(wbuf.write(header));

        // Write body
        zcheck!(wbuf.write_zint(sn));

        // Write extensions
        if has_exts {
            zcheck!(exts.write(wbuf));
        }

        true
    }

    pub fn read_header(zbuf: &mut ZBuf, header: u8) -> Option<(Reliability, ZInt, FrameExts)> {
        let reliability = if has_flag(header, Frame::FLAG_R) {
            Reliability::Reliable
        } else {
            Reliability::BestEffort
        };

        let sn = zbuf.read_zint()?;

        let exts = if has_flag(header, Frame::FLAG_Z) {
            FrameExts::read(zbuf)?
        } else {
            FrameExts::default()
        };

        Some((reliability, sn, exts))
    }

    pub fn skip_body(zbuf: &mut ZBuf) -> Option<()> {
        while zbuf.can_read() {
            let pos = zbuf.get_pos();
            if zbuf.read_zenoh_message(Reliability::default()).is_none() {
                zbuf.set_pos(pos);
                break;
            }
        }
        Some(())
    }
}

impl ZMessage for Frame {
    type Proto = TransportProto;
    const ID: u8 = TransportId::Frame.id();

    fn write(&self, wbuf: &mut WBuf) -> bool {
        zcheck!(Frame::write_header(
            wbuf,
            self.reliability,
            self.sn,
            &self.exts
        ));

        // Write payload
        // @TODO: remove clone
        let mut pld = self.payload.clone();
        for zmsg in pld.iter_mut() {
            zcheck!(wbuf.write_zenoh_message(zmsg));
        }

        true
    }

    fn read(zbuf: &mut ZBuf, header: u8) -> Option<Frame> {
        let (reliability, sn, exts) = Frame::read_header(zbuf, header)?;

        let mut payload: Vec<ZenohMessage> = Vec::with_capacity(1);
        while zbuf.can_read() {
            let pos = zbuf.get_pos();
            if let Some(msg) = zbuf.read_zenoh_message(reliability) {
                payload.push(msg);
            } else if zbuf.set_pos(pos) {
                break;
            } else {
                return None;
            }
        }

        Some(Frame {
            reliability,
            sn,
            payload,
            exts,
        })
    }
}

/// # Frame message extensions
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FrameExtId {
    // 0x00: Reserved
    // 0x01: Reserved
    QoS = 0x02,
    // 0x03: Reserved
    // ..  : Reserved
    // 0x1f: Reserved
}

impl TryFrom<u8> for FrameExtId {
    type Error = ();

    fn try_from(id: u8) -> Result<Self, Self::Error> {
        const QOS: u8 = FrameExtId::QoS.id();

        match id {
            QOS => Ok(FrameExtId::QoS),
            _ => Err(()),
        }
    }
}

impl FrameExtId {
    const fn id(self) -> u8 {
        self as u8
    }
}

/// # QoS extension
///
/// It is an extension containing a Byte.
///  
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|X|X|X|X|prio |
/// +-+-+-+-+-+-+-+-+
///
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ZExtQoS<const ID: u8> {
    pub priority: Priority,
}

impl<const ID: u8> ZExtQoS<{ ID }> {
    pub fn new(priority: Priority) -> Self {
        Self { priority }
    }
}

impl<const ID: u8> ZExtension for ZExtQoS<{ ID }> {
    fn id(&self) -> u8 {
        ID
    }

    fn length(&self) -> usize {
        1
    }

    fn write(&self, wbuf: &mut WBuf) -> bool {
        wbuf.write(self.priority.id())
    }

    fn read(zbuf: &mut ZBuf, _header: u8, _length: usize) -> Option<Self> {
        let value = zbuf.read()?;
        let priority = Priority::try_from(value & 0b111).ok()?;
        Some(Self { priority })
    }
}

impl<const ID: u8> From<Priority> for ZExtQoS<{ ID }> {
    fn from(v: Priority) -> Self {
        Self::new(v)
    }
}

type FrameExtQoS = ZExt<ZExtQoS<{ FrameExtId::QoS.id() }>>;
type FrameExtUnk = ZExt<ZExtUnknown>;

#[derive(Clone, Default, Debug, PartialEq)]
pub struct FrameExts {
    pub qos: Option<FrameExtQoS>,
}

impl FrameExts {
    fn is_empty(&self) -> bool {
        self.qos.is_none()
    }

    fn write(&self, wbuf: &mut WBuf) -> bool {
        if let Some(qos) = self.qos.as_ref() {
            zcheck!(qos.write(wbuf, false));
        }

        true
    }

    fn read(zbuf: &mut ZBuf) -> Option<FrameExts> {
        let mut exts = FrameExts::default();

        loop {
            let header = zbuf.read()?;

            match FrameExtId::try_from(eid(header)) {
                Ok(id) => match id {
                    FrameExtId::QoS => {
                        let e: FrameExtQoS = ZExt::read(zbuf, header)?;
                        exts.qos = Some(e);
                    }
                },
                Err(_) => {
                    let _e: FrameExtUnk = ZExt::read(zbuf, header)?;
                }
            }

            if !has_more(header) {
                break;
            }
        }

        Some(exts)
    }
}
