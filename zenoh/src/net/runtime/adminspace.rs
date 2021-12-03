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
use super::protocol::{
    core::{
        key_expr, queryable::EVAL, Channel, CongestionControl, Encoding, KeyExpr, PeerId,
        QueryConsolidation, QueryTarget, QueryableInfo, SubInfo, ZInt, EMPTY_EXPR_ID,
    },
    io::ZBuf,
    proto::{DataInfo, RoutingContext},
};
use super::routing::face::Face;
use super::transport::{Primitives, TransportUnicast};
use super::Runtime;
use async_std::sync::Arc;
use async_std::task;
use futures::future::{BoxFuture, FutureExt};
use log::{error, trace};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Mutex;
type PluginsHandles = zenoh_plugin_trait::loading::PluginsHandles<super::plugins::StartArgs>;

pub struct AdminContext {
    runtime: Runtime,
    plugins_mgr: PluginsHandles,
    pid_str: String,
    version: String,
}

type Handler = Box<
    dyn for<'a> Fn(&'a AdminContext, &'a KeyExpr<'a>, &'a str) -> BoxFuture<'a, (ZBuf, Encoding)>
        + Send
        + Sync,
>;

pub struct AdminSpace {
    pid: PeerId,
    primitives: Mutex<Option<Arc<Face>>>,
    mappings: Mutex<HashMap<ZInt, String>>,
    handlers: HashMap<String, Arc<Handler>>,
    context: Arc<AdminContext>,
}

impl AdminSpace {
    pub async fn start(runtime: &Runtime, plugins_mgr: PluginsHandles, version: String) {
        let pid_str = runtime.get_pid_str();
        let root_key = format!("/@/router/{}", pid_str);

        let mut handlers: HashMap<String, Arc<Handler>> = HashMap::new();
        handlers.insert(
            root_key.clone(),
            Arc::new(Box::new(|context, key, args| {
                router_data(context, key, args).boxed()
            })),
        );
        handlers.insert(
            [&root_key, "/linkstate/routers"].concat(),
            Arc::new(Box::new(|context, key, args| {
                linkstate_routers_data(context, key, args).boxed()
            })),
        );
        handlers.insert(
            [&root_key, "/linkstate/peers"].concat(),
            Arc::new(Box::new(|context, key, args| {
                linkstate_peers_data(context, key, args).boxed()
            })),
        );
        let context = Arc::new(AdminContext {
            runtime: runtime.clone(),
            plugins_mgr,
            pid_str,
            version,
        });
        let admin = Arc::new(AdminSpace {
            pid: runtime.pid,
            primitives: Mutex::new(None),
            mappings: Mutex::new(HashMap::new()),
            handlers,
            context,
        });

        let primitives = runtime.router.new_primitives(admin.clone());
        zlock!(admin.primitives).replace(primitives.clone());

        primitives.decl_queryable(
            &[&root_key, "/**"].concat().into(),
            EVAL,
            &QueryableInfo {
                complete: 0,
                distance: 0,
            },
            None,
        );

        primitives.decl_subscriber(
            &[&root_key, "/config/**"].concat().into(),
            &SubInfo::default(),
            None,
        );
    }

    pub fn key_expr_to_string(&self, key_expr: &KeyExpr) -> Option<String> {
        if key_expr.scope == EMPTY_EXPR_ID {
            Some(key_expr.suffix.to_string())
        } else if key_expr.suffix.is_empty() {
            zlock!(self.mappings).get(&key_expr.scope).cloned()
        } else {
            zlock!(self.mappings)
                .get(&key_expr.scope)
                .map(|prefix| format!("{}{}", prefix, key_expr.suffix.as_ref()))
        }
    }
}

impl Primitives for AdminSpace {
    fn decl_resource(&self, expr_id: ZInt, key_expr: &KeyExpr) {
        trace!("recv Resource {} {:?}", expr_id, key_expr);
        match self.key_expr_to_string(key_expr) {
            Some(s) => {
                zlock!(self.mappings).insert(expr_id, s);
            }
            None => error!("Unknown expr_id {}!", expr_id),
        }
    }

    fn forget_resource(&self, _expr_id: ZInt) {
        trace!("recv Forget Resource {}", _expr_id);
    }

    fn decl_publisher(&self, _key_expr: &KeyExpr, _routing_context: Option<RoutingContext>) {
        trace!("recv Publisher {:?}", _key_expr);
    }

    fn forget_publisher(&self, _key_expr: &KeyExpr, _routing_context: Option<RoutingContext>) {
        trace!("recv Forget Publisher {:?}", _key_expr);
    }

    fn decl_subscriber(
        &self,
        _key_expr: &KeyExpr,
        _sub_info: &SubInfo,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!("recv Subscriber {:?} , {:?}", _key_expr, _sub_info);
    }

    fn forget_subscriber(&self, _key_expr: &KeyExpr, _routing_context: Option<RoutingContext>) {
        trace!("recv Forget Subscriber {:?}", _key_expr);
    }

    fn decl_queryable(
        &self,
        _key_expr: &KeyExpr,
        _kind: ZInt,
        _qabl_info: &QueryableInfo,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!("recv Queryable {:?}", _key_expr);
    }

    fn forget_queryable(
        &self,
        _key_expr: &KeyExpr,
        _kind: ZInt,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!("recv Forget Queryable {:?}", _key_expr);
    }

    fn send_data(
        &self,
        key_expr: &KeyExpr,
        payload: ZBuf,
        channel: Channel,
        congestion_control: CongestionControl,
        data_info: Option<DataInfo>,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!(
            "recv Data {:?} {:?} {:?} {:?} {:?}",
            key_expr,
            payload,
            channel,
            congestion_control,
            data_info,
        );

        if let Some(key) = key_expr
            .as_str()
            .strip_prefix(&format!("/@/router/{}/config/", &self.context.pid_str))
        {
            match std::str::from_utf8(payload.contiguous().as_slice()) {
                Ok(json) => {
                    log::trace!(
                        "Insert conf value /@/router/{}/config/{}:{}",
                        &self.context.pid_str,
                        key,
                        json
                    );
                    if let Err(e) = self.context.runtime.config.insert_json5(key, json) {
                        error!(
                            "Error inserting conf value /@/router/{}/config/{}:{} - {}",
                            &self.context.pid_str, key, json, e
                        );
                    }
                }
                Err(e) => error!(
                    "Received non utf8 conf value on /@/router/{}/config/{} : {}",
                    &self.context.pid_str, key, e
                ),
            }
        }
    }

    fn send_query(
        &self,
        key_expr: &KeyExpr,
        value_selector: &str,
        qid: ZInt,
        target: QueryTarget,
        _consolidation: QueryConsolidation,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!(
            "recv Query {:?} {:?} {:?} {:?}",
            key_expr,
            value_selector,
            target,
            _consolidation
        );
        let pid = self.pid;
        let context = self.context.clone();
        let primitives = zlock!(self.primitives).as_ref().unwrap().clone();

        let mut matching_handlers = vec![];
        match self.key_expr_to_string(key_expr) {
            Some(name) => {
                for (key, handler) in &self.handlers {
                    if key_expr::intersect(&name, key) {
                        matching_handlers.push((key.clone(), handler.clone()));
                    }
                }
            }
            None => error!("Unknown KeyExpr!!"),
        };

        let key_expr = key_expr.to_owned();
        let value_selector = value_selector.to_string();

        // router is not re-entrant
        task::spawn(async move {
            for (key, handler) in matching_handlers {
                let (payload, encoding) = handler(&context, &key_expr, &value_selector).await;
                let mut data_info = DataInfo::new();
                data_info.encoding = Some(encoding);

                primitives.send_reply_data(qid, EVAL, pid, key.into(), Some(data_info), payload);
            }

            primitives.send_reply_final(qid);
        });
    }

    fn send_reply_data(
        &self,
        qid: ZInt,
        replier_kind: ZInt,
        replier_id: PeerId,
        key_expr: KeyExpr,
        info: Option<DataInfo>,
        payload: ZBuf,
    ) {
        trace!(
            "recv ReplyData {:?} {:?} {:?} {:?} {:?} {:?}",
            qid,
            replier_kind,
            replier_id,
            key_expr,
            info,
            payload
        );
    }

    fn send_reply_final(&self, qid: ZInt) {
        trace!("recv ReplyFinal {:?}", qid);
    }

    fn send_pull(
        &self,
        _is_final: bool,
        _key_expr: &KeyExpr,
        _pull_id: ZInt,
        _max_samples: &Option<ZInt>,
    ) {
        trace!(
            "recv Pull {:?} {:?} {:?} {:?}",
            _is_final,
            _key_expr,
            _pull_id,
            _max_samples
        );
    }

    fn send_close(&self) {
        trace!("recv Close");
    }
}

pub async fn router_data(
    context: &AdminContext,
    _key: &KeyExpr<'_>,
    #[allow(unused_variables)] selector: &str,
) -> (ZBuf, Encoding) {
    let transport_mgr = context.runtime.manager().clone();

    // plugins info
    let plugins: Vec<serde_json::Value> = context
        .plugins_mgr
        .plugins()
        .iter()
        .map(|plugin| {
            json!({
                "name": plugin.name,
                "path": plugin.path
            })
        })
        .collect();

    // locators info
    let locators: Vec<serde_json::Value> = transport_mgr
        .get_locators()
        .iter()
        .map(|locator| json!(locator.to_string()))
        .collect();

    // transports info
    let transport_to_json = |transport: &TransportUnicast| {
        #[allow(unused_mut)]
        let mut json = json!({
            "peer": transport.get_pid().map_or_else(|_| "unknown".to_string(), |p| p.to_string()),
            "whatami": transport.get_whatami().map_or_else(|_| "unknown".to_string(), |p| p.to_string()),
            "links": transport.get_links().map_or_else(
                |_| Vec::new(),
                |links| links.iter().map(|link| link.dst.to_string()).collect()
            ),
        });
        #[cfg(feature = "stats")]
        {
            use std::convert::TryFrom;
            let stats = crate::prelude::ValueSelector::try_from(selector)
                .ok()
                .map(|s| s.properties.get("stats").map(|v| v == "true"))
                .flatten()
                .unwrap_or(false);
            if stats {
                json.as_object_mut().unwrap().insert(
                    "stats".to_string(),
                    transport
                        .get_stats()
                        .map_or_else(|_| json!({}), |p| json!(p)),
                );
            }
        }
        json
    };
    let transports: Vec<serde_json::Value> = transport_mgr
        .get_transports()
        .iter()
        .map(transport_to_json)
        .collect();

    let json = json!({
        "pid": context.pid_str,
        "version": context.version,
        "locators": locators,
        "sessions": transports,
        "plugins": plugins,
    });
    log::trace!("AdminSpace router_data: {:?}", json);
    (
        ZBuf::from(json.to_string().as_bytes().to_vec()),
        Encoding::APP_JSON,
    )
}

pub async fn linkstate_routers_data(
    context: &AdminContext,
    _key: &KeyExpr<'_>,
    _args: &str,
) -> (ZBuf, Encoding) {
    let tables = zread!(context.runtime.router.tables);

    let res = (
        ZBuf::from(
            tables
                .routers_net
                .as_ref()
                .unwrap()
                .dot()
                .as_bytes()
                .to_vec(),
        ),
        Encoding::TEXT_PLAIN,
    );
    res
}

pub async fn linkstate_peers_data(
    context: &AdminContext,
    _key: &KeyExpr<'_>,
    _args: &str,
) -> (ZBuf, Encoding) {
    (
        ZBuf::from(
            context
                .runtime
                .router
                .tables
                .read()
                .unwrap()
                .peers_net
                .as_ref()
                .unwrap()
                .dot()
                .as_bytes()
                .to_vec(),
        ),
        Encoding::TEXT_PLAIN,
    )
}
