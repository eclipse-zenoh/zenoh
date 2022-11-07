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
use rand::*;
use std::convert::{TryFrom, TryInto};
use std::time::Duration;
use uhlc::Timestamp;
use zenoh_buffers::reader::HasReader;
use zenoh_buffers::writer::{BacktrackableWriter, HasWriter};
use zenoh_protocol::io::{WBuf, ZBuf};
use zenoh_protocol::io::{WBufCodec, ZBufCodec};
use zenoh_protocol::proto::defaults::SEQ_NUM_RES;
use zenoh_protocol::proto::{
    Attachment, DataInfo, Declaration, ForgetPublisher, ForgetQueryable, ForgetResource,
    ForgetSubscriber, FramePayload, MessageReader, MessageWriter, Publisher, QueryBody, Queryable,
    ReplierInfo, ReplyContext, Resource, RoutingContext, Subscriber, TransportMessage,
    ZenohMessage,
};
use zenoh_protocol_core::{whatami::WhatAmIMatcher, *};

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
        Some(ReplierInfo { id: gen_zid() })
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
            },
        }),
        Declaration::Subscriber(Subscriber {
            key: gen_key(),
            info: SubInfo {
                reliability: Reliability::BestEffort,
                mode: SubMode::Pull,
            },
        }),
        Declaration::Subscriber(Subscriber {
            key: gen_key(),
            info: SubInfo {
                reliability: Reliability::Reliable,
                mode: SubMode::Pull,
            },
        }),
        Declaration::Subscriber(Subscriber {
            key: gen_key(),
            info: SubInfo {
                reliability: Reliability::BestEffort,
                mode: SubMode::Push,
            },
        }),
        Declaration::ForgetSubscriber(ForgetSubscriber { key: gen_key() }),
        Declaration::Queryable(Queryable {
            key: gen_key(),
            info: QueryableInfo {
                complete: 1,
                distance: 0,
            },
        }),
        Declaration::Queryable(Queryable {
            key: gen_key(),
            info: QueryableInfo {
                complete: 0,
                distance: 10,
            },
        }),
        Declaration::Queryable(Queryable {
            key: gen_key(),
            info: QueryableInfo {
                complete: 10,
                distance: 0,
            },
        }),
        Declaration::ForgetQueryable(ForgetQueryable { key: gen_key() }),
        Declaration::ForgetQueryable(ForgetQueryable { key: gen_key() }),
        Declaration::ForgetQueryable(ForgetQueryable { key: gen_key() }),
    ]
}

fn gen_key() -> WireExpr<'static> {
    let key = [
        WireExpr::from(gen!(ZInt)),
        WireExpr::from("my_resource"),
        WireExpr::from(gen!(ZInt)).with_suffix("my_resource"),
    ];
    key[thread_rng().gen_range(0..key.len())].clone()
}

fn gen_query_target() -> QueryTarget {
    let tgt = [
        QueryTarget::BestMatching,
        QueryTarget::All,
        QueryTarget::AllComplete,
        #[cfg(feature = "complete_n")]
        QueryTarget::Complete(3),
    ];
    tgt[thread_rng().gen_range(0..tgt.len())]
}

fn gen_consolidation_mode() -> ConsolidationMode {
    let cm = [
        ConsolidationMode::None,
        ConsolidationMode::Monotonic,
        ConsolidationMode::Latest,
    ];
    cm[thread_rng().gen_range(0..cm.len())]
}

fn gen_timestamp() -> Timestamp {
    Timestamp::new(uhlc::NTP64(gen!(u64)), uhlc::ID::from(uuid::Uuid::new_v4()))
}

fn gen_data_info() -> DataInfo {
    DataInfo {
        kind: (gen!(ZInt) % 2).try_into().unwrap(),
        encoding: option_gen!(Encoding::Exact(TryFrom::try_from(gen!(u8) % 21).unwrap())),
        timestamp: option_gen!(gen_timestamp()),
        #[cfg(feature = "shared-memory")]
        sliced: false,
        source_id: option_gen!(gen_zid()),
        source_sn: option_gen!(gen!(ZInt)),
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
    let mut result = ZBuf::from(buf).reader().read_transport_message().unwrap();
    println!("Message read: {:?}", result);
    if let Some(attachment) = result.attachment.as_mut() {
        let properties = attachment.buffer.reader().read_properties();
        println!("Properties read: {:?}", properties);
    }
    assert_eq!(msg, result);
}

fn test_write_read_zenoh_message(mut msg: ZenohMessage) {
    let mut buf = WBuf::new(164, false);
    println!("\nWrite message: {:?}", msg);
    assert!(buf.write_zenoh_message(&mut msg));
    println!("Read message from: {:?}", buf);
    let buf = ZBuf::from(buf);
    println!("Converted into   : {:?}", buf);
    let mut result = buf
        .reader()
        .read_zenoh_message(msg.channel.reliability)
        .unwrap();
    println!("Message read: {:?}", result);
    if let Some(attachment) = &mut result.attachment {
        let properties = attachment.buffer.reader().read_properties();
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
        let zid_req = [true, false];
        let attachment = [None, Some(gen_attachment())];

        for w in wami.iter() {
            for p in zid_req.iter() {
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
        let zid = [None, Some(gen_zid())];
        let wami = [None, Some(gen!(ZInt))];
        let locators = [
            None,
            Some(vec![
                "tcp/1.2.3.4:1234".parse().unwrap(),
                "tcp/5.6.7.8:5678".parse().unwrap(),
            ]),
        ];
        let attachment = [None, Some(gen_attachment())];

        for p in zid.iter() {
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
                            gen_zid(),
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
                            gen_zid(),
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
                                gen_zid(),
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
        let zid = [None, Some(gen_zid())];
        let link_only = [true, false];
        let attachment = [None, Some(gen_attachment())];

        for p in zid.iter() {
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
        let zid = [None, Some(gen_zid())];
        let attachment = [None, Some(gen_attachment())];

        for p in zid.iter() {
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
        let mut wbuf = WBuf::new(64, true).writer();
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
        assert!(wbuf.as_mut().write_transport_message(&mut frame));

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
        assert!(wbuf.as_mut().write_zenoh_message(&mut data));

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
        assert!(wbuf.as_mut().write_transport_message(&mut frame));

        // Write until we fill the batch
        let mut messages: Vec<ZenohMessage> = vec![];
        loop {
            wbuf.mark();
            if wbuf.as_mut().write_zenoh_message(&mut data) {
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
        let zbuf = ZBuf::from(wbuf.into_inner());
        let mut zbuf = zbuf.reader();

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
        let parameters = [String::default(), "my_params".to_string()];
        let target = [None, Some(gen_query_target())];
        let routing_context = [None, Some(gen_routing_context())];
        let attachment = [None, Some(gen_attachment())];
        let body = [
            None,
            Some(QueryBody {
                data_info: gen_data_info(),
                payload: ZBuf::from(gen_buffer(MAX_PAYLOAD_SIZE)),
            }),
        ];

        for p in parameters.iter() {
            for t in target.iter() {
                for roc in routing_context.iter() {
                    for a in attachment.iter() {
                        for b in body.iter() {
                            let msg = ZenohMessage::make_query(
                                gen_key(),
                                p.clone(),
                                gen!(ZInt),
                                *t,
                                gen_consolidation_mode(),
                                b.clone(),
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
}
