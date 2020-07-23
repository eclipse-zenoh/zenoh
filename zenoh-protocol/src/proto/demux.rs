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
use async_trait::async_trait;

use crate::proto::{ZenohMessage, ZenohBody, Declare, Data, Query, Pull, Declaration, Primitives, zmsg};
use crate::session::MsgHandler;
use zenoh_util::zerror;
use zenoh_util::core::{ZResult, ZError, ZErrorKind};

pub struct DeMux<P: Primitives + Send + Sync> {
    primitives: P,
}

impl<P: Primitives + Send + Sync> DeMux<P> {
    pub fn new(primitives: P) -> DeMux<P> {
        DeMux {primitives,}
    }
}

#[async_trait]
impl<P: Primitives + Send + Sync> MsgHandler for DeMux<P> {

    async fn handle_message(&self, msg: ZenohMessage) -> ZResult<()> {
        let reliability = msg.is_reliable();
        match msg.body {
            ZenohBody::Declare(Declare{ declarations, .. }) => {
                for declaration in declarations {
                    match declaration {
                        Declaration::Resource { rid, key } => {
                            self.primitives.resource(rid, &key).await;
                        }
                        Declaration::Publisher { key } => {
                            self.primitives.publisher(&key).await;
                        }
                        Declaration::Subscriber { key, info } => {
                            self.primitives.subscriber(&key, &info).await;
                        }
                        Declaration::Queryable { key } => {
                            self.primitives.queryable(&key).await;
                        }
                        Declaration::ForgetResource { rid } => {
                            self.primitives.forget_resource(rid).await;
                        }
                        Declaration::ForgetPublisher { key } => {
                            self.primitives.forget_publisher(&key).await;
                        }
                        Declaration::ForgetSubscriber { key } => {
                            self.primitives.forget_subscriber(&key).await;
                        }
                        Declaration::ForgetQueryable { key } => {
                            self.primitives.forget_queryable(&key).await;
                        }
                    }

                }
            },
            
            ZenohBody::Data(Data{ key, info, payload, .. }) => {
                match msg.reply_context {
                    None => {
                        self.primitives.data(&key, reliability, &info, payload).await;
                    }
                    Some(rep) => {
                        match rep.replier_id {
                            Some(replier_id) => {
                                self.primitives.reply_data(rep.qid, rep.source_kind, replier_id, key, info, payload).await
                            }
                            None => return zerror!(ZErrorKind::Other {descr: "ReplyData with no replier_id".to_string()})
                        }
                    }
                }
            },

            ZenohBody::Unit { .. } => {
                if let Some(rep) = msg.reply_context {
                    if rep.is_final {
                        self.primitives.reply_final(rep.qid).await
                    }
                }
            },

            ZenohBody::Query(Query{ key, predicate, qid, target, consolidation, .. }) => {
                self.primitives.query(&key, &predicate, qid, target.unwrap_or_default(), consolidation).await;
            },

            ZenohBody::Pull(Pull{ key, pull_id, max_samples, .. }) => {
                self.primitives.pull(zmsg::has_flag(msg.header, zmsg::flag::F), &key, pull_id, &max_samples).await;
            }
        }

        Ok(())
    }

    async fn close(&self) {
        self.primitives.close().await;
    }
}