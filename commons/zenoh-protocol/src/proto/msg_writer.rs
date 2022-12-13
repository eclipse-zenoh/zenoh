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
use super::core::*;
use super::io::WBuf;
use super::msg::*;
use crate::io::WBufCodec;
use zenoh_buffers::buffer::{CopyBuffer, InsertBuffer};
use zenoh_core::zcheck;

pub trait MessageWriter {
    fn write_deco_attachment(&mut self, attachment: &Attachment) -> bool;
    fn write_deco_routing_context(&mut self, routing_context: &RoutingContext) -> bool;
    fn write_deco_reply_context(&mut self, reply_context: &ReplyContext) -> bool;
    fn write_deco_priority(&mut self, priority: Priority) -> bool;
    fn write_transport_message(&mut self, msg: &mut TransportMessage) -> bool;
    fn write_frame(&mut self, frame: &mut Frame) -> bool;
    fn write_scout(&mut self, scout: &Scout) -> bool;
    fn write_hello(&mut self, hello: &Hello) -> bool;
    fn write_init_syn(&mut self, init_syn: &InitSyn) -> bool;
    fn write_init_ack(&mut self, init_ack: &InitAck) -> bool;
    fn write_open_syn(&mut self, open_syn: &OpenSyn) -> bool;
    fn write_open_ack(&mut self, open_ack: &OpenAck) -> bool;
    fn write_join(&mut self, join: &Join) -> bool;
    fn write_close(&mut self, close: &Close) -> bool;
    fn write_sync(&mut self, sync: &Sync) -> bool;
    fn write_ack_nack(&mut self, ack_nack: &AckNack) -> bool;
    fn write_keep_alive(&mut self, keep_alive: &KeepAlive) -> bool;
    fn write_ping(&mut self, ping: &Ping) -> bool;
    fn write_pong(&mut self, pong: &Pong) -> bool;
    fn write_frame_header(
        &mut self,
        priority: Priority,
        reliability: Reliability,
        sn: ZInt,
        is_fragment: Option<bool>,
        attachment: Option<Attachment>,
    ) -> bool;
    fn write_zenoh_message(&mut self, msg: &mut ZenohMessage) -> bool;
    fn write_data(&mut self, data: &Data) -> bool;
    fn write_key_expr(&mut self, key: &WireExpr) -> bool;
    fn write_data_info(&mut self, info: &DataInfo) -> bool;
    fn write_queryable_info(&mut self, info: &QueryableInfo) -> bool;
    fn write_declare(&mut self, declare: &Declare) -> bool;
    fn write_declaration(&mut self, declaration: &Declaration) -> bool;
    fn write_submode(&mut self, mode: &SubMode) -> bool;
    fn write_unit(&mut self, unit: &Unit) -> bool;
    fn write_pull(&mut self, pull: &Pull) -> bool;
    fn write_query(&mut self, query: &Query) -> bool;
    fn write_link_state_list(&mut self, link_state_list: &LinkStateList) -> bool;
    fn write_link_state(&mut self, link_state: &LinkState) -> bool;
    fn write_query_target(&mut self, target: &QueryTarget) -> bool;
    fn write_consolidation_mode(mode: ConsolidationMode) -> ZInt;
    fn write_consolidation(&mut self, consolidation: ConsolidationMode) -> bool;
}

impl MessageWriter for WBuf {
    #[inline(always)]
    fn write_deco_attachment(&mut self, attachment: &Attachment) -> bool {
        zcheck!(self.write_byte(attachment.header()).is_some());
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
        zcheck!(self.write_byte(routing_context.header()).is_some());
        self.write_zint(routing_context.tree_id)
    }

    #[inline(always)]
    fn write_deco_reply_context(&mut self, reply_context: &ReplyContext) -> bool {
        zcheck!(self.write_byte(reply_context.header()).is_some());
        zcheck!(self.write_zint(reply_context.qid));
        if let Some(replier) = reply_context.replier.as_ref() {
            zcheck!(self.write_zid(&replier.id));
        }
        true
    }

    #[inline(always)]
    fn write_deco_priority(&mut self, priority: Priority) -> bool {
        self.write_byte(priority.header()).is_some()
    }

    /*************************************/
    /*            TRANSPORT              */
    /*************************************/
    #[allow(clippy::let_and_return)]
    fn write_transport_message(&mut self, msg: &mut TransportMessage) -> bool {
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

        zcheck!(self.write_byte(frame.header()).is_some());
        zcheck!(self.write_zint(frame.sn));
        match &mut frame.payload {
            FramePayload::Fragment { buffer, .. } => self.append(buffer.clone()).is_some(),
            FramePayload::Messages { messages } => {
                for m in messages.iter_mut() {
                    zcheck!(self.write_zenoh_message(m));
                }
                true
            }
        }
    }

    fn write_scout(&mut self, scout: &Scout) -> bool {
        zcheck!(self.write_byte(scout.header()).is_some());
        match scout.what {
            Some(w) => self.write_zint(w.into()),
            None => true,
        }
    }

    fn write_hello(&mut self, hello: &Hello) -> bool {
        zcheck!(self.write_byte(hello.header()).is_some());
        if let Some(zid) = hello.zid.as_ref() {
            zcheck!(self.write_zid(zid));
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
        zcheck!(self.write_byte(header).is_some());
        if init_syn.has_options() {
            zcheck!(self.write_zint(init_syn.options()));
        }
        zcheck!(self.write_byte(init_syn.version).is_some());
        zcheck!(self.write_zint(init_syn.whatami.into()));
        zcheck!(self.write_zid(&init_syn.zid));
        if imsg::has_flag(header, tmsg::flag::S) {
            zcheck!(self.write_zint(init_syn.sn_resolution));
        }
        true
    }

    fn write_init_ack(&mut self, init_ack: &InitAck) -> bool {
        zcheck!(self.write_byte(init_ack.header()).is_some());
        if init_ack.has_options() {
            zcheck!(self.write_zint(init_ack.options()));
        }
        zcheck!(self.write_zint(init_ack.whatami.into()));
        zcheck!(self.write_zid(&init_ack.zid));
        if let Some(snr) = init_ack.sn_resolution {
            zcheck!(self.write_zint(snr));
        }
        self.write_zslice_array(init_ack.cookie.clone())
    }

    fn write_open_syn(&mut self, open_syn: &OpenSyn) -> bool {
        let header = open_syn.header();
        zcheck!(self.write_byte(header).is_some());
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
        zcheck!(self.write_byte(header).is_some());
        if imsg::has_flag(header, tmsg::flag::T2) {
            zcheck!(self.write_zint(open_ack.lease.as_secs() as ZInt));
        } else {
            zcheck!(self.write_zint(open_ack.lease.as_millis() as ZInt));
        }
        self.write_zint(open_ack.initial_sn)
    }

    fn write_join(&mut self, join: &Join) -> bool {
        let header = join.header();
        zcheck!(self.write_byte(header).is_some());
        if join.has_options() {
            zcheck!(self.write_zint(join.options()));
        }
        zcheck!(self.write_byte(join.version).is_some());
        zcheck!(self.write_zint(join.whatami.into()));
        zcheck!(self.write_zid(&join.zid));
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
        zcheck!(self.write_byte(close.header()).is_some());
        if let Some(p) = close.zid.as_ref() {
            zcheck!(self.write_zid(p));
        }
        self.write_byte(close.reason).is_some()
    }

    fn write_sync(&mut self, sync: &Sync) -> bool {
        zcheck!(self.write_byte(sync.header()).is_some());
        zcheck!(self.write_zint(sync.sn));
        if let Some(c) = sync.count {
            zcheck!(self.write_zint(c));
        }
        true
    }

    fn write_ack_nack(&mut self, ack_nack: &AckNack) -> bool {
        zcheck!(self.write_byte(ack_nack.header()).is_some());
        zcheck!(self.write_zint(ack_nack.sn));
        if let Some(m) = ack_nack.mask {
            zcheck!(self.write_zint(m));
        }
        true
    }

    fn write_keep_alive(&mut self, keep_alive: &KeepAlive) -> bool {
        zcheck!(self.write_byte(keep_alive.header()).is_some());
        if let Some(p) = keep_alive.zid.as_ref() {
            zcheck!(self.write_zid(p));
        }
        true
    }

    fn write_ping(&mut self, ping: &Ping) -> bool {
        zcheck!(self.write_byte(ping.header()).is_some());
        self.write_zint(ping.hash)
    }

    fn write_pong(&mut self, pong: &Pong) -> bool {
        zcheck!(self.write_byte(pong.header()).is_some());
        self.write_zint(pong.hash)
    }

    #[inline(always)]
    fn write_frame_header(
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
        self.write_byte(header).is_some() && self.write_zint(sn)
    }

    /*************************************/
    /*              ZENOH                */
    /*************************************/
    #[allow(clippy::let_and_return)]
    fn write_zenoh_message(&mut self, msg: &mut ZenohMessage) -> bool {
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

        zcheck!(self.write_byte(data.header()).is_some());
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
    fn write_key_expr(&mut self, key: &WireExpr) -> bool {
        if key.has_suffix() {
            self.write_zint(key.scope) && self.write_string(key.suffix.as_ref())
        } else {
            self.write_zint(key.scope)
        }
    }

    #[inline(always)]
    fn write_data_info(&mut self, info: &DataInfo) -> bool {
        zcheck!(self.write_zint(info.options()));

        if info.kind != SampleKind::Put {
            zcheck!(self.write_zint(info.kind as u64));
        }
        if let Some(enc) = info.encoding.as_ref() {
            zcheck!(self.write_byte(u8::from(*enc.prefix())).is_some());
            zcheck!(self.write_string(enc.suffix()));
        }
        if let Some(ts) = info.timestamp.as_ref() {
            zcheck!(self.write_timestamp(ts));
        }
        if let Some(zid) = info.source_id.as_ref() {
            zcheck!(self.write_zid(zid));
        }
        if let Some(sn) = info.source_sn {
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
        zcheck!(self.write_byte(declare.header()).is_some());
        zcheck!(self.write_usize_as_zint(declare.declarations.len()));
        for l in declare.declarations.iter() {
            zcheck!(self.write_declaration(l));
        }
        true
    }

    fn write_declaration(&mut self, declaration: &Declaration) -> bool {
        match declaration {
            Declaration::Resource(r) => {
                self.write_byte(r.header()).is_some()
                    && self.write_zint(r.expr_id)
                    && self.write_key_expr(&r.key)
            }
            Declaration::ForgetResource(fr) => {
                self.write_byte(fr.header()).is_some() && self.write_zint(fr.expr_id)
            }
            Declaration::Subscriber(s) => {
                let header = s.header();
                zcheck!(self.write_byte(header).is_some());
                zcheck!(self.write_key_expr(&s.key));
                if imsg::has_flag(header, zmsg::flag::S) {
                    zcheck!(self.write_submode(&s.info.mode))
                }
                true
            }
            Declaration::ForgetSubscriber(fs) => {
                self.write_byte(fs.header()).is_some() && self.write_key_expr(&fs.key)
            }
            Declaration::Publisher(p) => {
                self.write_byte(p.header()).is_some() && self.write_key_expr(&p.key)
            }
            Declaration::ForgetPublisher(fp) => {
                self.write_byte(fp.header()).is_some() && self.write_key_expr(&fp.key)
            }
            Declaration::Queryable(q) => {
                let header = q.header();
                zcheck!(self.write_byte(header).is_some());
                zcheck!(self.write_key_expr(&q.key));
                if imsg::has_flag(header, zmsg::flag::Q) {
                    zcheck!(self.write_queryable_info(&q.info));
                }
                true
            }
            Declaration::ForgetQueryable(fq) => {
                self.write_byte(fq.header()).is_some() && self.write_key_expr(&fq.key)
            }
        }
    }

    fn write_submode(&mut self, mode: &SubMode) -> bool {
        let period_mask: u8 = 0;
        zcheck!(match mode {
            SubMode::Push => self
                .write_byte(zmsg::declaration::id::MODE_PUSH | period_mask)
                .is_some(),
            SubMode::Pull => self
                .write_byte(zmsg::declaration::id::MODE_PULL | period_mask)
                .is_some(),
        });
        true
    }

    fn write_unit(&mut self, unit: &Unit) -> bool {
        if let Some(reply_context) = unit.reply_context.as_ref() {
            zcheck!(self.write_deco_reply_context(reply_context));
        }

        self.write_byte(unit.header()).is_some()
    }

    fn write_pull(&mut self, pull: &Pull) -> bool {
        zcheck!(self.write_byte(pull.header()).is_some());
        zcheck!(self.write_key_expr(&pull.key));
        zcheck!(self.write_zint(pull.pull_id));
        if let Some(n) = pull.max_samples {
            zcheck!(self.write_zint(n));
        }
        true
    }

    fn write_query(&mut self, query: &Query) -> bool {
        zcheck!(self.write_byte(query.header()).is_some());
        zcheck!(self.write_key_expr(&query.key));
        zcheck!(self.write_string(&query.parameters));
        zcheck!(self.write_zint(query.qid));
        if let Some(t) = query.target.as_ref() {
            zcheck!(self.write_query_target(t));
        }
        zcheck!(self.write_consolidation(query.consolidation));
        if let Some(b) = query.body.as_ref() {
            #[allow(unused_assignments)]
            #[cfg(feature = "shared-memory")]
            let mut sliced = false;

            zcheck!(self.write_data_info(&b.data_info));
            #[cfg(feature = "shared-memory")]
            {
                sliced = b.data_info.sliced
            }

            #[cfg(feature = "shared-memory")]
            {
                self.write_zbuf(&b.payload, sliced)
            }
            #[cfg(not(feature = "shared-memory"))]
            {
                self.write_zbuf(&b.payload)
            }
        } else {
            true
        }
    }

    fn write_link_state_list(&mut self, link_state_list: &LinkStateList) -> bool {
        zcheck!(self.write_byte(link_state_list.header()).is_some());
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
        if let Some(zid) = link_state.zid.as_ref() {
            zcheck!(self.write_zid(zid));
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
        // Note: desactivate Clippy check here because cast to ZInt can't be changed since ZInt size might change
        #![allow(clippy::unnecessary_cast)]
        match target {
            QueryTarget::BestMatching => self.write_zint(0 as ZInt),
            QueryTarget::All => self.write_zint(1 as ZInt),
            QueryTarget::AllComplete => self.write_zint(2 as ZInt),
            #[cfg(feature = "complete_n")]
            QueryTarget::Complete(n) => self.write_zint(3 as ZInt) && self.write_zint(*n),
        }
    }

    fn write_consolidation_mode(mode: ConsolidationMode) -> ZInt {
        match mode {
            ConsolidationMode::None => 0,
            ConsolidationMode::Monotonic => 1,
            ConsolidationMode::Latest => 2,
        }
    }

    fn write_consolidation(&mut self, consolidation: ConsolidationMode) -> bool {
        self.write_zint(WBuf::write_consolidation_mode(consolidation))
    }
}
