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
use super::super::SessionEventHandler;
use super::link::Link;
use super::proto::{
    zmsg, Data, Declaration, Declare, LinkStateList, Pull, Query, ZenohBody, ZenohMessage,
};
use super::Primitives;
use std::any::Any;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::zerror;

pub struct DeMux<P: Primitives + Send + Sync> {
    primitives: P,
}

impl<P: Primitives + Send + Sync> DeMux<P> {
    pub fn new(primitives: P) -> DeMux<P> {
        DeMux { primitives }
    }
}

impl<P: 'static + Primitives + Send + Sync> SessionEventHandler for DeMux<P> {
    fn handle_message(&self, msg: ZenohMessage) -> ZResult<()> {
        match msg.body {
            ZenohBody::Declare(Declare { declarations, .. }) => {
                for declaration in declarations {
                    match declaration {
                        Declaration::Resource { rid, key } => {
                            self.primitives.decl_resource(rid, &key);
                        }
                        Declaration::Publisher { key } => {
                            self.primitives.decl_publisher(&key, msg.routing_context);
                        }
                        Declaration::Subscriber { key, info } => {
                            self.primitives
                                .decl_subscriber(&key, &info, msg.routing_context);
                        }
                        Declaration::Queryable { key, kind } => {
                            self.primitives
                                .decl_queryable(&key, kind, msg.routing_context);
                        }
                        Declaration::ForgetResource { rid } => {
                            self.primitives.forget_resource(rid);
                        }
                        Declaration::ForgetPublisher { key } => {
                            self.primitives.forget_publisher(&key, msg.routing_context);
                        }
                        Declaration::ForgetSubscriber { key } => {
                            self.primitives.forget_subscriber(&key, msg.routing_context);
                        }
                        Declaration::ForgetQueryable { key } => {
                            self.primitives.forget_queryable(&key, msg.routing_context);
                        }
                    }
                }
            }

            ZenohBody::Data(Data {
                key,
                data_info,
                payload,
            }) => match msg.reply_context {
                None => {
                    self.primitives.send_data(
                        &key,
                        payload,
                        msg.reliability,
                        msg.congestion_control,
                        data_info,
                        msg.routing_context,
                    );
                }
                Some(rep) => match rep.replier_id {
                    Some(replier_id) => {
                        self.primitives.send_reply_data(
                            rep.qid,
                            rep.source_kind,
                            replier_id,
                            key,
                            data_info,
                            payload,
                        );
                    }
                    None => {
                        return zerror!(ZErrorKind::Other {
                            descr: "ReplyData with no replier_id".to_string()
                        })
                    }
                },
            },

            ZenohBody::Unit { .. } => {
                if let Some(rep) = msg.reply_context {
                    if rep.is_final {
                        self.primitives.send_reply_final(rep.qid);
                    }
                }
            }

            ZenohBody::Query(Query {
                key,
                predicate,
                qid,
                target,
                consolidation,
                ..
            }) => {
                self.primitives.send_query(
                    &key,
                    &predicate,
                    qid,
                    target.unwrap_or_default(),
                    consolidation,
                    msg.routing_context,
                );
            }

            ZenohBody::Pull(Pull {
                key,
                pull_id,
                max_samples,
                ..
            }) => {
                self.primitives.send_pull(
                    zmsg::has_flag(msg.header, zmsg::flag::F),
                    &key,
                    pull_id,
                    &max_samples,
                );
            }

            ZenohBody::LinkStateList(LinkStateList { .. }) => {}
        }

        Ok(())
    }

    fn new_link(&self, _link: Link) {}

    fn del_link(&self, _link: Link) {}

    fn closing(&self) {
        self.primitives.send_close();
    }

    fn closed(&self) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}
