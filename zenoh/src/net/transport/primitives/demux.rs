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
use super::super::TransportEventHandler;
use super::protocol::proto::{
    Data, Declaration, Declare, LinkStateList, Pull, Query, Unit, ZenohBody, ZenohMessage,
};
use super::Primitives;
use crate::net::link::Link;
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

impl<P: 'static + Primitives + Send + Sync> TransportEventHandler for DeMux<P> {
    fn handle_message(&self, msg: ZenohMessage) -> ZResult<()> {
        match msg.body {
            ZenohBody::Declare(Declare { declarations, .. }) => {
                for declaration in declarations {
                    match declaration {
                        Declaration::Resource(r) => {
                            self.primitives.decl_resource(r.rid, &r.key);
                        }
                        Declaration::Publisher(p) => {
                            self.primitives.decl_publisher(&p.key, msg.routing_context);
                        }
                        Declaration::Subscriber(s) => {
                            self.primitives
                                .decl_subscriber(&s.key, &s.info, msg.routing_context);
                        }
                        Declaration::Queryable(q) => {
                            self.primitives
                                .decl_queryable(&q.key, q.kind, msg.routing_context);
                        }
                        Declaration::ForgetResource(fr) => {
                            self.primitives.forget_resource(fr.rid);
                        }
                        Declaration::ForgetPublisher(fp) => {
                            self.primitives
                                .forget_publisher(&fp.key, msg.routing_context);
                        }
                        Declaration::ForgetSubscriber(fs) => {
                            self.primitives
                                .forget_subscriber(&fs.key, msg.routing_context);
                        }
                        Declaration::ForgetQueryable(q) => {
                            self.primitives
                                .forget_queryable(&q.key, msg.routing_context);
                        }
                    }
                }
            }

            ZenohBody::Data(Data {
                key,
                data_info,
                payload,
                congestion_control,
                reply_context,
            }) => match reply_context {
                None => {
                    self.primitives.send_data(
                        &key,
                        payload,
                        msg.channel,
                        congestion_control,
                        data_info,
                        msg.routing_context,
                    );
                }
                Some(rep) => match rep.replier {
                    Some(replier) => {
                        self.primitives.send_reply_data(
                            rep.qid,
                            replier.kind,
                            replier.id,
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

            ZenohBody::Unit(Unit { reply_context, .. }) => {
                if let Some(rep) = reply_context {
                    if rep.is_final() {
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
                is_final,
            }) => {
                self.primitives
                    .send_pull(is_final, &key, pull_id, &max_samples);
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
