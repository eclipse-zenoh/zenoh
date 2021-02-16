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
use super::core::*;
use super::io::RBuf;

use super::msg::*;

impl RBuf {
    pub fn read_session_message(&mut self) -> Option<SessionMessage> {
        use super::smsg::id::*;

        let mut attachment = None;

        // Read the message
        let (header, body) = loop {
            // Read the header
            let header = self.read()?;

            // Read the body
            let body = match smsg::mid(header) {
                // Frame as first for optimization reasons
                FRAME => {
                    let (ch, reliability) = match smsg::has_flag(header, smsg::flag::R) {
                        true => (Channel::Reliable, Reliability::Reliable),
                        false => (Channel::BestEffort, Reliability::BestEffort),
                    };
                    let sn = self.read_zint()?;

                    let payload = if smsg::has_flag(header, smsg::flag::F) {
                        // A fragmented frame is not supposed to be followed by
                        // any other frame in the same batch. Read all the bytes.
                        let mut buffer = RBuf::new();
                        self.drain_into_rbuf(&mut buffer);
                        let is_final = smsg::has_flag(header, smsg::flag::E);

                        FramePayload::Fragment { buffer, is_final }
                    } else {
                        // @TODO: modify the get_pos/set_pos to mark/revert
                        let mut messages: Vec<ZenohMessage> = Vec::with_capacity(1);
                        while self.can_read() {
                            let pos = self.get_pos();
                            if let Some(msg) = self.read_zenoh_message(reliability) {
                                messages.push(msg);
                            } else if self.set_pos(pos) {
                                break;
                            } else {
                                return None;
                            }
                        }

                        FramePayload::Messages { messages }
                    };

                    SessionBody::Frame(Frame { ch, sn, payload })
                }

                // Decorator
                ATTACHMENT => {
                    attachment = Some(self.read_deco_attachment(header)?);
                    continue;
                }

                // Messages
                SCOUT => {
                    let pid_request = smsg::has_flag(header, smsg::flag::I);
                    let what = if smsg::has_flag(header, smsg::flag::W) {
                        Some(self.read_zint()?)
                    } else {
                        None
                    };

                    SessionBody::Scout(Scout { what, pid_request })
                }

                HELLO => {
                    let pid = if smsg::has_flag(header, smsg::flag::I) {
                        Some(self.read_peerid()?)
                    } else {
                        None
                    };
                    let whatami = if smsg::has_flag(header, smsg::flag::W) {
                        Some(self.read_zint()?)
                    } else {
                        None
                    };
                    let locators = if smsg::has_flag(header, smsg::flag::L) {
                        Some(self.read_locators()?)
                    } else {
                        None
                    };

                    SessionBody::Hello(Hello {
                        pid,
                        whatami,
                        locators,
                    })
                }

                INIT => {
                    if smsg::has_flag(header, smsg::flag::A) {
                        let whatami = self.read_zint()?;
                        let pid = self.read_peerid()?;
                        let sn_resolution = if smsg::has_flag(header, smsg::flag::S) {
                            Some(self.read_zint()?)
                        } else {
                            None
                        };
                        let cookie = self.read_rbuf()?;

                        SessionBody::InitAck(InitAck {
                            whatami,
                            pid,
                            sn_resolution,
                            cookie,
                        })
                    } else {
                        let version = self.read()?;
                        let whatami = self.read_zint()?;
                        let pid = self.read_peerid()?;
                        let sn_resolution = if smsg::has_flag(header, smsg::flag::S) {
                            Some(self.read_zint()?)
                        } else {
                            None
                        };

                        SessionBody::InitSyn(InitSyn {
                            version,
                            whatami,
                            pid,
                            sn_resolution,
                        })
                    }
                }

                OPEN => {
                    let lease = if smsg::has_flag(header, smsg::flag::T) {
                        1_000 * self.read_zint()?
                    } else {
                        self.read_zint()?
                    };
                    let initial_sn = self.read_zint()?;

                    if smsg::has_flag(header, smsg::flag::A) {
                        SessionBody::OpenAck(OpenAck { lease, initial_sn })
                    } else {
                        let cookie = self.read_rbuf()?;
                        SessionBody::OpenSyn(OpenSyn {
                            lease,
                            initial_sn,
                            cookie,
                        })
                    }
                }

                CLOSE => {
                    let link_only = smsg::has_flag(header, smsg::flag::K);
                    let pid = if smsg::has_flag(header, smsg::flag::I) {
                        Some(self.read_peerid()?)
                    } else {
                        None
                    };
                    let reason = self.read()?;

                    SessionBody::Close(Close {
                        pid,
                        reason,
                        link_only,
                    })
                }

                SYNC => {
                    let ch = match smsg::has_flag(header, smsg::flag::R) {
                        true => Channel::Reliable,
                        false => Channel::BestEffort,
                    };
                    let sn = self.read_zint()?;
                    let count = if smsg::has_flag(header, smsg::flag::C) {
                        Some(self.read_zint()?)
                    } else {
                        None
                    };

                    SessionBody::Sync(Sync { ch, sn, count })
                }

                ACK_NACK => {
                    let sn = self.read_zint()?;
                    let mask = if smsg::has_flag(header, smsg::flag::M) {
                        Some(self.read_zint()?)
                    } else {
                        None
                    };

                    SessionBody::AckNack(AckNack { sn, mask })
                }

                KEEP_ALIVE => {
                    let pid = if smsg::has_flag(header, smsg::flag::I) {
                        Some(self.read_peerid()?)
                    } else {
                        None
                    };

                    SessionBody::KeepAlive(KeepAlive { pid })
                }

                PING_PONG => {
                    let hash = self.read_zint()?;

                    if smsg::has_flag(header, smsg::flag::P) {
                        SessionBody::Ping(Ping { hash })
                    } else {
                        SessionBody::Pong(Pong { hash })
                    }
                }

                unknown => {
                    log::trace!("Session message with unknown ID: {}", unknown);
                    return None;
                }
            };

            break (header, body);
        };

        Some(SessionMessage {
            header,
            body,
            attachment,
        })
    }

    pub fn read_zenoh_message(&mut self, reliability: Reliability) -> Option<ZenohMessage> {
        use super::zmsg::id::*;

        // Message decorators
        let mut routing_context = None;
        let mut reply_context = None;
        let mut attachment = None;

        // Read the message
        let (header, body, congestion_control) = loop {
            // Read the header
            let header = self.read()?;

            // Read the body
            match zmsg::mid(header) {
                // Message data as first for optimization reasons
                DATA => {
                    let congestion_control = if zmsg::has_flag(header, zmsg::flag::D) {
                        CongestionControl::Drop
                    } else {
                        CongestionControl::Block
                    };
                    let key = self.read_reskey(zmsg::has_flag(header, zmsg::flag::K))?;
                    let data_info = if zmsg::has_flag(header, zmsg::flag::I) {
                        Some(self.read_data_info()?)
                    } else {
                        None
                    };
                    let payload = self.read_rbuf()?;

                    let body = ZenohBody::Data(Data {
                        key,
                        data_info,
                        payload,
                    });
                    break (header, body, congestion_control);
                }

                // Decorators
                ROUTING_CONTEXT => {
                    routing_context = Some(self.read_deco_routing_context(header)?);
                    continue;
                }

                REPLY_CONTEXT => {
                    reply_context = Some(self.read_deco_reply_context(header)?);
                    continue;
                }

                ATTACHMENT => {
                    attachment = Some(self.read_deco_attachment(header)?);
                    continue;
                }

                // Messages
                DECLARE => {
                    let declarations = self.read_declarations()?;

                    let body = ZenohBody::Declare(Declare { declarations });
                    let congestion_control = zmsg::default_congestion_control::DECLARE;
                    break (header, body, congestion_control);
                }

                UNIT => {
                    let congestion_control = if zmsg::has_flag(header, zmsg::flag::D) {
                        CongestionControl::Drop
                    } else {
                        CongestionControl::Block
                    };
                    let body = ZenohBody::Unit(Unit {});
                    break (header, body, congestion_control);
                }

                PULL => {
                    let is_final = zmsg::has_flag(header, zmsg::flag::F);
                    let key = self.read_reskey(zmsg::has_flag(header, zmsg::flag::K))?;
                    let pull_id = self.read_zint()?;
                    let max_samples = if zmsg::has_flag(header, zmsg::flag::N) {
                        Some(self.read_zint()?)
                    } else {
                        None
                    };

                    let body = ZenohBody::Pull(Pull {
                        key,
                        pull_id,
                        max_samples,
                        is_final,
                    });
                    let congestion_control = zmsg::default_congestion_control::PULL;
                    break (header, body, congestion_control);
                }

                QUERY => {
                    let key = self.read_reskey(zmsg::has_flag(header, zmsg::flag::K))?;
                    let predicate = self.read_string()?;
                    let qid = self.read_zint()?;
                    let target = if zmsg::has_flag(header, zmsg::flag::T) {
                        Some(self.read_query_target()?)
                    } else {
                        None
                    };
                    let consolidation = self.read_consolidation()?;

                    let body = ZenohBody::Query(Query {
                        key,
                        predicate,
                        qid,
                        target,
                        consolidation,
                    });
                    let congestion_control = zmsg::default_congestion_control::QUERY;
                    break (header, body, congestion_control);
                }

                LINK_STATE_LIST => {
                    let len = self.read_zint()?;
                    let mut link_states: Vec<LinkState> = Vec::new();
                    for _ in 0..len {
                        link_states.push(self.read_link_state()?);
                    }
                    let body = ZenohBody::LinkStateList(LinkStateList { link_states });
                    let congestion_control = zmsg::default_congestion_control::LINK_STATE_LIST;
                    break (header, body, congestion_control);
                }

                unknown => {
                    log::trace!("Zenoh message with unknown ID: {}", unknown);
                    return None;
                }
            }
        };

        Some(ZenohMessage {
            header,
            body,
            reliability,
            congestion_control,
            routing_context,
            reply_context,
            attachment,
        })
    }

    fn read_deco_routing_context(&mut self, _header: u8) -> Option<RoutingContext> {
        self.read_zint()
    }

    fn read_deco_attachment(&mut self, header: u8) -> Option<Attachment> {
        let encoding = smsg::flags(header);
        let buffer = self.read_rbuf()?;
        Some(Attachment { encoding, buffer })
    }

    // @TODO: Update the ReplyContext format
    fn read_deco_reply_context(&mut self, header: u8) -> Option<ReplyContext> {
        let is_final = zmsg::has_flag(header, zmsg::flag::F);
        let qid = self.read_zint()?;
        let source_kind = self.read_zint()?;
        let replier_id = if is_final {
            None
        } else {
            Some(self.read_peerid()?)
        };

        Some(ReplyContext {
            is_final,
            qid,
            source_kind,
            replier_id,
        })
    }

    pub fn read_data_info(&mut self) -> Option<DataInfo> {
        let options = self.read_zint()?;
        let source_id = if zmsg::has_option(options, zmsg::data::info::SRCID) {
            Some(self.read_peerid()?)
        } else {
            None
        };
        let source_sn = if zmsg::has_option(options, zmsg::data::info::SRCSN) {
            Some(self.read_zint()?)
        } else {
            None
        };
        let first_router_id = if zmsg::has_option(options, zmsg::data::info::RTRID) {
            Some(self.read_peerid()?)
        } else {
            None
        };
        let first_router_sn = if zmsg::has_option(options, zmsg::data::info::RTRSN) {
            Some(self.read_zint()?)
        } else {
            None
        };
        let timestamp = if zmsg::has_option(options, zmsg::data::info::TS) {
            Some(self.read_timestamp()?)
        } else {
            None
        };
        let kind = if zmsg::has_option(options, zmsg::data::info::KIND) {
            Some(self.read_zint()?)
        } else {
            None
        };
        let encoding = if zmsg::has_option(options, zmsg::data::info::ENC) {
            Some(self.read_zint()?)
        } else {
            None
        };

        Some(DataInfo {
            source_id,
            source_sn,
            first_router_id,
            first_router_sn,
            timestamp,
            kind,
            encoding,
        })
    }

    pub fn read_link_state(&mut self) -> Option<LinkState> {
        let options = self.read_zint()?;
        let psid = self.read_zint()?;
        let sn = self.read_zint()?;
        let pid = if zmsg::has_option(options, zmsg::link_state::PID) {
            Some(self.read_peerid()?)
        } else {
            None
        };
        let whatami = if zmsg::has_option(options, zmsg::link_state::WAI) {
            Some(self.read_zint()?)
        } else {
            None
        };
        let locators = if zmsg::has_option(options, zmsg::link_state::LOC) {
            Some(self.read_locators()?)
        } else {
            None
        };
        let len = self.read_zint()?;
        let mut links: Vec<ZInt> = Vec::new();
        for _ in 0..len {
            links.push(self.read_zint()?);
        }

        Some(LinkState {
            psid,
            sn,
            pid,
            whatami,
            locators,
            links,
        })
    }

    pub fn read_properties(&mut self) -> Option<Vec<Property>> {
        let len = self.read_zint()?;
        let mut vec: Vec<Property> = Vec::new();
        for _ in 0..len {
            vec.push(self.read_property()?);
        }
        Some(vec)
    }

    fn read_property(&mut self) -> Option<Property> {
        let key = self.read_zint()?;
        let value = self.read_bytes_array()?;
        Some(Property { key, value })
    }

    fn read_declarations(&mut self) -> Option<Vec<Declaration>> {
        let len = self.read_zint()?;
        let mut vec: Vec<Declaration> = Vec::new();
        for _ in 0..len {
            vec.push(self.read_declaration()?);
        }
        Some(vec)
    }

    fn read_declaration(&mut self) -> Option<Declaration> {
        use super::zmsg::declaration::id::*;
        use super::Declaration::*;

        macro_rules! read_key_delc {
            ($buf:ident, $header:ident, $type:ident) => {{
                Some($type {
                    key: $buf.read_reskey(zmsg::has_flag($header, zmsg::flag::K))?,
                })
            }};
        }

        let header = self.read()?;
        match zmsg::mid(header) {
            RESOURCE => {
                let rid = self.read_zint()?;
                let key = self.read_reskey(zmsg::has_flag(header, zmsg::flag::K))?;
                Some(Declaration::Resource { rid, key })
            }
            FORGET_RESOURCE => {
                let rid = self.read_zint()?;
                Some(Declaration::ForgetResource { rid })
            }

            SUBSCRIBER => {
                let reliability = if zmsg::has_flag(header, zmsg::flag::R) {
                    Reliability::Reliable
                } else {
                    Reliability::BestEffort
                };
                let key = self.read_reskey(zmsg::has_flag(header, zmsg::flag::K))?;
                let (mode, period) = if zmsg::has_flag(header, zmsg::flag::S) {
                    self.read_submode()?
                } else {
                    (SubMode::Push, None)
                };
                Some(Declaration::Subscriber {
                    key,
                    info: SubInfo {
                        reliability,
                        mode,
                        period,
                    },
                })
            }

            FORGET_SUBSCRIBER => read_key_delc!(self, header, ForgetSubscriber),
            PUBLISHER => read_key_delc!(self, header, Publisher),
            FORGET_PUBLISHER => read_key_delc!(self, header, ForgetPublisher),
            QUERYABLE => read_key_delc!(self, header, Queryable),
            FORGET_QUERYABLE => read_key_delc!(self, header, ForgetQueryable),

            id => {
                log::trace!("UNEXPECTED ID FOR Declaration: {}", id);
                None
            }
        }
    }

    fn read_submode(&mut self) -> Option<(SubMode, Option<Period>)> {
        use super::zmsg::declaration::flag::*;
        use super::zmsg::declaration::id::*;

        let mode_flag = self.read()?;
        let mode = match mode_flag & !PERIOD {
            MODE_PUSH => SubMode::Push,
            MODE_PULL => SubMode::Pull,
            id => {
                log::trace!("UNEXPECTED ID FOR SubMode: {}", id);
                return None;
            }
        };
        let period = if zmsg::has_flag(mode_flag, PERIOD) {
            Some(Period {
                origin: self.read_zint()?,
                period: self.read_zint()?,
                duration: self.read_zint()?,
            })
        } else {
            None
        };
        Some((mode, period))
    }

    fn read_reskey(&mut self, is_numeric: bool) -> Option<ResKey> {
        let id = self.read_zint()?;
        if is_numeric {
            Some(ResKey::RId(id))
        } else {
            let s = self.read_string()?;
            if id == NO_RESOURCE_ID {
                Some(ResKey::RName(s))
            } else {
                Some(ResKey::RIdWithSuffix(id, s))
            }
        }
    }

    fn read_query_target(&mut self) -> Option<QueryTarget> {
        let kind = self.read_zint()?;
        let target = self.read_target()?;
        Some(QueryTarget { kind, target })
    }

    fn read_target(&mut self) -> Option<Target> {
        let t = self.read_zint()?;
        match t {
            0 => Some(Target::BestMatching),
            1 => {
                let n = self.read_zint()?;
                Some(Target::Complete { n })
            }
            2 => Some(Target::All),
            3 => Some(Target::None),
            id => {
                log::trace!("UNEXPECTED ID FOR Target: {}", id);
                None
            }
        }
    }

    fn read_consolidation_mode(mode: ZInt) -> Option<ConsolidationMode> {
        match mode {
            0 => Some(ConsolidationMode::None),
            1 => Some(ConsolidationMode::Lazy),
            2 => Some(ConsolidationMode::Full),
            _ => {
                log::trace!("Invalid consolidation mode: {}", mode);
                None
            }
        }
    }

    fn read_consolidation(&mut self) -> Option<QueryConsolidation> {
        let modes = self.read_zint()?;
        Some(QueryConsolidation {
            first_routers: RBuf::read_consolidation_mode((modes >> 4) & 0x03)?,
            last_router: RBuf::read_consolidation_mode((modes >> 2) & 0x03)?,
            reception: RBuf::read_consolidation_mode(modes & 0x03)?,
        })
    }

    pub fn read_timestamp(&mut self) -> Option<Timestamp> {
        let time = self.read_zint_as_u64()?;
        let size = self.read_zint_as_usize()?;
        if size > (uhlc::ID::MAX_SIZE) {
            log::trace!(
                "Reading a Timestamp's ID size that exceed {} bytes: {}",
                uhlc::ID::MAX_SIZE,
                size
            );
            return None;
        }
        let mut id = [0u8; PeerId::MAX_SIZE];
        if self.read_bytes(&mut id[..size]) {
            Some(Timestamp::new(uhlc::NTP64(time), uhlc::ID::new(size, id)))
        } else {
            None
        }
    }
}
