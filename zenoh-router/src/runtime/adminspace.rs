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
use async_std::sync::{Arc, Mutex};
use async_std::task;
use async_trait::async_trait;
use futures::future;
use log::trace;
use zenoh_protocol:: {
    core::{ ResKey, ZInt },
    io::RBuf,
    proto::{ Primitives, QueryTarget, QueryConsolidation, Reply, SubInfo },
    proto::queryable::EVAL
};
use super::Runtime;
use crate::plugins::PluginsMgr;
use serde_json::json;

pub struct AdminSpace {
    runtime: Runtime,
    plugins_mgr: PluginsMgr,
    primitives: Mutex<Option<Arc<dyn Primitives + Send + Sync>>>,
    pid_str: String,
    router_path: String
}

impl AdminSpace {

    pub async fn start(runtime: &Runtime, plugins_mgr: PluginsMgr) {
        let rt = runtime.read().await;
        let pid_str = rt.pid.to_string();
        let router_path = format!("/@/router/{}", pid_str);

        let admin = Arc::new(AdminSpace { 
            runtime: runtime.clone(),
            plugins_mgr,
            primitives: Mutex::new(None),
            pid_str,
            router_path });

        let primitives = rt.broker.new_primitives(admin.clone()).await;
        admin.primitives.lock().await.replace(primitives.clone());

        // declare queryable on router_path
        primitives.queryable(&admin.router_path.clone().into()).await;
    }

    pub async fn create_reply_payload(&self) -> RBuf {
        let session_mgr = &self.runtime.read().await.orchestrator.manager;

        // plugins info
        let plugins: Vec<serde_json::Value> = self.plugins_mgr.plugins.iter().map(|plugin|
            json!({
                "name": plugin.name,
                "path": plugin.path
            })
        ).collect();

        // locators info
        let locators: Vec<serde_json::Value> = session_mgr.get_locators().await.iter().map(|locator|
            json!(locator.to_string())
        ).collect();

        // sessions info
        let sessions = future::join_all(session_mgr.get_sessions().await.iter().map(async move |session|
            json!({
                "peer": session.get_peer().map_or_else(|_| "unavailable".to_string(), |p| p.to_string()),
                "links": session.get_links().await.map_or_else(
                    |_| vec!(),
                    |links| links.iter().map(|link| link.get_dst().to_string()).collect()
                )
            })
        )).await;

        let json = json!({
            "pid": self.pid_str,
            "locators": locators,
            "sessions": sessions,
            "plugins": plugins,
        });
        log::debug!("JSON: {:?}", json);
        RBuf::from(json.to_string().as_bytes())
    }

}


#[async_trait]
impl Primitives for AdminSpace {

    async fn resource(&self, rid: ZInt, reskey: &ResKey) {
        trace!("recv Resource {} {:?}", rid, reskey);
    }

    async fn forget_resource(&self, _rid: ZInt) {
        trace!("recv Forget Resource {}", _rid);
    }

    async fn publisher(&self, _reskey: &ResKey) {
        trace!("recv Publisher {:?}", _reskey);
    }

    async fn forget_publisher(&self, _reskey: &ResKey) {
        trace!("recv Forget Publisher {:?}", _reskey);
    }

    async fn subscriber(&self, _reskey: &ResKey, _sub_info: &SubInfo) {
        trace!("recv Subscriber {:?} , {:?}", _reskey, _sub_info);
    }

    async fn forget_subscriber(&self, _reskey: &ResKey) {
        trace!("recv Forget Subscriber {:?}", _reskey);
    }

    async fn queryable(&self, _reskey: &ResKey) {
        trace!("recv Queryable {:?}", _reskey);
    }

    async fn forget_queryable(&self, _reskey: &ResKey) {
        trace!("recv Forget Queryable {:?}", _reskey);
    }

    async fn data(&self, reskey: &ResKey, _reliable: bool, info: &Option<RBuf>, payload: RBuf) {
        trace!("recv Data {:?} {:?} {:?} {:?}", reskey, _reliable, info, payload);
    }

    async fn query(&self, reskey: &ResKey, predicate: &str, qid: ZInt, target: QueryTarget, _consolidation: QueryConsolidation) {
        trace!("recv Query {:?} {:?} {:?} {:?}", reskey, predicate, target, _consolidation);
        let payload = self.create_reply_payload().await;

        // The following are cloned to be moved in the task below
        // (could be removed if the lock used in router (face::tables) is re-entrant)
        let primitives = self.primitives.lock().await.as_ref().unwrap().clone();
        let reskey = ResKey::RName(self.router_path.clone());
        let replier_id = self.runtime.read().await.pid.clone();   // @TODO build/use prebuilt specific pid

        task::spawn( async move { // router is not re-entrant
            primitives.reply(qid, Reply::ReplyData {
                source_kind: EVAL, 
                replier_id: replier_id.clone(),
                reskey, 
                info: None,
                payload,
            }).await;

            primitives.reply(qid, Reply::SourceFinal {
                source_kind: EVAL, 
                replier_id,
            }).await;
            primitives.reply(qid, Reply::ReplyFinal).await;
        });
    }

    async fn reply(&self, qid: ZInt, reply: Reply) {
        trace!("recv Reply {:?} {:?}", qid, reply);
    }

    async fn pull(&self, _is_final: bool, _reskey: &ResKey, _pull_id: ZInt, _max_samples: &Option<ZInt>) {
        trace!("recv Pull {:?} {:?} {:?} {:?}", _is_final, _reskey, _pull_id, _max_samples);
    }

    async fn close(&self) {
        trace!("recv Close");
    }

}