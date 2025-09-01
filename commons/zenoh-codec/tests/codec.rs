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
use std::convert::TryFrom;

use rand::{
    distributions::{Alphanumeric, DistString},
    *,
};
use zenoh_buffers::{
    reader::{HasReader, Reader},
    writer::HasWriter,
    BBuf, ZBuf, ZSlice,
};
use zenoh_codec::*;
use zenoh_protocol::{
    common::*,
    core::*,
    network::{self, *},
    scouting::*,
    transport::{self, *},
    zenoh, zextunit, zextz64, zextzbuf,
};

#[test]
fn zbuf_test() {
    let mut buffer = vec![0u8; 64];

    let zbuf = ZBuf::empty();
    let mut writer = buffer.writer();

    let codec = Zenoh080::new();
    codec.write(&mut writer, &zbuf).unwrap();
    println!("Buffer: {buffer:?}");

    let mut reader = buffer.reader();
    let ret: ZBuf = codec.read(&mut reader).unwrap();
    assert_eq!(ret, zbuf);
}

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
            assert_eq!(x, y);
            assert!(!reader.can_read());
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

            let mut zbuf = ZBuf::empty();
            let mut reader = vbuf.reader();
            while let Ok(b) = reader.read_u8() {
                zbuf.push_zslice(vec![b].into());
            }

            let mut reader = zbuf.reader();
            let y: $type = $rcode.read(&mut reader).unwrap();
            assert_eq!(x, y);
            assert!(!reader.can_read());
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
        let mut buffer = ZBuf::empty();
        run_single!($type, $rand, $wcode, $rcode, buffer);

        println!("ZSlice: codec {}", std::any::type_name::<$type>());
        for _ in 0..NUM_ITER {
            let x: $type = $rand;

            let mut buffer = vec![];
            let mut writer = buffer.writer();
            $wcode.write(&mut writer, &x).unwrap();

            let mut zslice = ZSlice::from(buffer);
            let mut reader = zslice.reader();
            let y: $type = $rcode.read(&mut reader).unwrap();
            assert_eq!(x, y);
            assert!(!reader.can_read());
        }

        println!("Fragmented: codec {}", std::any::type_name::<$type>());
        run_fragmented!($type, $rand, $wcode, $rcode)
    };
}

macro_rules! run {
    ($type:ty, $rand:expr) => {
        let codec = Zenoh080::new();
        run_buffers!($type, $rand, codec, codec);
    };
    ($type:ty, $rand:expr, $wcode:block, $rcode:block) => {
        run_buffers!($type, $rand, $wcode, $rcode);
    };
}

// Core
#[test]
fn codec_zint() {
    run!(u8, { u8::MIN });
    run!(u8, { u8::MAX });
    run!(u8, { thread_rng().gen::<u8>() });

    run!(u16, { u16::MIN });
    run!(u16, { u16::MAX });
    run!(u16, { thread_rng().gen::<u16>() });

    run!(u32, { u32::MIN });
    run!(u32, { u32::MAX });
    run!(u32, { thread_rng().gen::<u32>() });

    run!(u64, { u64::MIN });
    run!(u64, { u64::MAX });
    let codec = Zenoh080::new();
    for i in 1..=codec.w_len(u64::MAX) {
        run!(u64, { 1 << (7 * i) });
    }
    run!(u64, { thread_rng().gen::<u64>() });

    run!(usize, { usize::MIN });
    run!(usize, { usize::MAX });
    run!(usize, thread_rng().gen::<usize>());
}

#[test]
fn codec_zint_len() {
    let codec = Zenoh080::new();

    let mut buff = vec![];
    let mut writer = buff.writer();
    let n: u64 = 0;
    codec.write(&mut writer, n).unwrap();
    assert_eq!(codec.w_len(n), buff.len());

    for i in 1..=codec.w_len(u64::MAX) {
        let mut buff = vec![];
        let mut writer = buff.writer();
        let n: u64 = 1 << (7 * i);
        codec.write(&mut writer, n).unwrap();
        println!("ZInt len: {n} {buff:02x?}");
        assert_eq!(codec.w_len(n), buff.len());
    }

    let mut buff = vec![];
    let mut writer = buff.writer();
    let n = u64::MAX;
    codec.write(&mut writer, n).unwrap();
    assert_eq!(codec.w_len(n), buff.len());
}

#[test]
fn codec_zint_bounded() {
    use crate::Zenoh080Bounded;

    // Check bounded encoding
    for i in [
        u8::MAX as u64,
        u16::MAX as u64,
        u32::MAX as u64,
        usize::MAX as u64,
        u64::MAX,
    ] {
        let mut buff = vec![];

        let codec = Zenoh080::new();
        let mut writer = buff.writer();
        codec.write(&mut writer, i).unwrap();

        macro_rules! check {
            ($zint:ty) => {
                println!("Bound: {}. Value: {}", std::any::type_name::<$zint>(), i);
                let zodec = Zenoh080Bounded::<$zint>::new();

                let mut btmp = vec![];
                let mut wtmp = btmp.writer();
                let w_res = zodec.write(&mut wtmp, i);

                let mut reader = buff.reader();
                let r_res: Result<$zint, _> = zodec.read(&mut reader);
                if i <= <$zint>::MAX as u64 {
                    w_res.unwrap();
                    assert_eq!(i, r_res.unwrap() as u64);
                } else {
                    assert!(w_res.is_err());
                    assert!(r_res.is_err());
                }
            };
        }

        check!(u8);
        check!(u16);
        check!(u32);
        check!(u64);
        check!(usize);
    }
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
fn codec_string_bounded() {
    use crate::Zenoh080Bounded;

    let mut rng = rand::thread_rng();
    let str = Alphanumeric.sample_string(&mut rng, 1 + u8::MAX as usize);

    let zodec = Zenoh080Bounded::<u8>::new();
    let codec = Zenoh080::new();

    let mut buff = vec![];

    let mut writer = buff.writer();
    assert!(zodec.write(&mut writer, &str).is_err());
    let mut writer = buff.writer();
    codec.write(&mut writer, &str).unwrap();

    let mut reader = buff.reader();
    let r_res: Result<String, _> = zodec.read(&mut reader);
    assert!(r_res.is_err());
    let mut reader = buff.reader();
    let r_res: Result<String, _> = codec.read(&mut reader);
    assert_eq!(str, r_res.unwrap());
}

#[test]
fn codec_zid() {
    run!(ZenohIdProto, ZenohIdProto::default());
}

#[test]
fn codec_zslice() {
    run!(
        ZSlice,
        ZSlice::rand(thread_rng().gen_range(1..=MAX_PAYLOAD_SIZE))
    );
}

#[test]
fn codec_zslice_bounded() {
    use crate::Zenoh080Bounded;

    let zslice = ZSlice::rand(1 + u8::MAX as usize);

    let zodec = Zenoh080Bounded::<u8>::new();
    let codec = Zenoh080::new();

    let mut buff = vec![];

    let mut writer = buff.writer();
    assert!(zodec.write(&mut writer, &zslice).is_err());
    let mut writer = buff.writer();
    codec.write(&mut writer, &zslice).unwrap();

    let mut reader = buff.reader();
    let r_res: Result<ZSlice, _> = zodec.read(&mut reader);
    assert!(r_res.is_err());
    let mut reader = buff.reader();
    let r_res: Result<ZSlice, _> = codec.read(&mut reader);
    assert_eq!(zslice, r_res.unwrap());
}

#[test]
fn codec_zbuf() {
    run!(
        ZBuf,
        ZBuf::rand(thread_rng().gen_range(1..=MAX_PAYLOAD_SIZE))
    );
}

#[test]
fn codec_zbuf_bounded() {
    use crate::Zenoh080Bounded;

    let zbuf = ZBuf::rand(1 + u8::MAX as usize);

    let zodec = Zenoh080Bounded::<u8>::new();
    let codec = Zenoh080::new();

    let mut buff = vec![];

    let mut writer = buff.writer();
    assert!(zodec.write(&mut writer, &zbuf).is_err());
    let mut writer = buff.writer();
    codec.write(&mut writer, &zbuf).unwrap();

    let mut reader = buff.reader();
    let r_res: Result<ZBuf, _> = zodec.read(&mut reader);
    assert!(r_res.is_err());
    let mut reader = buff.reader();
    let r_res: Result<ZBuf, _> = codec.read(&mut reader);
    assert_eq!(zbuf, r_res.unwrap());
}

#[test]
fn codec_locator() {
    run!(Locator, Locator::rand());
}

#[test]
fn codec_timestamp() {
    run!(Timestamp, {
        let time = uhlc::NTP64(thread_rng().gen());
        let id = uhlc::ID::try_from(ZenohIdProto::rand().to_le_bytes()).unwrap();
        Timestamp::new(time, id)
    });
}

#[test]
fn codec_encoding() {
    run!(Encoding, Encoding::rand());
}

#[cfg(feature = "shared-memory")]
#[test]
fn codec_shm_info() {
    use zenoh_shm::{metadata::descriptor::MetadataDescriptor, ShmBufInfo};

    run!(ShmBufInfo, {
        let mut rng = rand::thread_rng();
        ShmBufInfo::new(
            rng.gen(),
            MetadataDescriptor {
                id: rng.gen(),
                index: rng.gen(),
            },
            rng.gen(),
        )
    });
}

// Common
#[test]
fn codec_extension() {
    zenoh_util::init_log_from_env_or("error");

    macro_rules! run_extension_single {
        ($ext:ty, $buff:expr) => {
            let codec = Zenoh080::new();
            for _ in 0..NUM_ITER {
                let more: bool = thread_rng().gen();
                let x: (&$ext, bool) = (&<$ext>::rand(), more);

                $buff.clear();
                let mut writer = $buff.writer();
                codec.write(&mut writer, x).unwrap();

                let mut reader = $buff.reader();
                let y: ($ext, bool) = codec.read(&mut reader).unwrap();
                assert!(!reader.can_read());

                assert_eq!(x.0, &y.0);
                assert_eq!(x.1, y.1);

                let mut reader = $buff.reader();
                let _ = zenoh_codec::common::extension::skip_all(&mut reader, "Test");
            }
        };
    }

    macro_rules! run_extension {
        ($type:ty) => {
            println!("Vec<u8>: codec {}", std::any::type_name::<$type>());
            let mut buff = vec![];
            run_extension_single!($type, buff);

            println!("BBuf: codec {}", std::any::type_name::<$type>());
            let mut buff = BBuf::with_capacity(u16::MAX as usize);
            run_extension_single!($type, buff);

            println!("ZBuf: codec {}", std::any::type_name::<$type>());
            let mut buff = ZBuf::empty();
            run_extension_single!($type, buff);
        };
    }

    run_extension!(zextunit!(0x00, true));
    run_extension!(zextunit!(0x00, false));
    run_extension!(zextz64!(0x01, true));
    run_extension!(zextz64!(0x01, false));
    run_extension!(zextzbuf!(0x02, true));
    run_extension!(zextzbuf!(0x02, false));
    run_extension!(ZExtUnknown);
}

// Scouting
#[test]
fn codec_scout() {
    run!(Scout, Scout::rand());
}

#[test]
fn codec_hello() {
    run!(HelloProto, HelloProto::rand());
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
fn codec_fragment_header() {
    run!(FragmentHeader, FragmentHeader::rand());
}

#[test]
fn codec_fragment() {
    run!(Fragment, Fragment::rand());
}

#[test]
fn codec_transport_oam() {
    run!(transport::Oam, transport::Oam::rand());
}

#[test]
fn codec_transport() {
    run!(TransportMessage, TransportMessage::rand());
}

// Network
#[test]
fn codec_declare() {
    run!(Declare, Declare::rand());
}

#[test]
fn codec_declare_body() {
    run!(DeclareBody, DeclareBody::rand());
}

#[test]
fn codec_interest() {
    run!(Interest, Interest::rand());
}

#[test]
fn codec_declare_keyexpr() {
    run!(DeclareKeyExpr, DeclareKeyExpr::rand());
}

#[test]
fn codec_undeclare_keyexpr() {
    run!(UndeclareKeyExpr, UndeclareKeyExpr::rand());
}

#[test]
fn codec_declare_subscriber() {
    run!(DeclareSubscriber, DeclareSubscriber::rand());
}

#[test]
fn codec_undeclare_subscriber() {
    run!(UndeclareSubscriber, UndeclareSubscriber::rand());
}

#[test]
fn codec_declare_queryable() {
    run!(DeclareQueryable, DeclareQueryable::rand());
}

#[test]
fn codec_undeclare_queryable() {
    run!(UndeclareQueryable, UndeclareQueryable::rand());
}

#[test]
fn codec_declare_token() {
    run!(DeclareToken, DeclareToken::rand());
}

#[test]
fn codec_undeclare_token() {
    run!(UndeclareToken, UndeclareToken::rand());
}

#[test]
fn codec_push() {
    run!(Push, Push::rand());
}

#[test]
fn codec_request() {
    run!(Request, Request::rand());
}

#[test]
fn codec_response() {
    run!(Response, Response::rand());
}

#[test]
fn codec_response_final() {
    run!(ResponseFinal, ResponseFinal::rand());
}

#[test]
fn codec_network_oam() {
    run!(network::Oam, network::Oam::rand());
}

#[test]
fn codec_network() {
    run!(NetworkMessage, NetworkMessage::rand());
}

// Zenoh
#[test]
fn codec_put() {
    run!(zenoh::Put, zenoh::Put::rand());
}

#[test]
fn codec_del() {
    run!(zenoh::Del, zenoh::Del::rand());
}

#[test]
fn codec_query() {
    run!(zenoh::Query, zenoh::Query::rand());
}

#[test]
fn codec_reply() {
    run!(zenoh::Reply, zenoh::Reply::rand());
}

#[test]
fn codec_err() {
    run!(zenoh::Err, zenoh::Err::rand());
}
