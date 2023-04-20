//
// Copyright (c) 2023 ZettaScale Technology
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
use super::routing::face::Face;
use super::Runtime;
use crate::key_expr::KeyExpr;
use crate::plugins::sealed as plugins;
use crate::prelude::sync::Sample;
use crate::queryable::Query;
use crate::queryable::QueryInner;
use crate::value::Value;
use async_std::task;
use log::{error, trace};
use serde_json::json;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::sync::Arc;
use std::sync::Mutex;
use zenoh_buffers::{SplitBuffer, ZBuf};
use zenoh_config::ValidatedMap;
use zenoh_core::SyncResolve;
use zenoh_protocol::{
    core::{
        key_expr::OwnedKeyExpr, Channel, CongestionControl, ConsolidationMode, KnownEncoding,
        QueryTarget, QueryableInfo, SampleKind, SubInfo, WireExpr, ZInt, ZenohId, EMPTY_EXPR_ID,
    },
    zenoh::{DataInfo, QueryBody, RoutingContext},
};
use zenoh_result::ZResult;
use zenoh_transport::{Primitives, TransportUnicast};

pub struct AdminContext {
    runtime: Runtime,
    plugins_mgr: Mutex<plugins::PluginsManager>,
    zid_str: String,
    version: String,
}

type Handler = Arc<dyn Fn(&AdminContext, Query) + Send + Sync>;

pub struct AdminSpace {
    zid: ZenohId,
    primitives: Mutex<Option<Arc<Face>>>,
    mappings: Mutex<HashMap<ZInt, String>>,
    handlers: HashMap<OwnedKeyExpr, Handler>,
    context: Arc<AdminContext>,
}

#[derive(Debug, Clone)]
enum PluginDiff {
    Delete(String),
    Start(crate::config::PluginLoad),
}

impl AdminSpace {
    pub async fn start(runtime: &Runtime, plugins_mgr: plugins::PluginsManager, version: String) {
        let zid_str = runtime.zid.to_string();
        let root_key: OwnedKeyExpr = format!("@/router/{zid_str}").try_into().unwrap();

        let mut handlers: HashMap<_, Handler> = HashMap::new();
        handlers.insert(root_key.clone(), Arc::new(router_data));
        handlers.insert(
            format!("@/router/{zid_str}/linkstate/routers")
                .try_into()
                .unwrap(),
            Arc::new(routers_linkstate_data),
        );
        handlers.insert(
            format!("@/router/{zid_str}/linkstate/peers")
                .try_into()
                .unwrap(),
            Arc::new(peers_linkstate_data),
        );
        handlers.insert(
            format!("@/router/{zid_str}/subscriber/**")
                .try_into()
                .unwrap(),
            Arc::new(subscribers_data),
        );
        handlers.insert(
            format!("@/router/{zid_str}/queryable/**")
                .try_into()
                .unwrap(),
            Arc::new(queryables_data),
        );
        handlers.insert(
            format!("@/router/{zid_str}/status/plugins/**")
                .try_into()
                .unwrap(),
            Arc::new(plugins_status),
        );

        let mut active_plugins = plugins_mgr
            .running_plugins_info()
            .into_iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect::<HashMap<_, _>>();

        let context = Arc::new(AdminContext {
            runtime: runtime.clone(),
            plugins_mgr: Mutex::new(plugins_mgr),
            zid_str,
            version,
        });
        let admin = Arc::new(AdminSpace {
            zid: runtime.zid,
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

    pub fn key_expr_to_string<'a>(&self, key_expr: &'a WireExpr) -> ZResult<KeyExpr<'a>> {
        if key_expr.scope == EMPTY_EXPR_ID {
            key_expr.suffix.as_ref().try_into()
        } else if key_expr.suffix.is_empty() {
            match zlock!(self.mappings).get(&key_expr.scope) {
                Some(prefix) => prefix.clone().try_into(),
                None => bail!("Failed to resolve ExprId {}", key_expr.scope),
            }
        } else {
            match zlock!(self.mappings).get(&key_expr.scope) {
                Some(prefix) => format!("{}{}", prefix, key_expr.suffix.as_ref()).try_into(),
                None => bail!("Failed to resolve ExprId {}", key_expr.scope),
            }
        }
    }
}

impl Primitives for AdminSpace {
    fn decl_resource(&self, expr_id: ZInt, key_expr: &WireExpr) {
        trace!("recv Resource {} {:?}", expr_id, key_expr);
        match self.key_expr_to_string(key_expr) {
            Ok(s) => {
                zlock!(self.mappings).insert(expr_id, s.into());
            }
            Err(e) => error!("Unknown expr_id {}! ({})", expr_id, e),
        }
    }

    fn forget_resource(&self, _expr_id: ZInt) {
        trace!("recv Forget Resource {}", _expr_id);
    }

    fn decl_publisher(&self, _key_expr: &WireExpr, _routing_context: Option<RoutingContext>) {
        trace!("recv Publisher {:?}", _key_expr);
    }

    fn forget_publisher(&self, _key_expr: &WireExpr, _routing_context: Option<RoutingContext>) {
        trace!("recv Forget Publisher {:?}", _key_expr);
    }

    fn decl_subscriber(
        &self,
        _key_expr: &WireExpr,
        _sub_info: &SubInfo,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!("recv Subscriber {:?} , {:?}", _key_expr, _sub_info);
    }

    fn forget_subscriber(&self, _key_expr: &WireExpr, _routing_context: Option<RoutingContext>) {
        trace!("recv Forget Subscriber {:?}", _key_expr);
    }

    fn decl_queryable(
        &self,
        _key_expr: &WireExpr,
        _qabl_info: &QueryableInfo,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!("recv Queryable {:?}", _key_expr);
    }

    fn forget_queryable(&self, _key_expr: &WireExpr, _routing_context: Option<RoutingContext>) {
        trace!("recv Forget Queryable {:?}", _key_expr);
    }

    fn send_data(
        &self,
        key_expr: &WireExpr,
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

        {
            let conf = self.context.runtime.config.lock();
            if !conf.adminspace.permissions().write {
                log::error!(
                    "Received PUT on '{}' but adminspace.permissions.write=false in configuration",
                    key_expr
                );
                return;
            }
        }

        if let Some(key) = key_expr
            .as_str()
            .strip_prefix(&format!("@/router/{}/config/", &self.context.zid_str))
        {
            if let Some(DataInfo {
                kind: SampleKind::Delete,
                ..
            }) = data_info
            {
                log::trace!(
                    "Deleting conf value /@/router/{}/config/{}",
                    &self.context.zid_str,
                    key
                );
                if let Err(e) = self.context.runtime.config.remove(key) {
                    log::error!("Error deleting conf value {} : {}", key_expr, e)
                }
            } else {
                match std::str::from_utf8(&payload.contiguous()) {
                    Ok(json) => {
                        log::trace!(
                            "Insert conf value /@/router/{}/config/{} : {}",
                            &self.context.zid_str,
                            key,
                            json
                        );
                        if let Err(e) = (&self.context.runtime.config).insert_json5(key, json) {
                            error!(
                                "Error inserting conf value /@/router/{}/config/{} : {} - {}",
                                &self.context.zid_str, key, json, e
                            );
                        }
                    }
                    Err(e) => error!(
                        "Received non utf8 conf value on /@/router/{}/config/{} : {}",
                        &self.context.zid_str, key, e
                    ),
                }
            }
        }
    }

    fn send_query(
        &self,
        key_expr: &WireExpr,
        parameters: &str,
        qid: ZInt,
        target: QueryTarget,
        _consolidation: ConsolidationMode,
        body: Option<QueryBody>,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!(
            "recv Query {:?} {:?} {:?} {:?}",
            key_expr,
            parameters,
            target,
            _consolidation
        );
        let primitives = zlock!(self.primitives).as_ref().unwrap().clone();

        {
            let conf = self.context.runtime.config.lock();
            if !conf.adminspace.permissions().read {
                log::error!(
                    "Received GET on '{}' but adminspace.permissions.read=false in configuration",
                    key_expr
                );
                primitives.send_reply_final(qid);
                return;
            }
        }

        let key_expr = match self.key_expr_to_string(key_expr) {
            Ok(key_expr) => key_expr.into_owned(),
            Err(e) => {
                log::error!("Unknown KeyExpr: {}", e);
                primitives.send_reply_final(qid);
                return;
            }
        };

        let zid = self.zid;
        let parameters = parameters.to_owned();
        let query = Query {
            inner: Arc::new(QueryInner {
                key_expr: key_expr.clone(),
                parameters,
                value: body.map(|b| {
                    Value::from(b.payload).encoding(b.data_info.encoding.unwrap_or_default())
                }),
                qid,
                zid,
                primitives,
            }),
        };

        for (key, handler) in &self.handlers {
            if key_expr.intersects(key) {
                handler(&self.context, query.clone());
            }
        }
    }

    fn send_reply_data(
        &self,
        qid: ZInt,
        replier_id: ZenohId,
        key_expr: WireExpr,
        info: Option<DataInfo>,
        payload: ZBuf,
    ) {
        trace!(
            "recv ReplyData {:?} {:?} {:?} {:?} {:?}",
            qid,
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
        _key_expr: &WireExpr,
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

fn router_data(context: &AdminContext, query: Query) {
    let reply_key: OwnedKeyExpr = format!("@/router/{}", context.zid_str).try_into().unwrap();

    let transport_mgr = context.runtime.manager().clone();

    // plugins info
    let plugins: serde_json::Value = {
        zlock!(context.plugins_mgr)
            .running_plugins_info()
            .into_iter()
            .map(|(k, v)| (k, json!({ "path": v })))
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
            let stats = crate::prelude::Parameters::decode(selector)
                .any(|(k, v)| k.as_ref() == "_stats" && v != "false");
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
        "zid": context.zid_str,
        "version": context.version,
        "locators": locators,
        "sessions": transports,
        "plugins": plugins,
    });
    log::trace!("AdminSpace router_data: {:?}", json);
    if let Err(e) = query
        .reply(Ok(Sample::new(
            reply_key,
            Value::from(json.to_string().as_bytes().to_vec())
                .encoding(KnownEncoding::AppJson.into()),
        )))
        .res()
    {
        log::error!("Error sending AdminSpace reply: {:?}", e);
    }
}

fn routers_linkstate_data(context: &AdminContext, query: Query) {
    let reply_key: OwnedKeyExpr = format!("@/router/{}/linkstate/routers", context.zid_str)
        .try_into()
        .unwrap();

    let tables = zread!(context.runtime.router.tables.tables);

    if let Err(e) = query
        .reply(Ok(Sample::new(
            reply_key,
            Value::from(
                tables
                    .routers_net
                    .as_ref()
                    .map(|net| net.dot())
                    .unwrap_or_else(|| "graph {}".to_string())
                    .as_bytes()
                    .to_vec(),
            )
            .encoding(KnownEncoding::TextPlain.into()),
        )))
        .res()
    {
        log::error!("Error sending AdminSpace reply: {:?}", e);
    }
}

fn peers_linkstate_data(context: &AdminContext, query: Query) {
    let reply_key: OwnedKeyExpr = format!("@/router/{}/linkstate/peers", context.zid_str)
        .try_into()
        .unwrap();

    let tables = zread!(context.runtime.router.tables.tables);

    if let Err(e) = query
        .reply(Ok(Sample::new(
            reply_key,
            Value::from(
                tables
                    .peers_net
                    .as_ref()
                    .map(|net| net.dot())
                    .unwrap_or_else(|| "graph {}".to_string())
                    .as_bytes()
                    .to_vec(),
            )
            .encoding(KnownEncoding::TextPlain.into()),
        )))
        .res()
    {
        log::error!("Error sending AdminSpace reply: {:?}", e);
    }
}

fn subscribers_data(context: &AdminContext, query: Query) {
    let tables = zread!(context.runtime.router.tables.tables);
    for sub in tables.router_subs.iter() {
        let key = KeyExpr::try_from(format!(
            "@/router/{}/subscriber/{}",
            context.zid_str,
            sub.expr()
        ))
        .unwrap();
        if query.key_expr().intersects(&key) {
            if let Err(e) = query.reply(Ok(Sample::new(key, Value::empty()))).res() {
                log::error!("Error sending AdminSpace reply: {:?}", e);
            }
        }
    }
}

fn queryables_data(context: &AdminContext, query: Query) {
    let tables = zread!(context.runtime.router.tables.tables);
    for qabl in tables.router_qabls.iter() {
        let key = KeyExpr::try_from(format!(
            "@/router/{}/queryable/{}",
            context.zid_str,
            qabl.expr()
        ))
        .unwrap();
        if query.key_expr().intersects(&key) {
            if let Err(e) = query.reply(Ok(Sample::new(key, Value::empty()))).res() {
                log::error!("Error sending AdminSpace reply: {:?}", e);
            }
        }
    }
}

fn plugins_status(context: &AdminContext, query: Query) {
    let selector = query.selector();
    let guard = zlock!(context.plugins_mgr);
    let mut root_key = format!("@/router/{}/status/plugins/", &context.zid_str);

    for (name, (path, plugin)) in guard.running_plugins() {
        with_extended_string(&mut root_key, &[name], |plugin_key| {
            with_extended_string(plugin_key, &["/__path__"], |plugin_path_key| {
                if let Ok(key_expr) = KeyExpr::try_from(plugin_path_key.clone()) {
                    if query.key_expr().intersects(&key_expr) {
                        if let Err(e) = query
                            .reply(Ok(Sample::new(
                                key_expr,
                                Value::from(path).encoding(KnownEncoding::AppJson.into()),
                            )))
                            .res()
                        {
                            log::error!("Error sending AdminSpace reply: {:?}", e);
                        }
                    }
                } else {
                    log::error!("Error: invalid plugin path key {}", plugin_path_key);
                }
            });
            let matches_plugin = |plugin_status_space: &mut String| {
                query
                    .key_expr()
                    .intersects(plugin_status_space.as_str().try_into().unwrap())
            };
            if !with_extended_string(plugin_key, &["/**"], matches_plugin) {
                return;
            }
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                plugin.adminspace_getter(&selector, plugin_key)
            })) {
                Ok(Ok(responses)) => {
                    for response in responses {
                        if let Ok(key_expr) = KeyExpr::try_from(response.key) {
                            if let Err(e) = query.reply(Ok(Sample::new(
                                key_expr,
                                Value::from(response.value).encoding(KnownEncoding::AppJson.into()),
                            )))
                            .res()
                            {
                                log::error!("Error sending AdminSpace reply: {:?}", e);
                            }
                        } else {
                            log::error!("Error: plugin {} replied with an invalid key", plugin_key);
                        }
                    }
                }
                Ok(Err(e)) => {
                    log::error!("Plugin {} bailed from responding to {}: {}", name, query.key_expr(), e)
                }
                Err(e) => match e
                    .downcast_ref::<String>()
                    .map(|s| s.as_str())
                    .or_else(|| e.downcast_ref::<&str>().copied())
                {
                    Some(e) => log::error!("Plugin {} panicked while responding to {}: {}", name, query.key_expr(), e),
                    None => log::error!("Plugin {} panicked while responding to {}. The panic message couldn't be recovered.", name, query.key_expr()),
                },
            }
        });
    }
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
