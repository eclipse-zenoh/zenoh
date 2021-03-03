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
use super::super::super::super::routing::face::Face;
use super::link::Link;
use super::proto::{
    zmsg, Data, Declaration, Declare, LinkStateList, Pull, Query, ZenohBody, ZenohMessage,
};
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::zerror;

pub struct DeMux {
    pub(crate) primitives: Face,
}

impl DeMux {
    pub fn new(primitives: Face) -> DeMux {
        DeMux { primitives }
    }

    pub async fn handle_message(&self, msg: ZenohMessage) -> ZResult<()> {
        match msg.body {
            ZenohBody::Declare(Declare { declarations, .. }) => {
                for declaration in declarations {
                    match declaration {
                        Declaration::Resource { rid, key } => {
                            self.primitives.decl_resource(rid, &key).await;
                        }
                        Declaration::Publisher { key } => {
                            self.primitives
                                .decl_publisher(&key, msg.routing_context)
                                .await;
                        }
                        Declaration::Subscriber { key, info } => {
                            self.primitives
                                .decl_subscriber(&key, &info, msg.routing_context)
                                .await;
                        }
                        Declaration::Queryable { key } => {
                            self.primitives
                                .decl_queryable(&key, msg.routing_context)
                                .await;
                        }
                        Declaration::ForgetResource { rid } => {
                            self.primitives.forget_resource(rid).await;
                        }
                        Declaration::ForgetPublisher { key } => {
                            self.primitives
                                .forget_publisher(&key, msg.routing_context)
                                .await;
                        }
                        Declaration::ForgetSubscriber { key } => {
                            self.primitives
                                .forget_subscriber(&key, msg.routing_context)
                                .await;
                        }
                        Declaration::ForgetQueryable { key } => {
                            self.primitives
                                .forget_queryable(&key, msg.routing_context)
                                .await;
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
                    self.primitives
                        .send_data(
                            &key,
                            payload,
                            msg.reliability,
                            msg.congestion_control,
                            data_info,
                            msg.routing_context,
                        )
                        .await;
                }
                Some(rep) => match rep.replier_id {
                    Some(replier_id) => {
                        self.primitives
                            .send_reply_data(
                                rep.qid,
                                rep.source_kind,
                                replier_id,
                                key,
                                data_info,
                                payload,
                            )
                            .await
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
                        self.primitives.send_reply_final(rep.qid).await
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
                self.primitives
                    .send_query(
                        &key,
                        &predicate,
                        qid,
                        target.unwrap_or_default(),
                        consolidation,
                        msg.routing_context,
                    )
                    .await;
            }

            ZenohBody::Pull(Pull {
                key,
                pull_id,
                max_samples,
                ..
            }) => {
                self.primitives
                    .send_pull(
                        zmsg::has_flag(msg.header, zmsg::flag::F),
                        &key,
                        pull_id,
                        &max_samples,
                    )
                    .await;
            }

            ZenohBody::LinkStateList(LinkStateList { .. }) => {}
        }

        Ok(())
    }

    pub async fn new_link(&self, _link: Link) {}

    pub async fn del_link(&self, _link: Link) {}

    pub async fn closing(&self) {
        self.primitives.send_close().await;
    }

    pub async fn closed(&self) {}
}
