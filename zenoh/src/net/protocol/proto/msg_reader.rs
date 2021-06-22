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
    fn read_deco_attachment(&mut self, header: u8) -> Option<Attachment> {
        let buffer = {
            #[cfg(feature = "zero-copy")]
            {
                if imsg::has_flag(header, smsg::flag::Z) {
                    self.read_rbuf(true)?
                } else {
                    self.read_rbuf(false)?
                }
            }

            #[cfg(not(feature = "zero-copy"))]
            {
                if imsg::has_flag(header, smsg::flag::Z) {
                    return None;
                } else {
                    self.read_rbuf(false)?
                }
            }
        };
        Some(Attachment { buffer })
    }

    /*************************************/
    /*             SESSION               */
    /*************************************/
    pub fn read_session_message(&mut self) -> Option<SessionMessage> {
        use super::smsg::id::*;

        let mut attachment = None;

        // Read the message
        let body = loop {
            // Read the header
            let header = self.read()?;

            // Read the body
            match imsg::mid(header) {
                FRAME => break self.read_frame(header)?,
                ATTACHMENT => {
                    attachment = Some(self.read_deco_attachment(header)?);
                    continue;
                }
                SCOUT => break self.read_scout(header)?,
                HELLO => break self.read_hello(header)?,
                INIT => {
                    if imsg::has_flag(header, smsg::flag::A) {
                        break self.read_init_ack(header)?;
                    } else {
                        break self.read_init_syn(header)?;
                    }
                }
                OPEN => {
                    if imsg::has_flag(header, smsg::flag::A) {
                        break self.read_open_ack(header)?;
                    } else {
                        break self.read_open_syn(header)?;
                    }
                }
                CLOSE => break self.read_close(header)?,
                SYNC => break self.read_sync(header)?,
                ACK_NACK => break self.read_ack_nack(header)?,
                KEEP_ALIVE => break self.read_keep_alive(header)?,

                PING_PONG => {
                    if imsg::has_flag(header, smsg::flag::P) {
                        break self.read_ping(header)?;
                    } else {
                        break self.read_pong(header)?;
                    }
                }

                unknown => {
                    log::trace!("Session message with unknown ID: {}", unknown);
                    return None;
                }
            }
        };

        Some(SessionMessage { body, attachment })
    }

    fn read_frame(&mut self, header: u8) -> Option<SessionBody> {
        let (ch, reliability) = match imsg::has_flag(header, smsg::flag::R) {
            true => (Channel::Reliable, Reliability::Reliable),
            false => (Channel::BestEffort, Reliability::BestEffort),
        };
        let sn = self.read_zint()?;

        let payload = if imsg::has_flag(header, smsg::flag::F) {
            // A fragmented frame is not supposed to be followed by
            // any other frame in the same batch. Read all the bytes.
            let buffer = self.read_zslice(self.readable())?;
            let is_final = imsg::has_flag(header, smsg::flag::E);

            FramePayload::Fragment { buffer, is_final }
        } else {
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

        Some(SessionBody::Frame(Frame { ch, sn, payload }))
    }

    fn read_scout(&mut self, header: u8) -> Option<SessionBody> {
        let pid_request = imsg::has_flag(header, smsg::flag::I);
        let what = if imsg::has_flag(header, smsg::flag::W) {
            Some(self.read_zint()?)
        } else {
            None
        };

        Some(SessionBody::Scout(Scout { what, pid_request }))
    }

    fn read_hello(&mut self, header: u8) -> Option<SessionBody> {
        let pid = if imsg::has_flag(header, smsg::flag::I) {
            Some(self.read_peerid()?)
        } else {
            None
        };
        let whatami = if imsg::has_flag(header, smsg::flag::W) {
            Some(self.read_zint()?)
        } else {
            None
        };
        let locators = if imsg::has_flag(header, smsg::flag::L) {
            Some(self.read_locators()?)
        } else {
            None
        };

        Some(SessionBody::Hello(Hello {
            pid,
            whatami,
            locators,
        }))
    }

    fn read_init_syn(&mut self, header: u8) -> Option<SessionBody> {
        let version = self.read()?;
        let whatami = self.read_zint()?;
        let pid = self.read_peerid()?;
        let sn_resolution = if imsg::has_flag(header, smsg::flag::S) {
            Some(self.read_zint()?)
        } else {
            None
        };

        Some(SessionBody::InitSyn(InitSyn {
            version,
            whatami,
            pid,
            sn_resolution,
        }))
    }

    fn read_init_ack(&mut self, header: u8) -> Option<SessionBody> {
        let whatami = self.read_zint()?;
        let pid = self.read_peerid()?;
        let sn_resolution = if imsg::has_flag(header, smsg::flag::S) {
            Some(self.read_zint()?)
        } else {
            None
        };
        let cookie = self.read_zslice_array()?;

        Some(SessionBody::InitAck(InitAck {
            whatami,
            pid,
            sn_resolution,
            cookie,
        }))
    }

    fn read_open_syn(&mut self, header: u8) -> Option<SessionBody> {
        let lease = if imsg::has_flag(header, smsg::flag::T) {
            1_000 * self.read_zint()?
        } else {
            self.read_zint()?
        };
        let initial_sn = self.read_zint()?;

        let cookie = self.read_zslice_array()?;
        Some(SessionBody::OpenSyn(OpenSyn {
            lease,
            initial_sn,
            cookie,
        }))
    }

    fn read_open_ack(&mut self, header: u8) -> Option<SessionBody> {
        let lease = if imsg::has_flag(header, smsg::flag::T) {
            1_000 * self.read_zint()?
        } else {
            self.read_zint()?
        };
        let initial_sn = self.read_zint()?;

        Some(SessionBody::OpenAck(OpenAck { lease, initial_sn }))
    }

    fn read_close(&mut self, header: u8) -> Option<SessionBody> {
        let link_only = imsg::has_flag(header, smsg::flag::K);
        let pid = if imsg::has_flag(header, smsg::flag::I) {
            Some(self.read_peerid()?)
        } else {
            None
        };
        let reason = self.read()?;

        Some(SessionBody::Close(Close {
            pid,
            reason,
            link_only,
        }))
    }

    fn read_sync(&mut self, header: u8) -> Option<SessionBody> {
        let ch = match imsg::has_flag(header, smsg::flag::R) {
            true => Channel::Reliable,
            false => Channel::BestEffort,
        };
        let sn = self.read_zint()?;
        let count = if imsg::has_flag(header, smsg::flag::C) {
            Some(self.read_zint()?)
        } else {
            None
        };

        Some(SessionBody::Sync(Sync { ch, sn, count }))
    }

    fn read_ack_nack(&mut self, header: u8) -> Option<SessionBody> {
        let sn = self.read_zint()?;
        let mask = if imsg::has_flag(header, smsg::flag::M) {
            Some(self.read_zint()?)
        } else {
            None
        };

        Some(SessionBody::AckNack(AckNack { sn, mask }))
    }

    fn read_keep_alive(&mut self, header: u8) -> Option<SessionBody> {
        let pid = if imsg::has_flag(header, smsg::flag::I) {
            Some(self.read_peerid()?)
        } else {
            None
        };

        Some(SessionBody::KeepAlive(KeepAlive { pid }))
    }

    fn read_ping(&mut self, _header: u8) -> Option<SessionBody> {
        let hash = self.read_zint()?;
        Some(SessionBody::Ping(Ping { hash }))
    }

    fn read_pong(&mut self, _header: u8) -> Option<SessionBody> {
        let hash = self.read_zint()?;
        Some(SessionBody::Pong(Pong { hash }))
    }

    /*************************************/
    /*              ZENOH                */
    /*************************************/
    fn read_deco_routing_context(&mut self, _header: u8) -> Option<RoutingContext> {
        let tree_id = self.read_zint()?;
        Some(RoutingContext { tree_id })
    }

    // @TODO: Update the ReplyContext format
    fn read_deco_reply_context(&mut self, header: u8) -> Option<ReplyContext> {
        let qid = self.read_zint()?;
        let source_kind = self.read_zint()?;
        let replier_id = if imsg::has_flag(header, zmsg::flag::F) {
            None
        } else {
            Some(self.read_peerid()?)
        };

        Some(ReplyContext {
            qid,
            source_kind,
            replier_id,
        })
    }

    pub fn read_zenoh_message(&mut self, reliability: Reliability) -> Option<ZenohMessage> {
        use super::zmsg::id::*;

        #[cfg(feature = "stats")]
        let start_readable = self.readable();

        // Message decorators
        let mut routing_context = None;
        let mut reply_context = None;
        let mut attachment = None;

        // Read the message
        let body = loop {
            // Read the header
            let header = self.read()?;

            // Read the body
            match imsg::mid(header) {
                DATA => break self.read_data(header, reliability)?,
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
                DECLARE => break self.read_declare(header, reliability)?,
                UNIT => break self.read_unit(header, reliability)?,
                PULL => break self.read_pull(header, reliability)?,
                QUERY => break self.read_query(header, reliability)?,
                LINK_STATE_LIST => break self.read_link_state_list(header, reliability)?,
                unknown => {
                    log::trace!("Zenoh message with unknown ID: {}", unknown);
                    return None;
                }
            }
        };

        #[cfg(feature = "stats")]
        let stop_readable = self.readable();

        Some(ZenohMessage {
            body,
            routing_context,
            reply_context,
            attachment,
            #[cfg(feature = "stats")]
            size: std::num::NonZeroUsize::new(start_readable - stop_readable),
        })
    }

    fn read_data(&mut self, header: u8, reliability: Reliability) -> Option<ZenohBody> {
        let congestion_control = if imsg::has_flag(header, zmsg::flag::D) {
            CongestionControl::Drop
        } else {
            CongestionControl::Block
        };
        let key = self.read_reskey(imsg::has_flag(header, zmsg::flag::K))?;
        let (data_info, is_sliced) = if imsg::has_flag(header, zmsg::flag::I) {
            let di = self.read_data_info()?;
            let is_sliced = di.is_sliced;
            (Some(di), is_sliced)
        } else {
            (None, false)
        };
        let payload = self.read_rbuf(is_sliced)?;

        let body = ZenohBody::Data(Data {
            key,
            data_info,
            payload,
            reliability,
            congestion_control,
        });
        Some(body)
    }

    pub fn read_data_info(&mut self) -> Option<DataInfo> {
        let options = self.read_zint()?;
        let kind = if imsg::has_option(options, zmsg::data::info::KIND) {
            Some(self.read_zint()?)
        } else {
            None
        };
        let encoding = if imsg::has_option(options, zmsg::data::info::ENC) {
            Some(self.read_zint()?)
        } else {
            None
        };
        let timestamp = if imsg::has_option(options, zmsg::data::info::TS) {
            Some(self.read_timestamp()?)
        } else {
            None
        };
        #[cfg(feature = "zero-copy")]
        let is_sliced = imsg::has_option(options, zmsg::data::info::SLICED);
        let source_id = if imsg::has_option(options, zmsg::data::info::SRCID) {
            Some(self.read_peerid()?)
        } else {
            None
        };
        let source_sn = if imsg::has_option(options, zmsg::data::info::SRCSN) {
            Some(self.read_zint()?)
        } else {
            None
        };
        let first_router_id = if imsg::has_option(options, zmsg::data::info::RTRID) {
            Some(self.read_peerid()?)
        } else {
            None
        };
        let first_router_sn = if imsg::has_option(options, zmsg::data::info::RTRSN) {
            Some(self.read_zint()?)
        } else {
            None
        };

        Some(DataInfo {
            kind,
            encoding,
            timestamp,
            #[cfg(feature = "zero-copy")]
            is_sliced,
            source_id,
            source_sn,
            first_router_id,
            first_router_sn,
        })
    }

    fn read_unit(&mut self, header: u8, reliability: Reliability) -> Option<ZenohBody> {
        let congestion_control = if imsg::has_flag(header, zmsg::flag::D) {
            CongestionControl::Drop
        } else {
            CongestionControl::Block
        };
        Some(ZenohBody::Unit(Unit {
            reliability,
            congestion_control,
        }))
    }

    fn read_pull(&mut self, header: u8, _reliability: Reliability) -> Option<ZenohBody> {
        let key = self.read_reskey(imsg::has_flag(header, zmsg::flag::K))?;
        let pull_id = self.read_zint()?;
        let max_samples = if imsg::has_flag(header, zmsg::flag::N) {
            Some(self.read_zint()?)
        } else {
            None
        };
        let is_final = imsg::has_flag(header, zmsg::flag::F);

        Some(ZenohBody::Pull(Pull {
            key,
            pull_id,
            max_samples,
            is_final,
        }))
    }

    fn read_declare(&mut self, _header: u8, _reliability: Reliability) -> Option<ZenohBody> {
        let declarations = self.read_declarations()?;
        Some(ZenohBody::Declare(Declare { declarations }))
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

        let header = self.read()?;
        match imsg::mid(header) {
            RESOURCE => {
                let rid = self.read_zint()?;
                let key = self.read_reskey(imsg::has_flag(header, zmsg::flag::K))?;
                Some(Declaration::Resource(Resource { rid, key }))
            }
            FORGET_RESOURCE => {
                let rid = self.read_zint()?;
                Some(Declaration::ForgetResource(ForgetResource { rid }))
            }
            SUBSCRIBER => {
                let reliability = if imsg::has_flag(header, zmsg::flag::R) {
                    Reliability::Reliable
                } else {
                    Reliability::BestEffort
                };
                let key = self.read_reskey(imsg::has_flag(header, zmsg::flag::K))?;
                let (mode, period) = if imsg::has_flag(header, zmsg::flag::S) {
                    self.read_submode()?
                } else {
                    (SubMode::Push, None)
                };
                Some(Declaration::Subscriber(Subscriber {
                    key,
                    info: SubInfo {
                        reliability,
                        mode,
                        period,
                    },
                }))
            }
            FORGET_SUBSCRIBER => {
                let key = self.read_reskey(imsg::has_flag(header, zmsg::flag::K))?;
                Some(Declaration::ForgetSubscriber(ForgetSubscriber { key }))
            }
            PUBLISHER => {
                let key = self.read_reskey(imsg::has_flag(header, zmsg::flag::K))?;
                Some(Declaration::Publisher(Publisher { key }))
            }
            FORGET_PUBLISHER => {
                let key = self.read_reskey(imsg::has_flag(header, zmsg::flag::K))?;
                Some(Declaration::ForgetPublisher(ForgetPublisher { key }))
            }
            QUERYABLE => {
                let key = self.read_reskey(imsg::has_flag(header, zmsg::flag::K))?;
                let kind = if imsg::has_flag(header, zmsg::flag::Q) {
                    self.read_zint()?
                } else {
                    queryable::STORAGE
                };
                Some(Declaration::Queryable(Queryable { key, kind }))
            }
            FORGET_QUERYABLE => {
                let key = self.read_reskey(imsg::has_flag(header, zmsg::flag::K))?;
                Some(Declaration::ForgetQueryable(ForgetQueryable { key }))
            }
            unknown => {
                log::trace!("Invalid ID for Declaration: {}", unknown);
                None
            }
        }
    }

    fn read_query(&mut self, header: u8, _reliability: Reliability) -> Option<ZenohBody> {
        let key = self.read_reskey(imsg::has_flag(header, zmsg::flag::K))?;
        let predicate = self.read_string()?;
        let qid = self.read_zint()?;
        let target = if imsg::has_flag(header, zmsg::flag::T) {
            Some(self.read_query_target()?)
        } else {
            None
        };
        let consolidation = self.read_consolidation()?;

        Some(ZenohBody::Query(Query {
            key,
            predicate,
            qid,
            target,
            consolidation,
        }))
    }

    fn read_link_state_list(
        &mut self,
        _header: u8,
        _reliability: Reliability,
    ) -> Option<ZenohBody> {
        let len = self.read_zint()?;
        let mut link_states: Vec<LinkState> = Vec::new();
        for _ in 0..len {
            link_states.push(self.read_link_state()?);
        }
        Some(ZenohBody::LinkStateList(LinkStateList { link_states }))
    }

    pub fn read_link_state(&mut self) -> Option<LinkState> {
        let options = self.read_zint()?;
        let psid = self.read_zint()?;
        let sn = self.read_zint()?;
        let pid = if imsg::has_option(options, zmsg::link_state::PID) {
            Some(self.read_peerid()?)
        } else {
            None
        };
        let whatami = if imsg::has_option(options, zmsg::link_state::WAI) {
            Some(self.read_zint()?)
        } else {
            None
        };
        let locators = if imsg::has_option(options, zmsg::link_state::LOC) {
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
        let period = if imsg::has_flag(mode_flag, PERIOD) {
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
            unknown => {
                log::trace!("Invalid consolidation mode: {}", unknown);
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
