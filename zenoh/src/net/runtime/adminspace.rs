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
use super::plugins::PluginsMgr;
use super::protocol::{
    core::{
        queryable::EVAL, rname, CongestionControl, PeerId, QueryConsolidation, QueryTarget,
        Reliability, ResKey, SubInfo, ZInt,
    },
    io::RBuf,
    proto::{encoding, DataInfo, RoutingContext},
};
use super::routing::face::Face;
use super::routing::OutSession;
use super::Runtime;
use async_std::sync::{Arc, Mutex};
use async_std::task;
use futures::future;
use futures::future::{BoxFuture, FutureExt};
use log::{error, trace};
use serde_json::json;
use std::collections::HashMap;

pub struct AdminContext {
    runtime: Runtime,
    plugins_mgr: PluginsMgr,
    pid_str: String,
    version: String,
}

type Handler = Box<dyn Fn(&AdminContext) -> BoxFuture<'_, (RBuf, ZInt)> + Send + Sync>;

pub struct AdminSpace {
    pid: PeerId,
    primitives: Mutex<Option<Arc<Face>>>,
    mappings: Mutex<HashMap<ZInt, String>>,
    handlers: HashMap<String, Arc<Handler>>,
    context: Arc<AdminContext>,
}

impl AdminSpace {
    pub async fn start(runtime: &Runtime, plugins_mgr: PluginsMgr, version: String) {
        let pid_str = runtime.get_pid_str().await;
        let root_path = format!("/@/router/{}", pid_str);

        let mut handlers: HashMap<String, Arc<Handler>> = HashMap::new();
        handlers.insert(
            root_path.clone(),
            Arc::new(Box::new(|context| router_data(context).boxed())),
        );
        handlers.insert(
            [&root_path, "/linkstate/routers"].concat(),
            Arc::new(Box::new(|context| linkstate_routers_data(context).boxed())),
        );
        handlers.insert(
            [&root_path, "/linkstate/peers"].concat(),
            Arc::new(Box::new(|context| linkstate_peers_data(context).boxed())),
        );
        let context = Arc::new(AdminContext {
            runtime: runtime.clone(),
            plugins_mgr,
            pid_str,
            version,
        });
        let admin = Arc::new(AdminSpace {
            pid: runtime.read().await.pid.clone(),
            primitives: Mutex::new(None),
            mappings: Mutex::new(HashMap::new()),
            handlers,
            context,
        });

        let primitives = runtime
            .read()
            .await
            .router
            .new_primitives(OutSession::Admin(admin.clone()))
            .await;
        admin.primitives.lock().await.replace(primitives.clone());

        primitives
            .decl_queryable(&[&root_path, "/**"].concat().into(), None)
            .await;
    }

    pub async fn reskey_to_string(&self, key: &ResKey) -> Option<String> {
        match key {
            ResKey::RId(id) => self.mappings.lock().await.get(&id).cloned(),
            ResKey::RIdWithSuffix(id, suffix) => self
                .mappings
                .lock()
                .await
                .get(&id)
                .map(|prefix| format!("{}{}", prefix, suffix)),
            ResKey::RName(name) => Some(name.clone()),
        }
    }

    pub(crate) async fn decl_resource(&self, rid: ZInt, reskey: &ResKey) {
        trace!("recv Resource {} {:?}", rid, reskey);
        match self.reskey_to_string(reskey).await {
            Some(s) => {
                self.mappings.lock().await.insert(rid, s);
            }
            None => error!("Unknown rid {}!", rid),
        }
    }

    pub(crate) async fn forget_resource(&self, _rid: ZInt) {
        trace!("recv Forget Resource {}", _rid);
    }

    pub(crate) async fn decl_publisher(
        &self,
        _reskey: &ResKey,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!("recv Publisher {:?}", _reskey);
    }

    pub(crate) async fn forget_publisher(
        &self,
        _reskey: &ResKey,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!("recv Forget Publisher {:?}", _reskey);
    }

    pub(crate) async fn decl_subscriber(
        &self,
        _reskey: &ResKey,
        _sub_info: &SubInfo,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!("recv Subscriber {:?} , {:?}", _reskey, _sub_info);
    }

    pub(crate) async fn forget_subscriber(
        &self,
        _reskey: &ResKey,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!("recv Forget Subscriber {:?}", _reskey);
    }

    pub(crate) async fn decl_queryable(
        &self,
        _reskey: &ResKey,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!("recv Queryable {:?}", _reskey);
    }

    pub(crate) async fn forget_queryable(
        &self,
        _reskey: &ResKey,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!("recv Forget Queryable {:?}", _reskey);
    }

    pub(crate) async fn send_data(
        &self,
        reskey: &ResKey,
        payload: RBuf,
        reliability: Reliability,
        congestion_control: CongestionControl,
        data_info: Option<DataInfo>,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!(
            "recv Data {:?} {:?} {:?} {:?} {:?}",
            reskey,
            payload,
            reliability,
            congestion_control,
            data_info,
        );
    }

    pub(crate) async fn send_query(
        &self,
        reskey: &ResKey,
        predicate: &str,
        qid: ZInt,
        target: QueryTarget,
        _consolidation: QueryConsolidation,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!(
            "recv Query {:?} {:?} {:?} {:?}",
            reskey,
            predicate,
            target,
            _consolidation
        );
        let pid = self.pid.clone();
        let context = self.context.clone();
        let primitives = self.primitives.lock().await.as_ref().unwrap().clone();

        let mut matching_handlers = vec![];
        match self.reskey_to_string(reskey).await {
            Some(name) => {
                for (path, handler) in &self.handlers {
                    if rname::intersect(&name, path) {
                        matching_handlers.push((path.clone(), handler.clone()));
                    }
                }
            }
            None => error!("Unknown ResKey!!"),
        };

        // router is not re-entrant
        task::spawn(async move {
            for (path, handler) in matching_handlers {
                let (payload, encoding) = handler(&context).await;
                let data_info = DataInfo {
                    source_id: None,
                    source_sn: None,
                    first_router_id: None,
                    first_router_sn: None,
                    timestamp: None,
                    kind: None,
                    encoding: Some(encoding),
                };
                primitives
                    .send_reply_data(
                        qid,
                        EVAL,
                        pid.clone(),
                        ResKey::RName(path),
                        Some(data_info),
                        payload,
                    )
                    .await;
            }

            primitives.send_reply_final(qid).await;
        });
    }

    pub(crate) async fn send_reply_data(
        &self,
        qid: ZInt,
        source_kind: ZInt,
        replier_id: PeerId,
        reskey: ResKey,
        info: Option<DataInfo>,
        payload: RBuf,
    ) {
        trace!(
            "recv ReplyData {:?} {:?} {:?} {:?} {:?} {:?}",
            qid,
            source_kind,
            replier_id,
            reskey,
            info,
            payload
        );
    }

    pub(crate) async fn send_reply_final(&self, qid: ZInt) {
        trace!("recv ReplyFinal {:?}", qid);
    }

    pub(crate) async fn send_pull(
        &self,
        _is_final: bool,
        _reskey: &ResKey,
        _pull_id: ZInt,
        _max_samples: &Option<ZInt>,
    ) {
        trace!(
            "recv Pull {:?} {:?} {:?} {:?}",
            _is_final,
            _reskey,
            _pull_id,
            _max_samples
        );
    }

    pub(crate) async fn send_close(&self) {
        trace!("recv Close");
    }
}

pub async fn router_data(context: &AdminContext) -> (RBuf, ZInt) {
    let session_mgr = context
        .runtime
        .read()
        .await
        .orchestrator
        .manager()
        .await
        .clone();

    // plugins info
    let plugins: Vec<serde_json::Value> = context
        .plugins_mgr
        .plugins
        .iter()
        .map(|plugin| {
            json!({
                "name": plugin.name,
                "path": plugin.path
            })
        })
        .collect();

    // locators info
    let locators: Vec<serde_json::Value> = session_mgr
        .get_locators()
        .await
        .iter()
        .map(|locator| json!(locator.to_string()))
        .collect();

    // sessions info
    let sessions = future::join_all(session_mgr.get_sessions().await.iter().map(async move |session|
        json!({
            "peer": session.get_pid().map_or_else(|_| "unavailable".to_string(), |p| p.to_string()),
            "links": session.get_links().await.map_or_else(
                |_| vec!(),
                |links| links.iter().map(|link| link.get_dst().to_string()).collect()
            )
        })
    )).await;

    let json = json!({
        "pid": context.pid_str,
        "version": context.version,
        "locators": locators,
        "sessions": sessions,
        "plugins": plugins,
    });
    log::trace!("AdminSpace router_data: {:?}", json);
    (RBuf::from(json.to_string().as_bytes()), encoding::APP_JSON)
}

pub async fn linkstate_routers_data(context: &AdminContext) -> (RBuf, ZInt) {
    let runtime = &context.runtime.read().await;
    let tables = runtime.router.tables.read().await;

    let res = (
        RBuf::from(tables.routers_net.as_ref().unwrap().dot().as_bytes()),
        encoding::TEXT_PLAIN,
    );
    res
}

pub async fn linkstate_peers_data(context: &AdminContext) -> (RBuf, ZInt) {
    (
        RBuf::from(
            context
                .runtime
                .read()
                .await
                .router
                .tables
                .read()
                .await
                .peers_net
                .as_ref()
                .unwrap()
                .dot()
                .as_bytes(),
        ),
        encoding::TEXT_PLAIN,
    )
}
