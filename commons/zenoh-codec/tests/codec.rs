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
use rand::{
    distributions::{Alphanumeric, DistString},
    *,
};
use std::convert::TryFrom;
use std::sync::Arc;
use zenoh_buffers::{
    reader::{HasReader, Reader},
    writer::HasWriter,
    BBuf, ZBuf, ZSlice,
};
use zenoh_codec::*;
use zenoh_protocol::{common::*, core::*, scouting::*, transport::*, zenoh::*};

const NUM_ITER: usize = 100;
const MAX_PAYLOAD_SIZE: usize = 256;

macro_rules! run_single {
    ($type:ty, $rand:expr, $wcode:expr, $rcode:expr, $buff:expr) => {
        for _ in 0..NUM_ITER {
            let x: $type = $rand;

            $buff.clear();
            let mut writer = $buff.writer();
            $wcode.write(&mut writer, &x).unwrap();

            let mut reader = $buff.reader();
            let y: $type = $rcode.read(&mut reader).unwrap();
            assert!(!reader.can_read());

            assert_eq!(x, y);
        }
    };
}

macro_rules! run_fragmented {
    ($type:ty, $rand:expr, $wcode:expr, $rcode:expr) => {
        for _ in 0..NUM_ITER {
            let x: $type = $rand;

            let mut vbuf = vec![];
            let mut writer = vbuf.writer();
            $wcode.write(&mut writer, &x).unwrap();

            let mut zbuf = ZBuf::default();
            let mut reader = vbuf.reader();
            while let Ok(b) = reader.read_u8() {
                zbuf.push_zslice(vec![b].into());
            }

            let mut reader = zbuf.reader();
            let y: $type = $rcode.read(&mut reader).unwrap();
            assert!(!reader.can_read());

            assert_eq!(x, y);
        }
    };
}

macro_rules! run_buffers {
    ($type:ty, $rand:expr, $wcode:expr, $rcode:expr) => {
        println!("Vec<u8>: codec {}", std::any::type_name::<$type>());
        let mut buffer = vec![];
        run_single!($type, $rand, $wcode, $rcode, buffer);

        println!("BBuf: codec {}", std::any::type_name::<$type>());
        let mut buffer = BBuf::with_capacity(u16::MAX as usize);
        run_single!($type, $rand, $wcode, $rcode, buffer);

        println!("ZBuf: codec {}", std::any::type_name::<$type>());
        let mut buffer = ZBuf::default();
        run_single!($type, $rand, $wcode, $rcode, buffer);

        println!("ZSlice: codec {}", std::any::type_name::<$type>());
        for _ in 0..NUM_ITER {
            let x: $type = $rand;

            let mut buffer = vec![];
            let mut writer = buffer.writer();
            $wcode.write(&mut writer, &x).unwrap();

            let mut zslice = ZSlice::from(Arc::new(buffer));
            let mut reader = zslice.reader();
            let y: $type = $rcode.read(&mut reader).unwrap();
            assert!(!reader.can_read());

            assert_eq!(x, y);
        }

        println!("Fragmented: codec {}", std::any::type_name::<$type>());
        run_fragmented!($type, $rand, $wcode, $rcode)
    };
}

macro_rules! run {
    ($type:ty, $rand:expr) => {
        let codec = Zenoh060::default();
        run_buffers!($type, $rand, codec, codec);
    };
    ($type:ty, $rand:expr, $wcode:block, $rcode:block) => {
        run_buffers!($type, $rand, $wcode, $rcode);
    };
}

// Core
#[test]
fn codec_zint() {
    run!(ZInt, thread_rng().gen::<ZInt>());
}

#[test]
fn codec_string() {
    let mut rng = thread_rng();
    run!(String, {
        let len = rng.gen_range(0..16);
        Alphanumeric.sample_string(&mut rng, len)
    });
}

#[test]
fn codec_zid() {
    run!(ZenohId, ZenohId::default());
}

#[test]
fn codec_zbuf() {
    run!(
        ZBuf,
        ZBuf::rand(thread_rng().gen_range(1..=MAX_PAYLOAD_SIZE))
    );
}

#[test]
fn codec_endpoint() {
    run!(EndPoint, EndPoint::rand());
}

#[test]
fn codec_locator() {
    run!(Locator, Locator::rand());
}

#[test]
fn codec_timestamp() {
    run!(Timestamp, {
        let time = uhlc::NTP64(thread_rng().gen());
        let id = uhlc::ID::try_from(ZenohId::rand().to_le_bytes()).unwrap();
        Timestamp::new(time, id)
    });
}

#[test]
fn codec_encoding() {
    run!(Encoding, Encoding::rand());
}

// Common
#[test]
fn codec_attachment() {
    run!(Attachment, Attachment::rand());
}

// Scouting
#[test]
fn codec_hello() {
    run!(Hello, Hello::rand());
}

#[test]
fn codec_scout() {
    run!(Scout, Scout::rand());
}

#[test]
fn codec_scouting() {
    run!(ScoutingMessage, ScoutingMessage::rand());
}

// Transport
#[test]
fn codec_init_syn() {
    run!(InitSyn, InitSyn::rand());
}

#[test]
fn codec_init_ack() {
    run!(InitAck, InitAck::rand());
}

#[test]
fn codec_open_syn() {
    run!(OpenSyn, OpenSyn::rand());
}

#[test]
fn codec_open_ack() {
    run!(OpenAck, OpenAck::rand());
}

#[test]
fn codec_join() {
    run!(Join, Join::rand());
}

#[test]
fn codec_close() {
    run!(Close, Close::rand());
}

#[test]
fn codec_keep_alive() {
    run!(KeepAlive, KeepAlive::rand());
}

#[test]
fn codec_frame_header() {
    run!(FrameHeader, FrameHeader::rand());
}

#[test]
fn codec_frame() {
    run!(Frame, Frame::rand());
}

#[test]
fn codec_transport() {
    run!(TransportMessage, TransportMessage::rand());
}

// Zenoh
#[test]
fn codec_routing_context() {
    run!(RoutingContext, RoutingContext::rand());
}

#[test]
fn codec_reply_context() {
    run!(ReplyContext, ReplyContext::rand());
}

#[test]
fn codec_data_info() {
    run!(DataInfo, DataInfo::rand());
}

#[test]
fn codec_data() {
    run!(Data, Data::rand());
}

#[test]
fn codec_unit() {
    run!(Unit, Unit::rand());
}

#[test]
fn codec_pull() {
    run!(Pull, Pull::rand());
}

#[test]
fn codec_query() {
    run!(Query, Query::rand());
}

#[test]
fn codec_declaration_resource() {
    run!(Resource, Resource::rand());
}

#[test]
fn codec_declaration_forget_resource() {
    run!(ForgetResource, ForgetResource::rand());
}

#[test]
fn codec_declaration_publisher() {
    run!(Publisher, Publisher::rand());
}

#[test]
fn codec_declaration_forget_publisher() {
    run!(ForgetPublisher, ForgetPublisher::rand());
}

#[test]
fn codec_declaration_subscriber() {
    run!(Subscriber, Subscriber::rand());
}

#[test]
fn codec_declaration_forget_subscriber() {
    run!(ForgetSubscriber, ForgetSubscriber::rand());
}

#[test]
fn codec_declaration_queryable() {
    run!(Queryable, Queryable::rand());
}

#[test]
fn codec_declaration_forget_queryable() {
    run!(ForgetQueryable, ForgetQueryable::rand());
}

#[test]
fn codec_declaration() {
    run!(Declaration, Declaration::rand());
}

#[test]
fn codec_declare() {
    run!(Declare, Declare::rand());
}

#[test]
fn codec_link_state() {
    run!(LinkState, LinkState::rand());
}

#[test]
fn codec_link_state_list() {
    run!(LinkStateList, LinkStateList::rand());
}

#[test]
fn codec_zenoh() {
    run!(
        ZenohMessage,
        {
            let mut x = ZenohMessage::rand();
            x.channel.reliability = Reliability::Reliable;
            x
        },
        { Zenoh060::default() },
        { Zenoh060Reliability::new(Reliability::Reliable) }
    );
    run!(
        ZenohMessage,
        {
            let mut x = ZenohMessage::rand();
            x.channel.reliability = Reliability::BestEffort;
            x
        },
        { Zenoh060::default() },
        { Zenoh060Reliability::new(Reliability::BestEffort) }
    );
}
