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
use std::convert::TryFrom;

use super::decl::*;
use super::msg::*;

use crate::core::*;
use crate::io::WBuf;
use crate::link::Locator;

use zenoh_util::to_zint;

macro_rules! check {
    ($op:expr) => {
        if !$op {
            return false;
        }
    };
}

impl WBuf {
    pub fn write_frame_header(
        &mut self,
        ch: Channel,
        sn: ZInt,
        is_fragment: Option<bool>,
        attachment: Option<Attachment>,
    ) -> bool {
        if let Some(attachment) = attachment {
            check!(self.write_deco_attachment(&attachment, true));
        }

        let header = SessionMessage::make_frame_header(ch, is_fragment);

        self.write(header) && self.write_zint(sn)
    }

    pub fn write_session_message(&mut self, msg: &SessionMessage) -> bool {
        if let Some(attachment) = msg.get_attachment() {
            check!(self.write_deco_attachment(attachment, true));
        };

        check!(self.write(msg.header));
        match msg.get_body() {
            SessionBody::Frame(Frame { sn, payload, .. }) => {
                check!(self.write_zint(*sn));
                match payload {
                    FramePayload::Fragment { buffer, .. } => {
                        check!(self.write_rbuf_slices(&buffer));
                    }
                    FramePayload::Messages { messages } => {
                        for m in messages {
                            check!(self.write_zenoh_message(m));
                        }
                    }
                }
            }

            SessionBody::Scout(Scout { what, .. }) => {
                if let Some(w) = *what {
                    check!(self.write_zint(w));
                }
            }

            SessionBody::Hello(Hello {
                pid,
                whatami,
                locators,
            }) => {
                if let Some(pid) = pid {
                    check!(self.write_peerid(pid));
                }
                if let Some(w) = *whatami {
                    if w != whatami::BROKER {
                        check!(self.write_zint(w));
                    }
                }
                if let Some(locs) = locators {
                    check!(self.write_locators(locs.as_ref()));
                }
            }

            SessionBody::Open(Open {
                version,
                whatami,
                pid,
                lease,
                initial_sn,
                sn_resolution,
                locators,
            }) => {
                check!(self.write(*version));
                check!(self.write_zint(*whatami));
                check!(self.write_peerid(pid));
                check!(self.write_zint(*lease));
                check!(self.write_zint(*initial_sn));
                // Compute the options byte flags
                let mut options: u8 = 0;
                if sn_resolution.is_some() {
                    options |= smsg::flag::S;
                }
                if locators.is_some() {
                    options |= smsg::flag::L;
                }
                if options != 0 {
                    check!(self.write(options));
                    if let Some(snr) = *sn_resolution {
                        check!(self.write_zint(snr));
                    }
                    if let Some(locs) = locators {
                        check!(self.write_locators(locs.as_ref()));
                    }
                }
            }

            SessionBody::Accept(Accept {
                whatami,
                opid,
                apid,
                initial_sn,
                sn_resolution,
                lease,
                locators,
            }) => {
                check!(self.write_zint(*whatami));
                check!(self.write_peerid(opid));
                check!(self.write_peerid(apid));
                check!(self.write_zint(*initial_sn));
                // Compute the options byte flags
                let mut options: u8 = 0;
                if sn_resolution.is_some() {
                    options |= smsg::flag::S;
                }
                if lease.is_some() {
                    options |= smsg::flag::D;
                }
                if locators.is_some() {
                    options |= smsg::flag::L;
                }
                if options != 0 {
                    check!(self.write(options));
                    if let Some(snr) = *sn_resolution {
                        check!(self.write_zint(snr));
                    }
                    if let Some(l) = *lease {
                        check!(self.write_zint(l));
                    }
                    if let Some(locs) = locators {
                        check!(self.write_locators(locs.as_ref()));
                    }
                }
            }

            SessionBody::Close(Close { pid, reason, .. }) => {
                if let Some(p) = pid {
                    check!(self.write_peerid(p));
                }
                check!(self.write(*reason));
            }

            SessionBody::Sync(Sync { sn, count, .. }) => {
                check!(self.write_zint(*sn));
                if let Some(c) = *count {
                    check!(self.write_zint(c));
                }
            }

            SessionBody::AckNack(AckNack { sn, mask }) => {
                check!(self.write_zint(*sn));
                if let Some(m) = *mask {
                    check!(self.write_zint(m));
                }
            }

            SessionBody::KeepAlive(KeepAlive { pid }) => {
                if let Some(p) = pid {
                    check!(self.write_peerid(p));
                }
            }

            SessionBody::Ping(Ping { hash }) | SessionBody::Pong(Pong { hash }) => {
                check!(self.write_zint(*hash));
            }
        }

        true
    }

    pub fn write_zenoh_message(&mut self, msg: &ZenohMessage) -> bool {
        if let Some(attachment) = &msg.attachment {
            check!(self.write_deco_attachment(attachment, false));
        }
        if let Some(reply_context) = &msg.reply_context {
            check!(self.write_deco_reply(reply_context));
        }

        check!(self.write(msg.header));
        match &msg.body {
            ZenohBody::Declare(Declare { declarations }) => {
                check!(self.write_declarations(&declarations));
            }

            ZenohBody::Data(Data { key, info, payload }) => {
                check!(self.write_reskey(&key));
                if let Some(data_info) = info {
                    check!(self.write_datainfo(&data_info));
                }
                check!(self.write_rbuf(&payload));
            }

            ZenohBody::Unit(Unit {}) => {}

            ZenohBody::Pull(Pull {
                key,
                pull_id,
                max_samples,
                ..
            }) => {
                check!(self.write_reskey(&key));
                check!(self.write_zint(*pull_id));
                if let Some(n) = max_samples {
                    check!(self.write_zint(*n));
                }
            }

            ZenohBody::Query(Query {
                key,
                predicate,
                qid,
                target,
                consolidation,
            }) => {
                check!(self.write_reskey(&key));
                check!(self.write_string(predicate));
                check!(self.write_zint(*qid));
                if let Some(t) = target {
                    check!(self.write_query_target(t));
                }
                check!(self.write_consolidation(consolidation));
            }
        }

        true
    }

    pub fn write_datainfo(&mut self, info: &DataInfo) -> bool {
        let mut header = 0u8;
        if info.source_id.is_some() {
            header |= zmsg::info_flag::SRCID
        }
        if info.source_sn.is_some() {
            header |= zmsg::info_flag::SRCSN
        }
        if info.first_broker_id.is_some() {
            header |= zmsg::info_flag::BKRID
        }
        if info.first_broker_sn.is_some() {
            header |= zmsg::info_flag::BKRSN
        }
        if info.timestamp.is_some() {
            header |= zmsg::info_flag::TS
        }
        if info.kind.is_some() {
            header |= zmsg::info_flag::KIND
        }
        if info.encoding.is_some() {
            header |= zmsg::info_flag::ENC
        }

        check!(self.write(header));
        if let Some(pid) = &info.source_id {
            check!(self.write_peerid(pid));
        }
        if let Some(sn) = &info.source_sn {
            check!(self.write_zint(*sn));
        }
        if let Some(pid) = &info.first_broker_id {
            check!(self.write_peerid(pid));
        }
        if let Some(sn) = &info.first_broker_sn {
            check!(self.write_zint(*sn));
        }
        if let Some(ts) = &info.timestamp {
            check!(self.write_timestamp(&ts));
        }
        if let Some(kind) = &info.kind {
            check!(self.write_zint(*kind));
        }
        if let Some(enc) = &info.encoding {
            check!(self.write_zint(*enc));
        }

        true
    }

    pub fn write_properties(&mut self, props: &[Property]) {
        self.write_zint(to_zint!(props.len()));
        for p in props {
            self.write_property(p);
        }
    }

    fn write_property(&mut self, p: &Property) -> bool {
        self.write_zint(p.key) && self.write_bytes_array(&p.value)
    }

    fn write_deco_attachment(&mut self, attachment: &Attachment, session: bool) -> bool {
        if session {
            check!(self.write(attachment.encoding | smsg::id::ATTACHMENT));
        } else {
            check!(self.write(attachment.encoding | zmsg::id::ATTACHMENT));
        }
        self.write_rbuf(&attachment.buffer)
    }

    fn write_deco_reply(&mut self, reply_context: &ReplyContext) -> bool {
        let fflag = if reply_context.is_final {
            zmsg::flag::F
        } else {
            0
        };
        check!(self.write(zmsg::id::REPLY_CONTEXT | fflag));
        check!(self.write_zint(reply_context.qid));
        check!(self.write_zint(reply_context.source_kind));
        if let Some(pid) = &reply_context.replier_id {
            check!(self.write_peerid(pid));
        }

        true
    }

    fn write_locators(&mut self, locators: &[Locator]) -> bool {
        check!(self.write_zint(to_zint!(locators.len())));
        for l in locators {
            check!(self.write_string(&l.to_string()));
        }

        true
    }

    fn write_declarations(&mut self, declarations: &[Declaration]) -> bool {
        check!(self.write_zint(to_zint!(declarations.len())));
        for l in declarations {
            check!(self.write_declaration(l));
        }
        true
    }

    fn write_declaration(&mut self, declaration: &Declaration) -> bool {
        use super::decl::{id::*, Declaration::*};

        macro_rules! write_key_decl {
            ($buf:ident, $flag:ident, $key:ident) => {{
                $buf.write(
                    $flag
                        | (if $key.is_numerical() {
                            zmsg::flag::K
                        } else {
                            0
                        }),
                ) && $buf.write_reskey($key)
            }};
        }

        match declaration {
            Resource { rid, key } => {
                let kflag = if key.is_numerical() { zmsg::flag::K } else { 0 };
                self.write(RESOURCE | kflag) && self.write_zint(*rid) && self.write_reskey(key)
            }

            ForgetResource { rid } => self.write(FORGET_RESOURCE) && self.write_zint(*rid),

            Subscriber { key, info } => {
                let kflag = if key.is_numerical() { zmsg::flag::K } else { 0 };
                let sflag = if info.mode == SubMode::Push && info.period.is_none() {
                    0
                } else {
                    zmsg::flag::S
                };
                let rflag = if info.reliability == Reliability::Reliable {
                    zmsg::flag::R
                } else {
                    0
                };
                self.write(SUBSCRIBER | rflag | sflag | kflag)
                    && self.write_reskey(key)
                    && (sflag == 0 || self.write_submode(&info.mode, &info.period))
            }

            ForgetSubscriber { key } => write_key_decl!(self, FORGET_SUBSCRIBER, key),
            Publisher { key } => write_key_decl!(self, PUBLISHER, key),
            ForgetPublisher { key } => write_key_decl!(self, FORGET_PUBLISHER, key),
            Queryable { key } => write_key_decl!(self, QUERYABLE, key),
            ForgetQueryable { key } => write_key_decl!(self, FORGET_QUERYABLE, key),
        }
    }

    fn write_submode(&mut self, mode: &SubMode, period: &Option<Period>) -> bool {
        let period_mask: u8 = if period.is_some() { id::PERIOD } else { 0x00 };
        check!(match mode {
            SubMode::Push => self.write(id::MODE_PUSH | period_mask),
            SubMode::Pull => self.write(id::MODE_PULL | period_mask),
        });
        if let Some(p) = period {
            self.write_zint(p.origin) && self.write_zint(p.period) && self.write_zint(p.duration)
        } else {
            true
        }
    }

    fn write_reskey(&mut self, key: &ResKey) -> bool {
        match key {
            ResKey::RId(rid) => self.write_zint(*rid),
            ResKey::RName(name) => self.write_zint(NO_RESOURCE_ID) && self.write_string(name),
            ResKey::RIdWithSuffix(rid, suffix) => {
                self.write_zint(*rid) && self.write_string(suffix)
            }
        }
    }

    fn write_query_target(&mut self, target: &QueryTarget) -> bool {
        self.write_zint(target.kind) && self.write_target(&target.target)
    }

    fn write_target(&mut self, target: &Target) -> bool {
        match target {
            Target::BestMatching => self.write_zint(0 as ZInt),
            Target::Complete { n } => self.write_zint(1 as ZInt) && self.write_zint(*n),
            Target::All => self.write_zint(2 as ZInt),
            Target::None => self.write_zint(3 as ZInt),
        }
    }

    fn write_consolidation(&mut self, consolidation: &QueryConsolidation) -> bool {
        match consolidation {
            QueryConsolidation::None => self.write_zint(0),
            QueryConsolidation::LastHop => self.write_zint(1),
            QueryConsolidation::Incremental => self.write_zint(2),
        }
    }

    fn write_peerid(&mut self, pid: &PeerId) -> bool {
        self.write_bytes_array(pid.as_slice())
    }

    fn write_timestamp(&mut self, tstamp: &Timestamp) -> bool {
        self.write_u64_as_zint(tstamp.get_time().as_u64())
            && self.write_bytes_array(tstamp.get_id().as_slice())
    }
}
