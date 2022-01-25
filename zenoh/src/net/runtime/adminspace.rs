use crate::{net::protocol::message::data_kind, prelude::Selector};

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
        key_expr, queryable::EVAL, Channel, CongestionControl, Encoding, KeyExpr,
        QueryConsolidation, QueryTarget, QueryableInfo, SubInfo, ZInt, ZenohId, EMPTY_EXPR_ID,
    },
    io::ZBuf,
    message::{DataInfo, RoutingContext},
};
use super::routing::face::Face;
use super::transport::{Primitives, TransportUnicast};
use super::Runtime;
use crate::plugins::PluginsManager;
use async_std::sync::Arc;
use async_std::task;
use futures::future::{BoxFuture, FutureExt};
use log::{error, trace};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Mutex;

pub struct AdminContext {
    runtime: Runtime,
    plugins_mgr: Mutex<PluginsManager>,
    pid_str: String,
    version: String,
}

type Handler = Box<
    dyn for<'a> Fn(&'a AdminContext, &'a KeyExpr<'a>, &'a str) -> BoxFuture<'a, (ZBuf, Encoding)>
        + Send
        + Sync,
>;

pub struct AdminSpace {
    pid: ZenohId,
    primitives: Mutex<Option<Arc<Face>>>,
    mappings: Mutex<HashMap<ZInt, String>>,
    handlers: HashMap<String, Arc<Handler>>,
    context: Arc<AdminContext>,
}

#[derive(Debug, Clone)]
enum PluginDiff {
    Delete(String),
    Start(crate::config::PluginLoad),
}

impl AdminSpace {
    pub async fn start(runtime: &Runtime, plugins_mgr: PluginsManager, version: String) {
        let pid_str = runtime.get_zid_str();
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

        let mut active_plugins = plugins_mgr
            .running_plugins_info()
            .into_iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect::<HashMap<_, _>>();

        let context = Arc::new(AdminContext {
            runtime: runtime.clone(),
            plugins_mgr: Mutex::new(plugins_mgr),
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

        let cfg_rx = admin.context.runtime.config.subscribe();
        task::spawn({
            let admin = admin.clone();
            async move {
                while let Ok(change) = cfg_rx.recv_async().await {
                    let change = change.strip_prefix('/').unwrap_or(&change);
                    if !change.starts_with("plugins") {
                        continue;
                    }

                    let requested_plugins = {
                        let cfg_guard = admin.context.runtime.config.lock();
                        cfg_guard.plugins().load_requests().collect::<Vec<_>>()
                    };
                    let mut diffs = Vec::new();
                    for plugin in active_plugins.keys() {
                        if !requested_plugins.iter().any(|r| &r.name == plugin) {
                            diffs.push(PluginDiff::Delete(plugin.clone()))
                        }
                    }
                    for request in requested_plugins {
                        if let Some(active) = active_plugins.get(&request.name) {
                            if request
                                .paths
                                .as_ref()
                                .map(|p| p.contains(active))
                                .unwrap_or(true)
                            {
                                continue;
                            }
                            diffs.push(PluginDiff::Delete(request.name.clone()))
                        }
                        diffs.push(PluginDiff::Start(request))
                    }
                    let mut plugins_mgr = zlock!(admin.context.plugins_mgr);
                    for diff in diffs {
                        match diff {
                            PluginDiff::Delete(plugin) => {
                                active_plugins.remove(plugin.as_str());
                                plugins_mgr.stop(&plugin);
                            }
                            PluginDiff::Start(plugin) => {
                                let load = match &plugin.paths {
                                    Some(paths) => {
                                        plugins_mgr.load_plugin_by_paths(plugin.name.clone(), paths)
                                    }
                                    None => plugins_mgr.load_plugin_by_name(plugin.name.clone()),
                                };
                                match load {
                                    Err(e) => {
                                        if plugin.required {
                                            panic!("Failed to load plugin `{}`: {}", plugin.name, e)
                                        } else {
                                            log::error!(
                                                "Failed to load plugin `{}`: {}",
                                                plugin.name,
                                                e
                                            )
                                        }
                                    }
                                    Ok(path) => {
                                        let name = &plugin.name;
                                        log::info!("Loaded plugin `{}` from {}", name, &path);
                                        match plugins_mgr.start(name, &admin.context.runtime) {
                                            Ok(Some((path, plugin))) => {
                                                active_plugins.insert(name.into(), path.into());
                                                let mut cfg_guard =
                                                    admin.context.runtime.config.lock();
                                                cfg_guard.add_plugin_validator(
                                                    name,
                                                    plugin.config_checker(),
                                                );
                                                log::info!(
                                                    "Successfully started plugin `{}` from {}",
                                                    name,
                                                    path
                                                );
                                            }
                                            Ok(None) => {
                                                log::warn!("Plugin `{}` was already running", name)
                                            }
                                            Err(e) => log::error!("{}", e),
                                        }
                                    }
                                }
                            }
                        }
                    }
                    log::info!("Running plugins: {:?}", &active_plugins)
                }
            }
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
            if let Some(DataInfo {
                kind: Some(data_kind::DELETE),
                ..
            }) = data_info
            {
                log::trace!(
                    "Deleting conf value /@/router/{}/config/{}",
                    &self.context.pid_str,
                    key
                );
                if let Err(e) = self.context.runtime.config.remove(key) {
                    log::error!("Error deleting conf value {}: {}", key_expr, e)
                }
            } else {
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
        let plugin_key = format!("/@/router/{}/status/plugins/**", &pid);
        let mut ask_plugins = false;
        let context = self.context.clone();
        let primitives = zlock!(self.primitives).as_ref().unwrap().clone();

        let mut matching_handlers = vec![];
        match self.key_expr_to_string(key_expr) {
            Some(name) => {
                ask_plugins = key_expr::intersect(&name, &plugin_key);
                for (key, handler) in &self.handlers {
                    if key_expr::intersect(&name, key) {
                        matching_handlers.push((key.clone(), handler.clone()));
                    }
                }
            }
            None => log::error!("Unknown KeyExpr!!"),
        };

        let key_expr = key_expr.to_owned();
        let value_selector = value_selector.to_string();

        // router is not re-entrant
        task::spawn(async move {
            let handler_tasks = futures::future::join_all(matching_handlers.into_iter().map(
                |(key, handler)| async {
                    let handler = handler;
                    let (payload, encoding) = handler(&context, &key_expr, &value_selector).await;
                    let mut data_info = DataInfo::new();
                    data_info.encoding = Some(encoding);

                    primitives.send_reply_data(
                        qid,
                        EVAL,
                        pid,
                        key.into(),
                        Some(data_info),
                        payload,
                    );
                },
            ));
            if ask_plugins {
                futures::join!(handler_tasks, async {
                    let plugin_status = plugins_status(&context, &key_expr, &value_selector).await;
                    for status in plugin_status {
                        let crate::plugins::Response { key, value } = status;
                        let payload: Vec<u8> = serde_json::to_vec(&value).unwrap();
                        let mut data_info = DataInfo::new();
                        data_info.encoding = Some(Encoding::APP_JSON);

                        primitives.send_reply_data(
                            qid,
                            EVAL,
                            pid,
                            key.into(),
                            Some(data_info),
                            payload.into(),
                        );
                    }
                });
            } else {
                handler_tasks.await;
            }

            primitives.send_reply_final(qid);
        });
    }

    fn send_reply_data(
        &self,
        qid: ZInt,
        replier_kind: ZInt,
        replier_id: ZenohId,
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
    let plugins: Vec<serde_json::Value> = {
        zlock!(context.plugins_mgr)
            .running_plugins_info()
            .iter()
            .map(|(name, path)| {
                json!({
                    "name": name,
                    "path": path
                })
            })
            .collect()
    };

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
            "peer": transport.get_zid().map_or_else(|_| "unknown".to_string(), |p| p.to_string()),
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
    let data: Vec<u8> = context
        .runtime
        .router
        .tables
        .read()
        .unwrap()
        .peers_net
        .as_ref()
        .unwrap()
        .dot()
        .into();
    (ZBuf::from(data), Encoding::TEXT_PLAIN)
}

pub async fn plugins_status(
    context: &AdminContext,
    key: &KeyExpr<'_>,
    args: &str,
) -> Vec<crate::plugins::Response> {
    let selector = Selector {
        key_selector: key.clone(),
        value_selector: args,
    };
    let guard = zlock!(context.plugins_mgr);
    let mut root_key = format!("/@/router/{}/status/plugins/", &context.pid_str);
    let mut responses = Vec::new();
    for (name, (path, plugin)) in guard.running_plugins() {
        with_extended_string(&mut root_key, &[name], |plugin_key| {
            with_extended_string(plugin_key, &["/__path__"], |plugin_path_key| {
                if key_expr::intersect(key.as_str(), plugin_path_key) {
                    responses.push(crate::plugins::Response {
                        key: plugin_path_key.clone(),
                        value: path.into(),
                    })
                }
            });
            let matches_plugin = |plugin_status_space: &mut String| {
                key_expr::intersect(key.as_str(), plugin_status_space)
            };
            if !with_extended_string(plugin_key, &["/**"], matches_plugin) {
                return;
            }
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        plugin.adminspace_getter(&selector, plugin_key)
                    })) {
                        Ok(Ok(response)) => responses.extend(response),
                        Ok(Err(e)) => {
                            log::error!("Plugin {} bailed from responding to {}: {}", name, key, e)
                        }
                        Err(e) => match e
                            .downcast_ref::<String>()
                            .map(|s| s.as_str())
                            .or_else(|| e.downcast_ref::<&str>().copied())
                        {
                            Some(e) => log::error!("Plugin {} panicked while responding to {}: {}", name, key, e),
                            None => log::error!("Plugin {} panicked while responding to {}. The panic message couldn't be recovered.", name, key),
                        },
                    }
        });
    }
    responses
}

fn with_extended_string<R, F: FnMut(&mut String) -> R>(
    prefix: &mut String,
    suffixes: &[&str],
    mut closure: F,
) -> R {
    let prefix_len = prefix.len();
    for suffix in suffixes {
        prefix.push_str(suffix);
    }
    let result = closure(prefix);
    prefix.truncate(prefix_len);
    result
}
