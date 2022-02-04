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
use super::io::ZBuf;
use super::zenoh::*;
use super::{transport as tmsg, zenoh as zmsg, Attachment, HEADER_BITS};
use std::convert::{TryFrom, TryInto};

impl ZBuf {
    #[allow(unused_variables)]
    #[inline(always)]
    fn read_deco_attachment(&mut self, header: u8) -> Option<Attachment> {
        #[cfg(feature = "shared-memory")]
        {
            let buffer = self.read_zbuf(super::has_flag(header, tmsg::flag::Z))?;
            Some(Attachment { buffer })
        }

        #[cfg(not(feature = "shared-memory"))]
        {
            let buffer = self.read_zbuf()?;
            Some(Attachment { buffer })
        }
    }

    #[inline(always)]
    fn read_deco_priority(&mut self, header: u8) -> Option<Priority> {
        let priority: Priority = (super::flags(header) >> HEADER_BITS).try_into().ok()?;
        Some(priority)
    }

    /*************************************/
    /*              ZENOH                */
    /*************************************/
    #[inline(always)]
    fn read_deco_routing_context(&mut self, _header: u8) -> Option<RoutingContext> {
        let tree_id = self.read_zint()?;
        Some(RoutingContext { tree_id })
    }

    // @TODO: Update the ReplyContext format
    #[inline(always)]
    fn read_deco_reply_context(&mut self, header: u8) -> Option<ReplyContext> {
        let qid = self.read_zint()?;
        let replier = if super::has_flag(header, zmsg::flag::F) {
            None
        } else {
            Some(ReplierInfo {
                kind: self.read_zint()?,
                id: self.read_zenohid()?,
            })
        };

        Some(ReplyContext { qid, replier })
    }

    pub fn read_zenoh_message(&mut self, reliability: Reliability) -> Option<ZenohMessage> {
        use self::zmsg::id::*;

        #[cfg(feature = "stats")]
        let start_readable = self.readable();

        // Message decorators
        let mut priority = Priority::default();
        let mut routing_context = None;
        let mut reply_context = None;
        let mut attachment = None;

        // Read the message
        let body = loop {
            // Read the header
            let header = self.read()?;

            // Read the body
            match super::mid(header) {
                DATA => break self.read_data(header, reply_context)?,
                PRIORITY => {
                    priority = self.read_deco_priority(header)?;
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
                ROUTING_CONTEXT => {
                    routing_context = Some(self.read_deco_routing_context(header)?);
                    continue;
                }
                DECLARE => break self.read_declare(header)?,
                UNIT => break self.read_unit(header, reply_context)?,
                PULL => break self.read_pull(header)?,
                QUERY => break self.read_query(header)?,
                LINK_STATE_LIST => break self.read_link_state_list(header)?,
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
            channel: Channel {
                priority,
                reliability,
            },
            routing_context,
            attachment,
            #[cfg(feature = "stats")]
            size: std::num::NonZeroUsize::new(start_readable - stop_readable),
        })
    }

    #[inline(always)]
    fn read_data(&mut self, header: u8, reply_context: Option<ReplyContext>) -> Option<ZenohBody> {
        let congestion_control = if super::has_flag(header, zmsg::flag::D) {
            CongestionControl::Drop
        } else {
            CongestionControl::Block
        };

        let key = self.read_key_expr(super::has_flag(header, zmsg::flag::K))?;

        #[cfg(feature = "shared-memory")]
        let mut sliced = false;

        let data_info = if super::has_flag(header, zmsg::flag::I) {
            let di = self.read_data_info()?;
            #[cfg(feature = "shared-memory")]
            {
                sliced = di.sliced;
            }
            Some(di)
        } else {
            None
        };

        #[cfg(feature = "shared-memory")]
        let payload = self.read_zbuf(sliced)?;
        #[cfg(not(feature = "shared-memory"))]
        let payload = self.read_zbuf()?;

        let body = ZenohBody::Data(Data {
            key,
            data_info,
            payload,
            congestion_control,
            reply_context,
        });
        Some(body)
    }

    #[inline(always)]
    fn read_key_expr(&mut self, has_suffix: bool) -> Option<KeyExpr<'static>> {
        let id = self.read_zint()?;
        if has_suffix {
            let s = self.read_string()?;
            Some(KeyExpr {
                scope: id,
                suffix: s.into(),
            })
        } else {
            Some(KeyExpr {
                scope: id,
                suffix: "".into(),
            })
        }
    }

    #[inline(always)]
    fn read_data_info(&mut self) -> Option<DataInfo> {
        let mut info = DataInfo::new();

        let options = self.read_zint()?;
        #[cfg(feature = "shared-memory")]
        {
            info.sliced = super::has_option(options, DataInfo::OPT_SLICED);
        }
        if super::has_option(options, DataInfo::OPT_KIND) {
            info.kind = Some(self.read_zint()?);
        }
        if super::has_option(options, DataInfo::OPT_ENCODING) {
            info.encoding = Some(Encoding {
                prefix: self.read_zint()?,
                suffix: self.read_string()?.into(),
            });
        }
        if super::has_option(options, DataInfo::OPT_TIMESTAMP) {
            info.timestamp = Some(self.read_timestamp()?);
        }
        if super::has_option(options, DataInfo::OPT_SRCID) {
            info.source_id = Some(self.read_zenohid()?);
        }
        if super::has_option(options, DataInfo::OPT_SRCSN) {
            info.source_sn = Some(self.read_zint()?);
        }
        if super::has_option(options, DataInfo::OPT_RTRID) {
            info.first_router_id = Some(self.read_zenohid()?);
        }
        if super::has_option(options, DataInfo::OPT_RTRSN) {
            info.first_router_sn = Some(self.read_zint()?);
        }

        Some(info)
    }

    #[inline(always)]
    fn read_queryable_info(&mut self) -> Option<QueryableInfo> {
        let complete = self.read_zint()?;
        let distance = self.read_zint()?;
        Some(QueryableInfo { complete, distance })
    }

    fn read_unit(&mut self, header: u8, reply_context: Option<ReplyContext>) -> Option<ZenohBody> {
        let congestion_control = if super::has_flag(header, zmsg::flag::D) {
            CongestionControl::Drop
        } else {
            CongestionControl::Block
        };
        Some(ZenohBody::Unit(Unit {
            congestion_control,
            reply_context,
        }))
    }

    fn read_pull(&mut self, header: u8) -> Option<ZenohBody> {
        let key = self.read_key_expr(super::has_flag(header, zmsg::flag::K))?;
        let pull_id = self.read_zint()?;
        let max_samples = if super::has_flag(header, zmsg::flag::N) {
            Some(self.read_zint()?)
        } else {
            None
        };
        let is_final = super::has_flag(header, zmsg::flag::F);

        Some(ZenohBody::Pull(Pull {
            key,
            pull_id,
            max_samples,
            is_final,
        }))
    }

    fn read_declare(&mut self, _header: u8) -> Option<ZenohBody> {
        let declarations = self.read_declarations()?;
        Some(ZenohBody::Declare(Declare { declarations }))
    }

    fn read_declarations(&mut self) -> Option<Vec<Declaration>> {
        let len = self.read_zint()?;
        let mut vec: Vec<Declaration> = Vec::with_capacity(len as usize);
        for _ in 0..len {
            vec.push(self.read_declaration()?);
        }
        Some(vec)
    }

    fn read_declaration(&mut self) -> Option<Declaration> {
        use zmsg::declaration::id::*;

        let header = self.read()?;
        match super::mid(header) {
            RESOURCE => {
                let expr_id = self.read_zint()?;
                let key = self.read_key_expr(super::has_flag(header, zmsg::flag::K))?;
                Some(Declaration::Resource(Resource { expr_id, key }))
            }
            FORGET_RESOURCE => {
                let expr_id = self.read_zint()?;
                Some(Declaration::ForgetResource(ForgetResource { expr_id }))
            }
            SUBSCRIBER => {
                let reliability = if super::has_flag(header, zmsg::flag::R) {
                    Reliability::Reliable
                } else {
                    Reliability::BestEffort
                };
                let key = self.read_key_expr(super::has_flag(header, zmsg::flag::K))?;
                let (mode, period) = if super::has_flag(header, zmsg::flag::S) {
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
                let key = self.read_key_expr(super::has_flag(header, zmsg::flag::K))?;
                Some(Declaration::ForgetSubscriber(ForgetSubscriber { key }))
            }
            PUBLISHER => {
                let key = self.read_key_expr(super::has_flag(header, zmsg::flag::K))?;
                Some(Declaration::Publisher(Publisher { key }))
            }
            FORGET_PUBLISHER => {
                let key = self.read_key_expr(super::has_flag(header, zmsg::flag::K))?;
                Some(Declaration::ForgetPublisher(ForgetPublisher { key }))
            }
            QUERYABLE => {
                let key = self.read_key_expr(super::has_flag(header, zmsg::flag::K))?;
                let kind = self.read_zint()?;
                let info = if super::has_flag(header, zmsg::flag::Q) {
                    self.read_queryable_info()?
                } else {
                    QueryableInfo::default()
                };
                Some(Declaration::Queryable(Queryable { key, kind, info }))
            }
            FORGET_QUERYABLE => {
                let key = self.read_key_expr(super::has_flag(header, zmsg::flag::K))?;
                let kind = self.read_zint()?;
                Some(Declaration::ForgetQueryable(ForgetQueryable { key, kind }))
            }
            unknown => {
                log::trace!("Invalid ID for Declaration: {}", unknown);
                None
            }
        }
    }

    fn read_query(&mut self, header: u8) -> Option<ZenohBody> {
        let key = self.read_key_expr(super::has_flag(header, zmsg::flag::K))?;
        let value_selector = self.read_string()?;
        let qid = self.read_zint()?;
        let target = if super::has_flag(header, zmsg::flag::T) {
            Some(self.read_query_target()?)
        } else {
            None
        };
        let consolidation = self.read_consolidation()?;

        Some(ZenohBody::Query(Query {
            key,
            value_selector,
            qid,
            target,
            consolidation,
        }))
    }

    fn read_link_state_list(&mut self, _header: u8) -> Option<ZenohBody> {
        let len = self.read_zint()?;
        let mut link_states: Vec<LinkState> = Vec::with_capacity(len as usize);
        for _ in 0..len {
            link_states.push(self.read_link_state()?);
        }
        Some(ZenohBody::LinkStateList(LinkStateList { link_states }))
    }

    fn read_link_state(&mut self) -> Option<LinkState> {
        let options = self.read_zint()?;
        let psid = self.read_zint()?;
        let sn = self.read_zint()?;
        let pid = if super::has_option(options, LinkState::OPT_PID) {
            Some(self.read_zenohid()?)
        } else {
            None
        };
        let whatami = if super::has_option(options, LinkState::OPT_WAI) {
            Some(WhatAmI::try_from(self.read()?).ok()?)
        } else {
            None
        };
        let locators = if super::has_option(options, LinkState::OPT_LOC) {
            Some(self.read_locators()?)
        } else {
            None
        };
        let len = self.read_zint()?;
        let mut links: Vec<ZInt> = Vec::with_capacity(len as usize);
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
        let mode_flag = self.read()?;
        let mode: SubMode = (mode_flag & !Subscriber::PERIOD)
            .try_into()
            .map_err(|e| {
                log::trace!("{}", e);
                e
            })
            .ok()?;
        let period = if super::has_flag(mode_flag, Subscriber::PERIOD) {
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

    fn read_query_target(&mut self) -> Option<QueryTarget> {
        let kind = self.read_zint()?;
        let target = self.read_target()?;
        Some(QueryTarget { kind, target })
    }

    fn read_target(&mut self) -> Option<Target> {
        let t = self.read_zint()?;
        match t {
            0 => Some(Target::BestMatching),
            1 => Some(Target::All),
            2 => Some(Target::AllComplete),
            3 => Some(Target::None),
            #[cfg(feature = "complete_n")]
            4 => {
                let n = self.read_zint()?;
                Some(Target::Complete(n))
            }
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
            first_routers: ZBuf::read_consolidation_mode((modes >> 4) & 0x03)?,
            last_router: ZBuf::read_consolidation_mode((modes >> 2) & 0x03)?,
            reception: ZBuf::read_consolidation_mode(modes & 0x03)?,
        })
    }
}
