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
use rand::*;
use std::time::Duration;
use uhlc::Timestamp;
use zenoh::buf::ZSlice;
use zenoh::net::protocol::core::{whatami::WhatAmIMatcher, *};
use zenoh::net::protocol::io::{WBuf, ZBuf};
use zenoh::net::protocol::message::extensions::{ZExt, ZExtPolicy};
use zenoh::net::protocol::message::*;

const NUM_ITER: usize = 100;
const PROPS_LENGTH: usize = 3;
const PROP_MAX_SIZE: usize = 64;
const MAX_PAYLOAD_SIZE: usize = 256;

macro_rules! ztesth {
    ( $m:expr, $r:expr) => {
        let mut wbuf = WBuf::new(256, false);
        println!("\nWrite message: {:?}", $m);
        assert!($m.write(&mut wbuf));

        println!("Read message from: {:?}", wbuf);
        let mut zbuf = ZBuf::from(wbuf);
        let h = zbuf.read().unwrap();
        let r = $r(&mut zbuf, h).unwrap();

        println!("Message read: {:?}", r);
        assert_eq!($m, r);
    };
}

macro_rules! gen {
    ($name:ty) => {
        thread_rng().gen::<$name>()
    };
}

macro_rules! gen_bool {
    () => {
        thread_rng().gen_bool(0.5)
    };
}

macro_rules! option_gen {
    ($e:expr) => {
        if gen_bool!() {
            Some($e)
        } else {
            None
        }
    };
}

fn gen_buffer(max_size: usize) -> Vec<u8> {
    let len: usize = thread_rng().gen_range(1..max_size + 1);
    let mut buf: Vec<u8> = vec![0; len];
    thread_rng().fill(buf.as_mut_slice());
    buf
}

fn gen_zid() -> ZenohId {
    ZenohId::from(uuid::Uuid::new_v4())
}

fn gen_props(len: usize, max_size: usize) -> Vec<Property> {
    let mut props = Vec::with_capacity(len);
    for _ in 0..len {
        let key = gen!(ZInt);
        let value = gen_buffer(max_size);
        props.push(Property { key, value });
    }
    props
}

fn gen_routing_context() -> RoutingContext {
    RoutingContext::new(gen!(ZInt))
}

fn gen_reply_context(is_final: bool) -> ReplyContext {
    let qid = gen!(ZInt);
    let replier = if !is_final {
        Some(ReplierInfo {
            kind: thread_rng().gen_range(0..4),
            id: gen_zid(),
        })
    } else {
        None
    };
    ReplyContext::new(qid, replier)
}

fn gen_attachment() -> Attachment {
    let mut wbuf = WBuf::new(PROP_MAX_SIZE, false);
    let props = gen_props(PROPS_LENGTH, PROP_MAX_SIZE);
    wbuf.write_properties(&props);

    let zbuf = ZBuf::from(wbuf);
    Attachment::new(zbuf)
}

fn gen_declarations() -> Vec<Declaration> {
    vec![
        Declaration::Resource(Resource {
            expr_id: gen!(ZInt),
            key: gen_key(),
        }),
        Declaration::ForgetResource(ForgetResource {
            expr_id: gen!(ZInt),
        }),
        Declaration::Publisher(Publisher { key: gen_key() }),
        Declaration::ForgetPublisher(ForgetPublisher { key: gen_key() }),
        Declaration::Subscriber(Subscriber {
            key: gen_key(),
            info: SubInfo {
                reliability: Reliability::Reliable,
                mode: SubMode::Push,
                period: None,
            },
        }),
        Declaration::Subscriber(Subscriber {
            key: gen_key(),
            info: SubInfo {
                reliability: Reliability::BestEffort,
                mode: SubMode::Pull,
                period: None,
            },
        }),
        Declaration::Subscriber(Subscriber {
            key: gen_key(),
            info: SubInfo {
                reliability: Reliability::Reliable,
                mode: SubMode::Pull,
                period: Some(Period {
                    origin: gen!(ZInt),
                    period: gen!(ZInt),
                    duration: gen!(ZInt),
                }),
            },
        }),
        Declaration::Subscriber(Subscriber {
            key: gen_key(),
            info: SubInfo {
                reliability: Reliability::BestEffort,
                mode: SubMode::Push,
                period: Some(Period {
                    origin: gen!(ZInt),
                    period: gen!(ZInt),
                    duration: gen!(ZInt),
                }),
            },
        }),
        Declaration::ForgetSubscriber(ForgetSubscriber { key: gen_key() }),
        Declaration::Queryable(Queryable {
            key: gen_key(),
            kind: queryable::ALL_KINDS,
            info: QueryableInfo {
                complete: 1,
                distance: 0,
            },
        }),
        Declaration::Queryable(Queryable {
            key: gen_key(),
            kind: queryable::STORAGE,
            info: QueryableInfo {
                complete: 0,
                distance: 10,
            },
        }),
        Declaration::Queryable(Queryable {
            key: gen_key(),
            kind: queryable::EVAL,
            info: QueryableInfo {
                complete: 10,
                distance: 0,
            },
        }),
        Declaration::ForgetQueryable(ForgetQueryable {
            key: gen_key(),
            kind: queryable::ALL_KINDS,
        }),
        Declaration::ForgetQueryable(ForgetQueryable {
            key: gen_key(),
            kind: queryable::STORAGE,
        }),
        Declaration::ForgetQueryable(ForgetQueryable {
            key: gen_key(),
            kind: queryable::EVAL,
        }),
    ]
}

fn gen_key() -> KeyExpr<'static> {
    let num: u8 = thread_rng().gen_range(0..3);
    match num {
        0 => KeyExpr::from(gen!(ZInt)),
        1 => KeyExpr::from("my_resource"),
        _ => KeyExpr::from(gen!(ZInt)).with_suffix("my_resource"),
    }
}

fn gen_query_target() -> QueryTarget {
    let kind: ZInt = thread_rng().gen_range(0..4);
    let target = gen_target();
    QueryTarget { kind, target }
}

fn gen_target() -> Target {
    let num: u8 = thread_rng().gen_range(0..4);
    match num {
        0 => Target::BestMatching,
        1 => Target::All,
        2 => Target::AllComplete,
        #[cfg(feature = "complete_n")]
        4 => Target::Complete(3),
        _ => Target::None,
    }
}

fn gen_consolidation_mode() -> ConsolidationMode {
    let num: u8 = thread_rng().gen_range(0..3);
    match num {
        0 => ConsolidationMode::None,
        1 => ConsolidationMode::Lazy,
        _ => ConsolidationMode::Full,
    }
}

fn gen_consolidation() -> QueryConsolidation {
    QueryConsolidation {
        first_routers: gen_consolidation_mode(),
        last_router: gen_consolidation_mode(),
        reception: gen_consolidation_mode(),
    }
}

fn gen_timestamp() -> Timestamp {
    Timestamp::new(uhlc::NTP64(gen!(u64)), uhlc::ID::from(uuid::Uuid::new_v4()))
}

fn gen_data_info() -> DataInfo {
    DataInfo {
        kind: option_gen!(gen!(ZInt)),
        encoding: option_gen!(Encoding {
            prefix: gen!(ZInt),
            suffix: "".into()
        }),
        timestamp: option_gen!(gen_timestamp()),
        #[cfg(feature = "shared-memory")]
        sliced: false,
        source_id: option_gen!(gen_zid()),
        source_sn: option_gen!(gen!(ZInt)),
        first_router_id: option_gen!(gen_zid()),
        first_router_sn: option_gen!(gen!(ZInt)),
    }
}

fn gen_initial_sn() -> ConduitSn {
    ConduitSn {
        reliable: gen!(ZInt),
        best_effort: gen!(ZInt),
    }
}

fn gen_wireproperties() -> WireProperties {
    let mut wps = WireProperties::new();

    let num = gen!(usize) % 8;
    for _ in 0..num {
        let key = gen!(ZInt) % 8;
        let value = gen_buffer(1 + gen!(usize) % 7);
        wps.insert(key, value);
    }

    wps
}

/*************************************/
/*       SCOUTING MESSAGES           */
/*************************************/
#[test]
fn codec_scout() {
    for _ in 0..NUM_ITER {
        let version = [
            Version {
                stable: gen!(u8),
                experimental: None,
            },
            Version {
                stable: gen!(u8),
                experimental: NonZeroZInt::new(gen!(ZInt)),
            },
        ];
        let tmp = 1 + (gen!(u8) % 6);
        let what = WhatAmIMatcher::try_from(tmp).unwrap();
        let zid = [None, Some(gen_zid())];
        let u_ext = [
            None,
            Some(ZExt::new(gen_wireproperties(), ZExtPolicy::Ignore)),
            Some(ZExt::new(gen_wireproperties(), ZExtPolicy::Forward)),
        ];

        for v in version.iter() {
            for z in zid.iter() {
                for u in u_ext.iter() {
                    let mut msg = Scout::new(v.clone(), what, *z);
                    msg.exts.user = u.clone();
                    ztesth!(msg, Scout::read);
                }
            }
        }
    }
}

#[test]
fn codec_hello() {
    for _ in 0..NUM_ITER {
        let version = [
            Version {
                stable: gen!(u8),
                experimental: None,
            },
            Version {
                stable: gen!(u8),
                experimental: NonZeroZInt::new(gen!(ZInt)),
            },
        ];
        let wami = [WhatAmI::Client, WhatAmI::Peer, WhatAmI::Router];
        let zid = gen_zid();
        let locators = [
            vec![],
            vec![
                "tcp/1.2.3.4:1234".parse().unwrap(),
                "tcp/5.6.7.8:5678".parse().unwrap(),
            ],
        ];
        let u_ext = [
            None,
            Some(ZExt::new(gen_wireproperties(), ZExtPolicy::Ignore)),
            Some(ZExt::new(gen_wireproperties(), ZExtPolicy::Forward)),
        ];

        for v in version.iter() {
            for w in wami.iter() {
                for l in locators.iter() {
                    for u in u_ext.iter() {
                        let mut msg = Hello::new(v.clone(), *w, zid, l.clone());
                        msg.exts.user = u.clone();
                        ztesth!(msg, Hello::read);
                    }
                }
            }
        }
    }
}

/*************************************/
/*       TRANSPORT MESSAGES          */
/*************************************/
#[test]
fn codec_init() {
    for _ in 0..NUM_ITER {
        let version = [
            Version {
                stable: gen!(u8),
                experimental: None,
            },
            Version {
                stable: gen!(u8),
                experimental: NonZeroZInt::new(gen!(ZInt)),
            },
        ];
        let whatami = [WhatAmI::Router, WhatAmI::Client];
        let sn_bytes = [SeqNumBytes::default(), SeqNumBytes::MIN, SeqNumBytes::MAX];
        let ext_qos = [None, Some(ZExt::new((), ZExtPolicy::Ignore))];
        let ext_auth = [
            None,
            Some(ZExt::new(gen_wireproperties(), ZExtPolicy::Ignore)),
        ];

        for v in version.iter() {
            for w in whatami.iter() {
                for s in sn_bytes.iter() {
                    for q in ext_qos.iter() {
                        for a in ext_auth.iter() {
                            let mut msg = InitSyn::new(v.clone(), *w, gen_zid(), *s);
                            msg.exts.qos = q.clone();
                            msg.exts.authentication = a.clone();
                            ztesth!(msg, InitSyn::read);

                            let mut msg =
                                InitAck::new(v.clone(), *w, gen_zid(), *s, gen_buffer(64).into());
                            msg.exts.qos = q.clone();
                            msg.exts.authentication = a.clone();
                            ztesth!(msg, InitAck::read);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn codec_open() {
    for _ in 0..NUM_ITER {
        let lease = [Duration::from_secs(1), Duration::from_millis(1234)];
        let ext_auth = [
            None,
            Some(ZExt::new(gen_wireproperties(), ZExtPolicy::Ignore)),
        ];

        for l in lease.iter() {
            for a in ext_auth.iter() {
                let mut msg = OpenSyn::new(*l, gen!(ZInt), gen_buffer(64).into());
                msg.exts.authentication = a.clone();
                ztesth!(msg, OpenSyn::read);

                let mut msg = OpenAck::new(*l, gen!(ZInt));
                msg.exts.authentication = a.clone();
                ztesth!(msg, OpenAck::read);
            }
        }
    }
}

#[test]
fn codec_join() {
    for _ in 0..NUM_ITER {
        let version = [
            Version {
                stable: gen!(u8),
                experimental: None,
            },
            Version {
                stable: gen!(u8),
                experimental: NonZeroZInt::new(gen!(ZInt)),
            },
        ];
        let whatami = [WhatAmI::Router, WhatAmI::Client];
        let sn_bytes = [SeqNumBytes::default(), SeqNumBytes::MIN, SeqNumBytes::MAX];
        let lease = [Duration::from_secs(1), Duration::from_millis(1234)];
        let ext_qos = [
            None,
            Some(ZExt::new(
                [gen_initial_sn(); Priority::NUM],
                ZExtPolicy::Ignore,
            )),
        ];
        let ext_auth = [
            None,
            Some(ZExt::new(gen_wireproperties(), ZExtPolicy::Ignore)),
        ];

        for v in version.iter() {
            for w in whatami.iter() {
                for s in sn_bytes.iter() {
                    for l in lease.iter() {
                        for q in ext_qos.iter() {
                            for a in ext_auth.iter() {
                                let mut msg =
                                    Join::new(v.clone(), *w, gen_zid(), *s, *l, gen_initial_sn());
                                msg.exts.qos = q.clone();
                                msg.exts.authentication = a.clone();
                                ztesth!(msg, Join::read);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn codec_close() {
    for _ in 0..NUM_ITER {
        let reason = [
            CloseReason::Generic,
            CloseReason::Unsupported,
            CloseReason::default(),
        ];

        for r in reason.iter() {
            let msg = Close::new(*r);
            ztesth!(msg, Close::read);
        }
    }
}

// #[test]
// fn codec_sync() {
//     for _ in 0..NUM_ITER {
//         let ch = [Reliability::Reliable, Reliability::BestEffort];
//         let count = [None, Some(gen!(ZInt))];
//         let attachment = [None, Some(gen_attachment())];

//         for c in ch.iter() {
//             for n in count.iter() {
//                 for a in attachment.iter() {
//                     let msg = TransportMessage::make_sync(*c, gen!(ZInt), *n, a.clone());
//                     test_write_read_transport_message(msg);
//                 }
//             }
//         }
//     }
// }

// #[test]
// fn codec_ack_nack() {
//     for _ in 0..NUM_ITER {
//         let mask = [None, Some(gen!(ZInt))];
//         let attachment = [None, Some(gen_attachment())];

//         for m in mask.iter() {
//             for a in attachment.iter() {
//                 let msg = TransportMessage::make_ack_nack(gen!(ZInt), *m, a.clone());
//                 test_write_read_transport_message(msg);
//             }
//         }
//     }
// }

#[test]
fn codec_keep_alive() {
    for _ in 0..NUM_ITER {
        let msg = KeepAlive::new();
        ztesth!(msg, KeepAlive::read);
    }
}

#[test]
fn codec_frame() {
    for _ in 0..NUM_ITER {
        let reliabilty = [Reliability::BestEffort, Reliability::Reliable];
        let ext_qos = [
            None,
            Some(ZExt::new(Priority::default(), ZExtPolicy::Ignore)),
            Some(ZExt::new(Priority::RealTime, ZExtPolicy::Ignore)),
            Some(ZExt::new(Priority::Background, ZExtPolicy::Ignore)),
        ];

        let congestion_control = [CongestionControl::Block, CongestionControl::Drop];
        let data_info = [None, Some(gen_data_info())];
        let routing_context = [None, Some(gen_routing_context())];
        let reply_context = [
            None,
            Some(gen_reply_context(false)),
            Some(gen_reply_context(true)),
        ];
        let attachment = [None, Some(gen_attachment())];

        for r in reliabilty.iter() {
            for q in ext_qos.iter() {
                let mut payload = vec![];

                for cc in congestion_control.iter() {
                    for di in data_info.iter() {
                        for rec in reply_context.iter() {
                            for roc in routing_context.iter() {
                                for a in attachment.iter() {
                                    payload.push(ZenohMessage::make_data(
                                        gen_key(),
                                        ZBuf::from(gen_buffer(MAX_PAYLOAD_SIZE)),
                                        Channel {
                                            reliability: *r,
                                            priority: Priority::default(),
                                        },
                                        *cc,
                                        di.clone(),
                                        *roc,
                                        rec.clone(),
                                        a.clone(),
                                    ));
                                }
                            }
                        }
                    }
                }

                let mut msg = Frame::new(*r, gen!(ZInt), payload);
                msg.exts.qos = q.clone();
                ztesth!(msg, Frame::read);
            }
        }
    }
}

#[test]
fn codec_fragment() {
    for _ in 0..NUM_ITER {
        let reliabilty = [Reliability::BestEffort, Reliability::Reliable];
        let has_more = [true, false];
        let ext_qos = [
            None,
            Some(ZExt::new(Priority::default(), ZExtPolicy::Ignore)),
            Some(ZExt::new(Priority::RealTime, ZExtPolicy::Ignore)),
            Some(ZExt::new(Priority::Background, ZExtPolicy::Ignore)),
        ];

        for r in reliabilty.iter() {
            for m in has_more.iter() {
                for q in ext_qos.iter() {
                    let payload = ZSlice::from(gen_buffer(MAX_PAYLOAD_SIZE));
                    let mut msg = Fragment::new(*r, gen!(ZInt), *m, payload);
                    msg.exts.qos = q.clone();
                    ztesth!(msg, Fragment::read);
                }
            }
        }
    }
}

/*************************************/
/*         ZENOH MESSAGES            */
/*************************************/
fn test_write_read_zenoh_message(mut msg: ZenohMessage) {
    let mut buf = WBuf::new(164, false);
    println!("\nWrite message: {:?}", msg);
    buf.write_zenoh_message(&mut msg);
    println!("Read message from: {:?}", buf);
    let mut result = ZBuf::from(buf)
        .read_zenoh_message(msg.channel.reliability)
        .unwrap();
    println!("Message read: {:?}", result);
    if let Some(attachment) = &mut result.attachment {
        let properties = attachment.buffer.read_properties();
        println!("Properties read: {:?}", properties);
    }
    assert_eq!(msg, result);
}

#[test]
fn codec_declare() {
    for _ in 0..NUM_ITER {
        let routing_context = [None, Some(gen_routing_context())];
        let attachment = [None, Some(gen_attachment())];

        for roc in routing_context.iter() {
            for a in attachment.iter() {
                let msg = ZenohMessage::make_declare(gen_declarations(), *roc, a.clone());
                test_write_read_zenoh_message(msg);
            }
        }
    }
}

#[test]
fn codec_data() {
    for _ in 0..NUM_ITER {
        let channel = [
            Channel {
                priority: Priority::default(),
                reliability: Reliability::Reliable,
            },
            Channel {
                priority: Priority::default(),
                reliability: Reliability::BestEffort,
            },
            Channel {
                priority: Priority::RealTime,
                reliability: Reliability::Reliable,
            },
            Channel {
                priority: Priority::RealTime,
                reliability: Reliability::BestEffort,
            },
        ];
        let congestion_control = [CongestionControl::Block, CongestionControl::Drop];
        let data_info = [None, Some(gen_data_info())];
        let routing_context = [None, Some(gen_routing_context())];
        let reply_context = [
            None,
            Some(gen_reply_context(false)),
            Some(gen_reply_context(true)),
        ];
        let attachment = [None, Some(gen_attachment())];

        for ch in channel.iter() {
            for cc in congestion_control.iter() {
                for di in data_info.iter() {
                    for roc in routing_context.iter() {
                        for rec in reply_context.iter() {
                            for a in attachment.iter() {
                                let msg = ZenohMessage::make_data(
                                    gen_key(),
                                    ZBuf::from(gen_buffer(MAX_PAYLOAD_SIZE)),
                                    *ch,
                                    *cc,
                                    di.clone(),
                                    *roc,
                                    rec.clone(),
                                    a.clone(),
                                );
                                test_write_read_zenoh_message(msg);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn codec_unit() {
    for _ in 0..NUM_ITER {
        let channel = [
            Channel {
                priority: Priority::default(),
                reliability: Reliability::Reliable,
            },
            Channel {
                priority: Priority::default(),
                reliability: Reliability::BestEffort,
            },
            Channel {
                priority: Priority::RealTime,
                reliability: Reliability::Reliable,
            },
            Channel {
                priority: Priority::RealTime,
                reliability: Reliability::BestEffort,
            },
        ];
        let congestion_control = [CongestionControl::Block, CongestionControl::Drop];
        let reply_context = [
            None,
            Some(gen_reply_context(false)),
            Some(gen_reply_context(true)),
        ];
        let attachment = [None, Some(gen_attachment())];

        for ch in channel.iter() {
            for cc in congestion_control.iter() {
                for rc in reply_context.iter() {
                    for a in attachment.iter() {
                        let msg = ZenohMessage::make_unit(*ch, *cc, rc.clone(), a.clone());
                        test_write_read_zenoh_message(msg);
                    }
                }
            }
        }
    }
}

#[test]
fn codec_pull() {
    for _ in 0..NUM_ITER {
        let max_samples = [None, Some(gen!(ZInt))];
        let attachment = [None, Some(gen_attachment())];
        let is_final = [false, true];

        for f in is_final.iter() {
            for m in max_samples.iter() {
                for a in attachment.iter() {
                    let msg = ZenohMessage::make_pull(*f, gen_key(), gen!(ZInt), *m, a.clone());
                    test_write_read_zenoh_message(msg);
                }
            }
        }
    }
}

#[test]
fn codec_query() {
    for _ in 0..NUM_ITER {
        let value_selector = [String::default(), "my_value_selector".to_string()];
        let target = [None, Some(gen_query_target())];
        let routing_context = [None, Some(gen_routing_context())];
        let attachment = [None, Some(gen_attachment())];

        for p in value_selector.iter() {
            for t in target.iter() {
                for roc in routing_context.iter() {
                    for a in attachment.iter() {
                        let msg = ZenohMessage::make_query(
                            gen_key(),
                            p.clone(),
                            gen!(ZInt),
                            t.clone(),
                            gen_consolidation(),
                            *roc,
                            a.clone(),
                        );
                        test_write_read_zenoh_message(msg);
                    }
                }
            }
        }
    }
}
