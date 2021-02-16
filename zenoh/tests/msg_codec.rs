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
use uhlc::Timestamp;
use zenoh::net::protocol::core::*;
use zenoh::net::protocol::io::{RBuf, WBuf};
use zenoh::net::protocol::proto::*;

const NUM_ITER: usize = 100;
const PROPS_LENGTH: usize = 3;
const PROP_MAX_SIZE: usize = 64;
const MAX_PAYLOAD_SIZE: usize = 256;

macro_rules! gen {
    ($name:ty) => {
        thread_rng().gen::<$name>()
    };
}

macro_rules! option_gen {
    ($e:expr) => {
        if thread_rng().gen_bool(0.5) {
            Some($e)
        } else {
            None
        }
    };
}

fn gen_buffer(max_size: usize) -> Vec<u8> {
    let len: usize = thread_rng().gen_range(1..max_size + 1);
    let mut buf: Vec<u8> = Vec::with_capacity(len);
    buf.resize(len, 0);
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
    gen!(ZInt)
}

fn gen_reply_context(is_final: bool) -> ReplyContext {
    let qid = gen!(ZInt);
    let source_kind: ZInt = thread_rng().gen_range(0..4);
    let replier_id = if is_final { None } else { Some(gen_pid()) };
    ReplyContext::make(qid, source_kind, replier_id)
}

fn gen_attachment() -> Attachment {
    let mut wbuf = WBuf::new(PROP_MAX_SIZE, false);
    let props = gen_props(PROPS_LENGTH, PROP_MAX_SIZE);
    wbuf.write_properties(&props);

    let rbuf = RBuf::from(&wbuf);
    Attachment::make(0, rbuf)
}

fn gen_declarations() -> Vec<Declaration> {
    use zenoh::net::protocol::proto::Declaration::*;
    let mut decls = Vec::new();
    decls.push(Resource {
        rid: gen!(ZInt),
        key: gen_key(),
    });
    decls.push(ForgetResource { rid: gen!(ZInt) });
    decls.push(Publisher { key: gen_key() });
    decls.push(ForgetPublisher { key: gen_key() });
    decls.push(Subscriber {
        key: gen_key(),
        info: SubInfo {
            reliability: Reliability::Reliable,
            mode: SubMode::Push,
            period: None,
        },
    });
    decls.push(Subscriber {
        key: gen_key(),
        info: SubInfo {
            reliability: Reliability::BestEffort,
            mode: SubMode::Pull,
            period: None,
        },
    });
    decls.push(Subscriber {
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
    });
    decls.push(Subscriber {
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
    });
    decls.push(ForgetSubscriber { key: gen_key() });
    decls.push(Queryable { key: gen_key() });
    decls.push(ForgetQueryable { key: gen_key() });
    decls
}

fn gen_key() -> ResKey {
    let num: u8 = thread_rng().gen_range(0..3);
    match num {
        0 => ResKey::from(gen!(ZInt)),
        1 => ResKey::from("my_resource".to_string()),
        _ => ResKey::from((gen!(ZInt), "my_resource".to_string())),
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
        1 => Target::Complete { n: 3 },
        2 => Target::All,
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
        source_id: option_gen!(gen_pid()),
        source_sn: option_gen!(gen!(ZInt)),
        first_router_id: option_gen!(gen_pid()),
        first_router_sn: option_gen!(gen!(ZInt)),
        timestamp: option_gen!(gen_timestamp()),
        kind: option_gen!(gen!(ZInt)),
        encoding: option_gen!(gen!(ZInt)),
    }
}

fn test_write_read_session_message(msg: SessionMessage) {
    let mut buf = WBuf::new(164, false);
    println!("\nWrite message: {:?}", msg);
    buf.write_session_message(&msg);
    println!("Read message from: {:?}", buf);
    let mut result = RBuf::from(&buf).read_session_message().unwrap();
    println!("Message read: {:?}", result);
    if let Some(attachment) = result.get_attachment_mut() {
        let properties = attachment.buffer.read_properties();
        println!("Properties read: {:?}", properties);
    }

    assert_eq!(msg, result);
}

fn test_write_read_zenoh_message(msg: ZenohMessage) {
    let mut buf = WBuf::new(164, false);
    println!("\nWrite message: {:?}", msg);
    buf.write_zenoh_message(&msg);
    println!("Read message from: {:?}", buf);
    let mut result = RBuf::from(&buf)
        .read_zenoh_message(msg.reliability)
        .unwrap();
    println!("Message read: {:?}", result);
    if let Some(attachment) = &mut result.attachment {
        let properties = attachment.buffer.read_properties();
        println!("Properties read: {:?}", properties);
    }

    assert_eq!(msg, result);
}

/*************************************/
/*        SESSION MESSAGES           */
/*************************************/

#[test]
fn scout_tests() {
    for _ in 0..NUM_ITER {
        let wami = [None, Some(gen!(ZInt))];
        let pid_req = [true, false];
        let attachment = [None, Some(gen_attachment())];

        for w in wami.iter() {
            for p in pid_req.iter() {
                for a in attachment.iter() {
                    let msg = SessionMessage::make_scout(w.clone(), *p, a.clone());
                    test_write_read_session_message(msg);
                }
            }
        }
    }
}

#[test]
fn hello_tests() {
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
                        let msg =
                            SessionMessage::make_hello(p.clone(), w.clone(), l.clone(), a.clone());
                        test_write_read_session_message(msg);
                    }
                }
            }
        }
    }
}

#[test]
fn init_tests() {
    for _ in 0..NUM_ITER {
        let wami = [whatami::ROUTER, whatami::CLIENT];
        let sn_resolution = [None, Some(gen!(ZInt))];
        let attachment = [None, Some(gen_attachment())];

        for w in wami.iter() {
            for s in sn_resolution.iter() {
                for a in attachment.iter() {
                    let msg = SessionMessage::make_init_syn(gen!(u8), *w, gen_pid(), *s, a.clone());
                    test_write_read_session_message(msg);
                }
            }
        }

        for w in wami.iter() {
            for s in sn_resolution.iter() {
                for a in attachment.iter() {
                    let msg = SessionMessage::make_init_ack(
                        *w,
                        gen_pid(),
                        *s,
                        RBuf::from(gen_buffer(64)),
                        a.clone(),
                    );
                    test_write_read_session_message(msg);
                }
            }
        }
    }
}

#[test]
fn open_tests() {
    for _ in 0..NUM_ITER {
        let attachment = [None, Some(gen_attachment())];

        for a in attachment.iter() {
            let msg = SessionMessage::make_open_syn(
                gen!(ZInt),
                gen!(ZInt),
                RBuf::from(gen_buffer(64)),
                a.clone(),
            );
            test_write_read_session_message(msg);
        }

        for a in attachment.iter() {
            let msg = SessionMessage::make_open_ack(gen!(ZInt), gen!(ZInt), a.clone());
            test_write_read_session_message(msg);
        }
    }
}

#[test]
fn close_tests() {
    for _ in 0..NUM_ITER {
        let pid = [None, Some(gen_pid())];
        let link_only = [true, false];
        let attachment = [None, Some(gen_attachment())];

        for p in pid.iter() {
            for k in link_only.iter() {
                for a in attachment.iter() {
                    let msg = SessionMessage::make_close(p.clone(), gen!(u8), *k, a.clone());
                    test_write_read_session_message(msg);
                }
            }
        }
    }
}

#[test]
fn sync_tests() {
    for _ in 0..NUM_ITER {
        let ch = [Channel::Reliable, Channel::BestEffort];
        let count = [None, Some(gen!(ZInt))];
        let attachment = [None, Some(gen_attachment())];

        for c in ch.iter() {
            for n in count.iter() {
                for a in attachment.iter() {
                    let msg = SessionMessage::make_sync(*c, gen!(ZInt), n.clone(), a.clone());
                    test_write_read_session_message(msg);
                }
            }
        }
    }
}

#[test]
fn ack_nack_tests() {
    for _ in 0..NUM_ITER {
        let mask = [None, Some(gen!(ZInt))];
        let attachment = [None, Some(gen_attachment())];

        for m in mask.iter() {
            for a in attachment.iter() {
                let msg = SessionMessage::make_ack_nack(gen!(ZInt), m.clone(), a.clone());
                test_write_read_session_message(msg);
            }
        }
    }
}

#[test]
fn keep_alive_tests() {
    for _ in 0..NUM_ITER {
        let pid = [None, Some(gen_pid())];
        let attachment = [None, Some(gen_attachment())];

        for p in pid.iter() {
            for a in attachment.iter() {
                let msg = SessionMessage::make_keep_alive(p.clone(), a.clone());
                test_write_read_session_message(msg);
            }
        }
    }
}

#[test]
fn ping_tests() {
    for _ in 0..NUM_ITER {
        let attachment = [None, Some(gen_attachment())];

        for a in attachment.iter() {
            let msg = SessionMessage::make_ping(gen!(ZInt), a.clone());
            test_write_read_session_message(msg);
        }
    }
}

#[test]
fn pong_tests() {
    for _ in 0..NUM_ITER {
        let attachment = [None, Some(gen_attachment())];

        for a in attachment.iter() {
            let msg = SessionMessage::make_pong(gen!(ZInt), a.clone());
            test_write_read_session_message(msg);
        }
    }
}

#[test]
fn frame_tests() {
    let msg_payload_count = 4;

    for _ in 0..NUM_ITER {
        let ch = [Channel::Reliable, Channel::BestEffort];
        let congestion_control = [CongestionControl::Block, CongestionControl::Drop];
        let data_info = [None, Some(gen_data_info())];
        let routing_context = [None, Some(gen_routing_context())];
        let reply_context = [
            None,
            Some(gen_reply_context(false)),
            Some(gen_reply_context(true)),
        ];
        let attachment = [None, Some(gen_attachment())];

        let mut payload = Vec::new();
        payload.push(FramePayload::Fragment {
            buffer: RBuf::from(gen_buffer(MAX_PAYLOAD_SIZE)),
            is_final: false,
        });
        payload.push(FramePayload::Fragment {
            buffer: RBuf::from(gen_buffer(MAX_PAYLOAD_SIZE)),
            is_final: true,
        });
        for cc in congestion_control.iter() {
            for di in data_info.iter() {
                for rec in reply_context.iter() {
                    for roc in routing_context.iter() {
                        for a in attachment.iter() {
                            payload.push(FramePayload::Messages {
                                messages: vec![
                                    ZenohMessage::make_data(
                                        gen_key(),
                                        RBuf::from(gen_buffer(MAX_PAYLOAD_SIZE)),
                                        Reliability::BestEffort,
                                        *cc,
                                        di.clone(),
                                        roc.clone(),
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
        }

        for c in ch.iter() {
            for mut p in payload.drain(..) {
                for a in attachment.iter() {
                    // Put in accordance ZenohMessage reliability and Session channel
                    match p {
                        FramePayload::Fragment { .. } => {}
                        FramePayload::Messages { ref mut messages } => {
                            for m in messages.iter_mut() {
                                match c {
                                    Channel::Reliable => m.reliability = Reliability::Reliable,
                                    Channel::BestEffort => m.reliability = Reliability::BestEffort,
                                }
                            }
                        }
                    }
                    let msg = SessionMessage::make_frame(*c, gen!(ZInt), p.clone(), a.clone());
                    test_write_read_session_message(msg);
                }
            }
        }
    }
}

#[test]
fn frame_batching_tests() {
    for _ in 0..NUM_ITER {
        // Contigous batch
        let mut wbuf = WBuf::new(64, true);
        // Written messages
        let mut written: Vec<SessionMessage> = Vec::new();

        // Create empty frame message
        let ch = Channel::Reliable;
        let payload = FramePayload::Messages { messages: vec![] };
        let sn = gen!(ZInt);
        let sattachment = None;
        let frame = SessionMessage::make_frame(ch, sn, payload, sattachment.clone());

        // Write the first frame header
        assert!(wbuf.write_session_message(&frame));

        // Create data message
        let key = ResKey::RName("test".to_string());
        let payload = RBuf::from(vec![0u8; 1]);
        let reliability = Reliability::Reliable;
        let congestion_control = CongestionControl::Block;
        let data_info = None;
        let routing_context = None;
        let reply_context = None;
        let zattachment = None;
        let data = ZenohMessage::make_data(
            key,
            payload,
            reliability,
            congestion_control,
            data_info,
            routing_context,
            reply_context,
            zattachment,
        );

        // Write the first data message
        assert!(wbuf.write_zenoh_message(&data));

        // Store the first session message written
        let payload = FramePayload::Messages {
            messages: vec![data.clone(); 1],
        };
        written.push(SessionMessage::make_frame(
            ch,
            sn,
            payload,
            sattachment.clone(),
        ));

        // Write the second frame header
        assert!(wbuf.write_session_message(&frame));

        // Write until we fill the batch
        let mut messages: Vec<ZenohMessage> = Vec::new();
        loop {
            wbuf.mark();
            if wbuf.write_zenoh_message(&data) {
                messages.push(data.clone());
            } else {
                wbuf.revert();
                break;
            }
        }

        // Store the second session message written
        let payload = FramePayload::Messages { messages };
        written.push(SessionMessage::make_frame(ch, sn, payload, sattachment));

        // Deserialize from the buffer
        let mut rbuf = RBuf::from(&wbuf);

        let mut read: Vec<SessionMessage> = Vec::new();
        loop {
            let pos = rbuf.get_pos();
            match rbuf.read_session_message() {
                Some(msg) => read.push(msg),
                None => {
                    assert!(rbuf.set_pos(pos));
                    break;
                }
            }
        }

        assert_eq!(written, read);
    }
}

/*************************************/
/*         ZENOH MESSAGES            */
/*************************************/

#[test]
fn declare_tests() {
    for _ in 0..NUM_ITER {
        let routing_context = [None, Some(gen_routing_context())];
        let attachment = [None, Some(gen_attachment())];

        for roc in routing_context.iter() {
            for a in attachment.iter() {
                let msg = ZenohMessage::make_declare(gen_declarations(), roc.clone(), a.clone());
                test_write_read_zenoh_message(msg);
            }
        }
    }
}

#[test]
fn data_tests() {
    for _ in 0..NUM_ITER {
        let reliability = [Reliability::Reliable, Reliability::BestEffort];
        let congestion_control = [CongestionControl::Block, CongestionControl::Drop];
        let data_info = [None, Some(gen_data_info())];
        let routing_context = [None, Some(gen_routing_context())];
        let reply_context = [
            None,
            Some(gen_reply_context(false)),
            Some(gen_reply_context(true)),
        ];
        let attachment = [None, Some(gen_attachment())];

        for rl in reliability.iter() {
            for cc in congestion_control.iter() {
                for di in data_info.iter() {
                    for roc in routing_context.iter() {
                        for rec in reply_context.iter() {
                            for a in attachment.iter() {
                                let msg = ZenohMessage::make_data(
                                    gen_key(),
                                    RBuf::from(gen_buffer(MAX_PAYLOAD_SIZE)),
                                    *rl,
                                    *cc,
                                    di.clone(),
                                    roc.clone(),
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
fn unit_tests() {
    for _ in 0..NUM_ITER {
        let reliability = [Reliability::Reliable, Reliability::BestEffort];
        let congestion_control = [CongestionControl::Block, CongestionControl::Drop];
        let reply_context = [
            None,
            Some(gen_reply_context(false)),
            Some(gen_reply_context(true)),
        ];
        let attachment = [None, Some(gen_attachment())];

        for rl in reliability.iter() {
            for cc in congestion_control.iter() {
                for rc in reply_context.iter() {
                    for a in attachment.iter() {
                        let msg = ZenohMessage::make_unit(*rl, *cc, rc.clone(), a.clone());
                        test_write_read_zenoh_message(msg);
                    }
                }
            }
        }
    }
}

#[test]
fn pull_tests() {
    for _ in 0..NUM_ITER {
        let is_final = [false, true];
        let max_samples = [None, Some(gen!(ZInt))];
        let attachment = [None, Some(gen_attachment())];

        for f in is_final.iter() {
            for m in max_samples.iter() {
                for a in attachment.iter() {
                    let msg =
                        ZenohMessage::make_pull(*f, gen_key(), gen!(ZInt), m.clone(), a.clone());
                    test_write_read_zenoh_message(msg);
                }
            }
        }
    }
}

#[test]
fn query_tests() {
    for _ in 0..NUM_ITER {
        let predicate = [String::default(), "my_predicate".to_string()];
        let target = [None, Some(gen_query_target())];
        let routing_context = [None, Some(gen_routing_context())];
        let attachment = [None, Some(gen_attachment())];

        for p in predicate.iter() {
            for t in target.iter() {
                for roc in routing_context.iter() {
                    for a in attachment.iter() {
                        let msg = ZenohMessage::make_query(
                            gen_key(),
                            p.clone(),
                            gen!(ZInt),
                            t.clone(),
                            gen_consolidation(),
                            roc.clone(),
                            a.clone(),
                        );
                        test_write_read_zenoh_message(msg);
                    }
                }
            }
        }
    }
}
