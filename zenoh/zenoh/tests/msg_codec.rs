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
use zenoh::net::protocol::core::{whatami::WhatAmIMatcher, *};
use zenoh::net::protocol::io::{WBuf, ZBuf};
use zenoh::net::protocol::proto::defaults::SEQ_NUM_RES;
use zenoh::net::protocol::proto::*;
use zenoh_protocol::io::{WBufCodec, ZBufCodec};

const NUM_ITER: usize = 100;
const PROPS_LENGTH: usize = 3;
const PROP_MAX_SIZE: usize = 64;
const MAX_PAYLOAD_SIZE: usize = 256;

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

fn gen_pid() -> PeerId {
    PeerId::from(uuid::Uuid::new_v4())
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
            id: gen_pid(),
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
        source_id: option_gen!(gen_pid()),
        source_sn: option_gen!(gen!(ZInt)),
        first_router_id: option_gen!(gen_pid()),
        first_router_sn: option_gen!(gen!(ZInt)),
    }
}

fn gen_initial_sn() -> ConduitSn {
    ConduitSn {
        reliable: gen!(ZInt),
        best_effort: gen!(ZInt),
    }
}

fn test_write_read_transport_message(mut msg: TransportMessage) {
    let mut buf = WBuf::new(164, false);
    println!("\nWrite message: {:?}", msg);
    buf.write_transport_message(&mut msg);
    println!("Read message from: {:?}", buf);
    let mut result = ZBuf::from(buf).read_transport_message().unwrap();
    println!("Message read: {:?}", result);
    if let Some(attachment) = result.attachment.as_mut() {
        let properties = attachment.buffer.read_properties();
        println!("Properties read: {:?}", properties);
    }
    assert_eq!(msg, result);
}

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

/*************************************/
/*       TRANSPORTMESSAGES           */
/*************************************/

#[test]
fn codec_scout() {
    for _ in 0..NUM_ITER {
        let wami = [None, Some(gen!(ZInt))];
        let pid_req = [true, false];
        let attachment = [None, Some(gen_attachment())];

        for w in wami.iter() {
            for p in pid_req.iter() {
                for a in attachment.iter() {
                    let msg = TransportMessage::make_scout(
                        w.map(WhatAmIMatcher::try_from).flatten(),
                        *p,
                        a.clone(),
                    );
                    test_write_read_transport_message(msg);
                }
            }
        }
    }
}

#[test]
fn codec_hello() {
    for _ in 0..NUM_ITER {
        let pid = [None, Some(gen_pid())];
        let wami = [None, Some(gen!(ZInt))];
        let locators = [
            None,
            Some(vec![
                "tcp/1.2.3.4:1234".parse().unwrap(),
                "tcp/5.6.7.8:5678".parse().unwrap(),
            ]),
        ];
        let attachment = [None, Some(gen_attachment())];

        for p in pid.iter() {
            for w in wami.iter() {
                for l in locators.iter() {
                    for a in attachment.iter() {
                        let msg = TransportMessage::make_hello(
                            *p,
                            w.map(WhatAmI::try_from).flatten(),
                            l.clone(),
                            a.clone(),
                        );
                        test_write_read_transport_message(msg);
                    }
                }
            }
        }
    }
}

#[test]
fn codec_init() {
    for _ in 0..NUM_ITER {
        let is_qos = [true, false];
        let wami = [WhatAmI::Router, WhatAmI::Client];
        let sn_resolution = [SEQ_NUM_RES, gen!(ZInt)];
        let attachment = [None, Some(gen_attachment())];

        for q in is_qos.iter() {
            for w in wami.iter() {
                for s in sn_resolution.iter() {
                    for a in attachment.iter() {
                        let msg = TransportMessage::make_init_syn(
                            gen!(u8),
                            *w,
                            gen_pid(),
                            *s,
                            *q,
                            a.clone(),
                        );
                        test_write_read_transport_message(msg);
                    }
                }
            }
        }

        let sn_resolution = [None, Some(gen!(ZInt))];
        for q in is_qos.iter() {
            for w in wami.iter() {
                for s in sn_resolution.iter() {
                    for a in attachment.iter() {
                        let msg = TransportMessage::make_init_ack(
                            *w,
                            gen_pid(),
                            *s,
                            *q,
                            gen_buffer(64).into(),
                            a.clone(),
                        );
                        test_write_read_transport_message(msg);
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
        let attachment = [None, Some(gen_attachment())];

        for l in lease.iter() {
            for a in attachment.iter() {
                let msg = TransportMessage::make_open_syn(
                    *l,
                    gen!(ZInt),
                    gen_buffer(64).into(),
                    a.clone(),
                );
                test_write_read_transport_message(msg);
            }
        }

        for l in lease.iter() {
            for a in attachment.iter() {
                let msg = TransportMessage::make_open_ack(*l, gen!(ZInt), a.clone());
                test_write_read_transport_message(msg);
            }
        }
    }
}

#[test]
fn codec_join() {
    for _ in 0..NUM_ITER {
        let lease = [Duration::from_secs(1), Duration::from_millis(1234)];
        let wami = [WhatAmI::Router, WhatAmI::Client];
        let sn_resolution = [SEQ_NUM_RES, gen!(ZInt)];
        let initial_sns = [
            ConduitSnList::Plain(gen_initial_sn()),
            ConduitSnList::QoS(Box::new([gen_initial_sn(); Priority::NUM])),
        ];
        let attachment = [None, Some(gen_attachment())];

        for l in lease.iter() {
            for w in wami.iter() {
                for s in sn_resolution.iter() {
                    for i in initial_sns.iter() {
                        for a in attachment.iter() {
                            let msg = TransportMessage::make_join(
                                gen!(u8),
                                *w,
                                gen_pid(),
                                *l,
                                *s,
                                i.clone(),
                                a.clone(),
                            );
                            test_write_read_transport_message(msg);
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
        let pid = [None, Some(gen_pid())];
        let link_only = [true, false];
        let attachment = [None, Some(gen_attachment())];

        for p in pid.iter() {
            for k in link_only.iter() {
                for a in attachment.iter() {
                    let msg = TransportMessage::make_close(*p, gen!(u8), *k, a.clone());
                    test_write_read_transport_message(msg);
                }
            }
        }
    }
}

#[test]
fn codec_sync() {
    for _ in 0..NUM_ITER {
        let ch = [Reliability::Reliable, Reliability::BestEffort];
        let count = [None, Some(gen!(ZInt))];
        let attachment = [None, Some(gen_attachment())];

        for c in ch.iter() {
            for n in count.iter() {
                for a in attachment.iter() {
                    let msg = TransportMessage::make_sync(*c, gen!(ZInt), *n, a.clone());
                    test_write_read_transport_message(msg);
                }
            }
        }
    }
}

#[test]
fn codec_ack_nack() {
    for _ in 0..NUM_ITER {
        let mask = [None, Some(gen!(ZInt))];
        let attachment = [None, Some(gen_attachment())];

        for m in mask.iter() {
            for a in attachment.iter() {
                let msg = TransportMessage::make_ack_nack(gen!(ZInt), *m, a.clone());
                test_write_read_transport_message(msg);
            }
        }
    }
}

#[test]
fn codec_keep_alive() {
    for _ in 0..NUM_ITER {
        let pid = [None, Some(gen_pid())];
        let attachment = [None, Some(gen_attachment())];

        for p in pid.iter() {
            for a in attachment.iter() {
                let msg = TransportMessage::make_keep_alive(*p, a.clone());
                test_write_read_transport_message(msg);
            }
        }
    }
}

#[test]
fn codec_ping() {
    for _ in 0..NUM_ITER {
        let attachment = [None, Some(gen_attachment())];

        for a in attachment.iter() {
            let msg = TransportMessage::make_ping(gen!(ZInt), a.clone());
            test_write_read_transport_message(msg);
        }
    }
}

#[test]
fn codec_pong() {
    for _ in 0..NUM_ITER {
        let attachment = [None, Some(gen_attachment())];

        for a in attachment.iter() {
            let msg = TransportMessage::make_pong(gen!(ZInt), a.clone());
            test_write_read_transport_message(msg);
        }
    }
}

#[test]
fn codec_frame() {
    let msg_payload_count = 4;

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
                let mut payload = vec![
                    FramePayload::Fragment {
                        buffer: gen_buffer(MAX_PAYLOAD_SIZE).into(),
                        is_final: false,
                    },
                    FramePayload::Fragment {
                        buffer: gen_buffer(MAX_PAYLOAD_SIZE).into(),
                        is_final: true,
                    },
                ];

                for di in data_info.iter() {
                    for rec in reply_context.iter() {
                        for roc in routing_context.iter() {
                            for a in attachment.iter() {
                                payload.push(FramePayload::Messages {
                                    messages: vec![
                                        ZenohMessage::make_data(
                                            gen_key(),
                                            ZBuf::from(gen_buffer(MAX_PAYLOAD_SIZE)),
                                            *ch,
                                            *cc,
                                            di.clone(),
                                            *roc,
                                            rec.clone(),
                                            a.clone(),
                                        );
                                        msg_payload_count
                                    ],
                                });
                            }
                        }
                    }
                }

                for p in payload.drain(..) {
                    for a in attachment.iter() {
                        let msg =
                            TransportMessage::make_frame(*ch, gen!(ZInt), p.clone(), a.clone());
                        test_write_read_transport_message(msg);
                    }
                }
            }
        }
    }
}

#[test]
fn codec_frame_batching() {
    for _ in 0..NUM_ITER {
        // Contigous batch
        let mut wbuf = WBuf::new(64, true);
        // Written messages
        let mut written: Vec<TransportMessage> = vec![];

        // Create empty frame message
        let channel = Channel::default();
        let congestion_control = CongestionControl::default();
        let payload = FramePayload::Messages { messages: vec![] };
        let sn = gen!(ZInt);
        let sattachment = None;
        let mut frame = TransportMessage::make_frame(channel, sn, payload, sattachment.clone());

        // Write the first frame header
        assert!(wbuf.write_transport_message(&mut frame));

        // Create data message
        let key = "test".into();
        let payload = ZBuf::from(vec![0_u8; 1]);
        let data_info = None;
        let routing_context = None;
        let reply_context = None;
        let zattachment = None;
        let mut data = ZenohMessage::make_data(
            key,
            payload,
            channel,
            congestion_control,
            data_info,
            routing_context,
            reply_context,
            zattachment,
        );

        // Write the first data message
        assert!(wbuf.write_zenoh_message(&mut data));

        // Store the first transport message written
        let payload = FramePayload::Messages {
            messages: vec![data.clone(); 1],
        };
        written.push(TransportMessage::make_frame(
            channel,
            sn,
            payload,
            sattachment.clone(),
        ));

        // Write the second frame header
        assert!(wbuf.write_transport_message(&mut frame));

        // Write until we fill the batch
        let mut messages: Vec<ZenohMessage> = vec![];
        loop {
            wbuf.mark();
            if wbuf.write_zenoh_message(&mut data) {
                messages.push(data.clone());
            } else {
                wbuf.revert();
                break;
            }
        }

        // Store the second transport message written
        let payload = FramePayload::Messages { messages };
        written.push(TransportMessage::make_frame(
            channel,
            sn,
            payload,
            sattachment,
        ));

        // Deserialize from the buffer
        let mut zbuf = ZBuf::from(wbuf);

        let mut read: Vec<TransportMessage> = vec![];
        while let Some(msg) = zbuf.read_transport_message() {
            read.push(msg);
        }

        assert_eq!(written, read);
    }
}

/*************************************/
/*         ZENOH MESSAGES            */
/*************************************/

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
