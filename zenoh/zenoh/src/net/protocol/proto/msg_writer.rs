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
use super::io::WBuf;
use super::msg::*;
use zenoh_core::zcheck;

impl WBuf {
    #[inline(always)]
    fn write_deco_attachment(&mut self, attachment: &Attachment) -> bool {
        zcheck!(self.write(attachment.header()));
        #[cfg(feature = "shared-memory")]
        {
            self.write_zbuf(&attachment.buffer, attachment.buffer.has_shminfo())
        }

        #[cfg(not(feature = "shared-memory"))]
        {
            self.write_zbuf(&attachment.buffer)
        }
    }

    #[inline(always)]
    fn write_deco_routing_context(&mut self, routing_context: &RoutingContext) -> bool {
        zcheck!(self.write(routing_context.header()));
        self.write_zint(routing_context.tree_id)
    }

    #[inline(always)]
    fn write_deco_reply_context(&mut self, reply_context: &ReplyContext) -> bool {
        zcheck!(self.write(reply_context.header()));
        zcheck!(self.write_zint(reply_context.qid));
        if let Some(replier) = reply_context.replier.as_ref() {
            zcheck!(self.write_zint(replier.kind));
            zcheck!(self.write_peeexpr_id(&replier.id));
        }
        true
    }

    #[inline(always)]
    fn write_deco_priority(&mut self, priority: Priority) -> bool {
        self.write(priority.header())
    }

    #[inline(always)]
    pub fn write_frame_header(
        &mut self,
        priority: Priority,
        reliability: Reliability,
        sn: ZInt,
        is_fragment: Option<bool>,
        attachment: Option<Attachment>,
    ) -> bool {
        if let Some(attachment) = attachment {
            zcheck!(self.write_deco_attachment(&attachment));
        }
        if priority != Priority::default() {
            zcheck!(self.write_deco_priority(priority))
        }

        let header = Frame::make_header(reliability, is_fragment);
        self.write(header) && self.write_zint(sn)
    }

    /*************************************/
    /*            TRANSPORT              */
    /*************************************/
    #[allow(clippy::let_and_return)]
    pub fn write_transport_message(&mut self, msg: &mut TransportMessage) -> bool {
        #[cfg(feature = "stats")]
        let start_written = self.len();

        if let Some(attachment) = msg.attachment.as_ref() {
            zcheck!(self.write_deco_attachment(attachment));
        }

        let res = match &mut msg.body {
            TransportBody::Frame(frame) => self.write_frame(frame),
            TransportBody::Scout(scout) => self.write_scout(scout),
            TransportBody::Hello(hello) => self.write_hello(hello),
            TransportBody::InitSyn(init_syn) => self.write_init_syn(init_syn),
            TransportBody::InitAck(init_ack) => self.write_init_ack(init_ack),
            TransportBody::OpenSyn(open_syn) => self.write_open_syn(open_syn),
            TransportBody::OpenAck(open_ack) => self.write_open_ack(open_ack),
            TransportBody::Join(join) => self.write_join(join),
            TransportBody::Close(close) => self.write_close(close),
            TransportBody::Sync(sync) => self.write_sync(sync),
            TransportBody::AckNack(ack_nack) => self.write_ack_nack(ack_nack),
            TransportBody::KeepAlive(keep_alive) => self.write_keep_alive(keep_alive),
            TransportBody::Ping(ping) => self.write_ping(ping),
            TransportBody::Pong(pong) => self.write_pong(pong),
        };

        #[cfg(feature = "stats")]
        {
            let stop_written = self.len();
            msg.size = std::num::NonZeroUsize::new(stop_written - start_written);
        }

        res
    }

    fn write_frame(&mut self, frame: &mut Frame) -> bool {
        if frame.channel.priority != Priority::default() {
            zcheck!(self.write_deco_priority(frame.channel.priority))
        }

        zcheck!(self.write(frame.header()));
        zcheck!(self.write_zint(frame.sn));
        match &mut frame.payload {
            FramePayload::Fragment { buffer, .. } => self.write_zslice(buffer.clone()),
            FramePayload::Messages { messages } => {
                for m in messages.iter_mut() {
                    zcheck!(self.write_zenoh_message(m));
                }
                true
            }
        }
    }

    fn write_scout(&mut self, scout: &Scout) -> bool {
        zcheck!(self.write(scout.header()));
        match scout.what {
            Some(w) => self.write_zint(w.into()),
            None => true,
        }
    }

    fn write_hello(&mut self, hello: &Hello) -> bool {
        zcheck!(self.write(hello.header()));
        if let Some(pid) = hello.pid.as_ref() {
            zcheck!(self.write_peeexpr_id(pid));
        }
        if let Some(w) = hello.whatami {
            if w != WhatAmI::Router {
                zcheck!(self.write_zint(w.into()));
            }
        }
        if let Some(locs) = hello.locators.as_ref() {
            zcheck!(self.write_locators(locs));
        }
        true
    }

    fn write_init_syn(&mut self, init_syn: &InitSyn) -> bool {
        let header = init_syn.header();
        zcheck!(self.write(header));
        if init_syn.has_options() {
            zcheck!(self.write_zint(init_syn.options()));
        }
        zcheck!(self.write(init_syn.version));
        zcheck!(self.write_zint(init_syn.whatami.into()));
        zcheck!(self.write_peeexpr_id(&init_syn.pid));
        if imsg::has_flag(header, tmsg::flag::S) {
            zcheck!(self.write_zint(init_syn.sn_resolution));
        }
        true
    }

    fn write_init_ack(&mut self, init_ack: &InitAck) -> bool {
        zcheck!(self.write(init_ack.header()));
        if init_ack.has_options() {
            zcheck!(self.write_zint(init_ack.options()));
        }
        zcheck!(self.write_zint(init_ack.whatami.into()));
        zcheck!(self.write_peeexpr_id(&init_ack.pid));
        if let Some(snr) = init_ack.sn_resolution {
            zcheck!(self.write_zint(snr));
        }
        self.write_zslice_array(init_ack.cookie.clone())
    }

    fn write_open_syn(&mut self, open_syn: &OpenSyn) -> bool {
        let header = open_syn.header();
        zcheck!(self.write(header));
        if imsg::has_flag(header, tmsg::flag::T2) {
            zcheck!(self.write_zint(open_syn.lease.as_secs() as ZInt));
        } else {
            zcheck!(self.write_zint(open_syn.lease.as_millis() as ZInt));
        }
        zcheck!(self.write_zint(open_syn.initial_sn));
        self.write_zslice_array(open_syn.cookie.clone())
    }

    fn write_open_ack(&mut self, open_ack: &OpenAck) -> bool {
        let header = open_ack.header();
        zcheck!(self.write(header));
        if imsg::has_flag(header, tmsg::flag::T2) {
            zcheck!(self.write_zint(open_ack.lease.as_secs() as ZInt));
        } else {
            zcheck!(self.write_zint(open_ack.lease.as_millis() as ZInt));
        }
        self.write_zint(open_ack.initial_sn)
    }

    fn write_join(&mut self, join: &Join) -> bool {
        let header = join.header();
        zcheck!(self.write(header));
        if join.has_options() {
            zcheck!(self.write_zint(join.options()));
        }
        zcheck!(self.write(join.version));
        zcheck!(self.write_zint(join.whatami.into()));
        zcheck!(self.write_peeexpr_id(&join.pid));
        if imsg::has_flag(header, tmsg::flag::T1) {
            zcheck!(self.write_zint(join.lease.as_secs() as ZInt));
        } else {
            zcheck!(self.write_zint(join.lease.as_millis() as ZInt));
        }
        if imsg::has_flag(header, tmsg::flag::S) {
            zcheck!(self.write_zint(join.sn_resolution));
        }
        match &join.next_sns {
            ConduitSnList::Plain(sn) => {
                zcheck!(self.write_zint(sn.reliable));
                zcheck!(self.write_zint(sn.best_effort));
            }
            ConduitSnList::QoS(sns) => {
                for sn in sns.iter() {
                    zcheck!(self.write_zint(sn.reliable));
                    zcheck!(self.write_zint(sn.best_effort));
                }
            }
        }
        true
    }

    fn write_close(&mut self, close: &Close) -> bool {
        zcheck!(self.write(close.header()));
        if let Some(p) = close.pid.as_ref() {
            zcheck!(self.write_peeexpr_id(p));
        }
        self.write(close.reason)
    }

    fn write_sync(&mut self, sync: &Sync) -> bool {
        zcheck!(self.write(sync.header()));
        zcheck!(self.write_zint(sync.sn));
        if let Some(c) = sync.count {
            zcheck!(self.write_zint(c));
        }
        true
    }

    fn write_ack_nack(&mut self, ack_nack: &AckNack) -> bool {
        zcheck!(self.write(ack_nack.header()));
        zcheck!(self.write_zint(ack_nack.sn));
        if let Some(m) = ack_nack.mask {
            zcheck!(self.write_zint(m));
        }
        true
    }

    fn write_keep_alive(&mut self, keep_alive: &KeepAlive) -> bool {
        zcheck!(self.write(keep_alive.header()));
        if let Some(p) = keep_alive.pid.as_ref() {
            zcheck!(self.write_peeexpr_id(p));
        }
        true
    }

    fn write_ping(&mut self, ping: &Ping) -> bool {
        zcheck!(self.write(ping.header()));
        self.write_zint(ping.hash)
    }

    fn write_pong(&mut self, pong: &Pong) -> bool {
        zcheck!(self.write(pong.header()));
        self.write_zint(pong.hash)
    }

    /*************************************/
    /*              ZENOH                */
    /*************************************/
    #[allow(clippy::let_and_return)]
    pub fn write_zenoh_message(&mut self, msg: &mut ZenohMessage) -> bool {
        #[cfg(feature = "stats")]
        let start_written = self.len();

        if let Some(attachment) = msg.attachment.as_ref() {
            zcheck!(self.write_deco_attachment(attachment));
        }
        if let Some(routing_context) = msg.routing_context.as_ref() {
            zcheck!(self.write_deco_routing_context(routing_context));
        }
        if msg.channel.priority != Priority::default() {
            zcheck!(self.write_deco_priority(msg.channel.priority));
        }

        let res = match &msg.body {
            ZenohBody::Data(data) => self.write_data(data),
            ZenohBody::Declare(declare) => self.write_declare(declare),
            ZenohBody::Unit(unit) => self.write_unit(unit),
            ZenohBody::Pull(pull) => self.write_pull(pull),
            ZenohBody::Query(query) => self.write_query(query),
            ZenohBody::LinkStateList(link_state_list) => {
                self.write_link_state_list(link_state_list)
            }
        };

        #[cfg(feature = "stats")]
        {
            let stop_written = self.len();
            msg.size = std::num::NonZeroUsize::new(stop_written - start_written);
        }

        res
    }

    #[inline(always)]
    fn write_data(&mut self, data: &Data) -> bool {
        if let Some(reply_context) = data.reply_context.as_ref() {
            zcheck!(self.write_deco_reply_context(reply_context));
        }

        zcheck!(self.write(data.header()));
        zcheck!(self.write_key_expr(&data.key));

        #[cfg(feature = "shared-memory")]
        let mut sliced = false;

        if let Some(data_info) = data.data_info.as_ref() {
            zcheck!(self.write_data_info(data_info));
            #[cfg(feature = "shared-memory")]
            {
                sliced = data_info.sliced
            }
        }

        #[cfg(feature = "shared-memory")]
        {
            self.write_zbuf(&data.payload, sliced)
        }
        #[cfg(not(feature = "shared-memory"))]
        {
            self.write_zbuf(&data.payload)
        }
    }

    #[inline(always)]
    fn write_key_expr(&mut self, key: &KeyExpr) -> bool {
        if key.has_suffix() {
            self.write_zint(key.scope) && self.write_string(key.suffix.as_ref())
        } else {
            self.write_zint(key.scope)
        }
    }

    #[inline(always)]
    fn write_data_info(&mut self, info: &DataInfo) -> bool {
        zcheck!(self.write_zint(info.options()));

        if let Some(kind) = info.kind {
            zcheck!(self.write_zint(kind));
        }
        if let Some(enc) = info.encoding.as_ref() {
            zcheck!(self.write_zint(enc.prefix));
            zcheck!(self.write_string(enc.suffix.as_ref()));
        }
        if let Some(ts) = info.timestamp.as_ref() {
            zcheck!(self.write_timestamp(ts));
        }
        if let Some(pid) = info.source_id.as_ref() {
            zcheck!(self.write_peeexpr_id(pid));
        }
        if let Some(sn) = info.source_sn {
            zcheck!(self.write_zint(sn));
        }
        if let Some(pid) = info.first_router_id.as_ref() {
            zcheck!(self.write_peeexpr_id(pid));
        }
        if let Some(sn) = info.first_router_sn {
            zcheck!(self.write_zint(sn));
        }
        true
    }

    #[inline(always)]
    fn write_queryable_info(&mut self, info: &QueryableInfo) -> bool {
        zcheck!(self.write_zint(info.complete));
        zcheck!(self.write_zint(info.distance));
        true
    }

    fn write_declare(&mut self, declare: &Declare) -> bool {
        zcheck!(self.write(declare.header()));
        zcheck!(self.write_usize_as_zint(declare.declarations.len()));
        for l in declare.declarations.iter() {
            zcheck!(self.write_declaration(l));
        }
        true
    }

    fn write_declaration(&mut self, declaration: &Declaration) -> bool {
        match declaration {
            Declaration::Resource(r) => {
                self.write(r.header()) && self.write_zint(r.expr_id) && self.write_key_expr(&r.key)
            }
            Declaration::ForgetResource(fr) => {
                self.write(fr.header()) && self.write_zint(fr.expr_id)
            }
            Declaration::Subscriber(s) => {
                let header = s.header();
                zcheck!(self.write(header));
                zcheck!(self.write_key_expr(&s.key));
                if imsg::has_flag(header, zmsg::flag::S) {
                    zcheck!(self.write_submode(&s.info.mode, &s.info.period))
                }
                true
            }
            Declaration::ForgetSubscriber(fs) => {
                self.write(fs.header()) && self.write_key_expr(&fs.key)
            }
            Declaration::Publisher(p) => self.write(p.header()) && self.write_key_expr(&p.key),
            Declaration::ForgetPublisher(fp) => {
                self.write(fp.header()) && self.write_key_expr(&fp.key)
            }
            Declaration::Queryable(q) => {
                let header = q.header();
                zcheck!(self.write(header));
                zcheck!(self.write_key_expr(&q.key));
                zcheck!(self.write_zint(q.kind));
                if imsg::has_flag(header, zmsg::flag::Q) {
                    zcheck!(self.write_queryable_info(&q.info));
                }
                true
            }
            Declaration::ForgetQueryable(fq) => {
                zcheck!(self.write(fq.header()) && self.write_key_expr(&fq.key));
                zcheck!(self.write_zint(fq.kind));
                true
            }
        }
    }

    fn write_submode(&mut self, mode: &SubMode, period: &Option<Period>) -> bool {
        let period_mask: u8 = if period.is_some() {
            zmsg::declaration::flag::PERIOD
        } else {
            0
        };
        zcheck!(match mode {
            SubMode::Push => self.write(zmsg::declaration::id::MODE_PUSH | period_mask),
            SubMode::Pull => self.write(zmsg::declaration::id::MODE_PULL | period_mask),
        });
        if let Some(p) = period {
            self.write_zint(p.origin) && self.write_zint(p.period) && self.write_zint(p.duration)
        } else {
            true
        }
    }

    fn write_unit(&mut self, unit: &Unit) -> bool {
        if let Some(reply_context) = unit.reply_context.as_ref() {
            zcheck!(self.write_deco_reply_context(reply_context));
        }

        self.write(unit.header())
    }

    fn write_pull(&mut self, pull: &Pull) -> bool {
        zcheck!(self.write(pull.header()));
        zcheck!(self.write_key_expr(&pull.key));
        zcheck!(self.write_zint(pull.pull_id));
        if let Some(n) = pull.max_samples {
            zcheck!(self.write_zint(n));
        }
        true
    }

    fn write_query(&mut self, query: &Query) -> bool {
        zcheck!(self.write(query.header()));
        zcheck!(self.write_key_expr(&query.key));
        zcheck!(self.write_string(&query.value_selector));
        zcheck!(self.write_zint(query.qid));
        if let Some(t) = query.target.as_ref() {
            zcheck!(self.write_query_target(t));
        }
        self.write_consolidation(&query.consolidation)
    }

    fn write_link_state_list(&mut self, link_state_list: &LinkStateList) -> bool {
        zcheck!(self.write(link_state_list.header()));
        zcheck!(self.write_usize_as_zint(link_state_list.link_states.len()));
        for link_state in link_state_list.link_states.iter() {
            zcheck!(self.write_link_state(link_state));
        }
        true
    }

    fn write_link_state(&mut self, link_state: &LinkState) -> bool {
        zcheck!(self.write_zint(link_state.options()));
        zcheck!(self.write_zint(link_state.psid));
        zcheck!(self.write_zint(link_state.sn));
        if let Some(pid) = link_state.pid.as_ref() {
            zcheck!(self.write_peeexpr_id(pid));
        }
        if let Some(&whatami) = link_state.whatami.as_ref() {
            zcheck!(self.write_zint(whatami.into()));
        }
        if let Some(locators) = link_state.locators.as_ref() {
            zcheck!(self.write_locators(locators));
        }
        zcheck!(self.write_usize_as_zint(link_state.links.len()));
        for link in &link_state.links {
            zcheck!(self.write_zint(*link));
        }

        true
    }

    fn write_query_target(&mut self, target: &QueryTarget) -> bool {
        self.write_zint(target.kind) && self.write_target(&target.target)
    }

    fn write_target(&mut self, target: &Target) -> bool {
        // Note: desactivate Clippy check here because cast to ZInt can't be changed since ZInt size might change
        #![allow(clippy::unnecessary_cast)]
        match target {
            Target::BestMatching => self.write_zint(0 as ZInt),
            Target::All => self.write_zint(1 as ZInt),
            Target::AllComplete => self.write_zint(2 as ZInt),
            Target::None => self.write_zint(3 as ZInt),
            #[cfg(feature = "complete_n")]
            Target::Complete(n) => self.write_zint(4 as ZInt) && self.write_zint(*n),
        }
    }

    fn write_consolidation_mode(mode: ConsolidationMode) -> ZInt {
        match mode {
            ConsolidationMode::None => 0,
            ConsolidationMode::Lazy => 1,
            ConsolidationMode::Full => 2,
        }
    }

    fn write_consolidation(&mut self, consolidation: &QueryConsolidation) -> bool {
        self.write_zint(
            (WBuf::write_consolidation_mode(consolidation.first_routers) << 4)
                | (WBuf::write_consolidation_mode(consolidation.last_router) << 2)
                | (WBuf::write_consolidation_mode(consolidation.reception)),
        )
    }
}
