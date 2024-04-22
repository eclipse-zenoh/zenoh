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
use super::routing::dispatcher::face::Face;
use super::Runtime;
use crate::key_expr::KeyExpr;
use crate::net::primitives::Primitives;
use crate::plugins::sealed::{self as plugins};
use crate::prelude::sync::{Sample, SyncResolve};
use crate::queryable::Query;
use crate::queryable::QueryInner;
use crate::value::Value;
use serde_json::json;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::sync::Arc;
use std::sync::Mutex;
use tracing::{error, trace};
use zenoh_buffers::buffer::SplitBuffer;
use zenoh_config::{ConfigValidator, ValidatedMap, WhatAmI};
use zenoh_plugin_trait::{PluginControl, PluginStatus};
use zenoh_protocol::core::key_expr::keyexpr;
use zenoh_protocol::{
    core::{key_expr::OwnedKeyExpr, ExprId, KnownEncoding, WireExpr, ZenohId, EMPTY_EXPR_ID},
    network::{
        declare::{queryable::ext::QueryableInfo, subscriber::ext::SubscriberInfo},
        ext, Declare, DeclareBody, DeclareQueryable, DeclareSubscriber, Push, Request, Response,
        ResponseFinal,
    },
    zenoh::{PushBody, RequestBody},
};
use zenoh_result::ZResult;
use zenoh_transport::unicast::TransportUnicast;

pub struct AdminContext {
    runtime: Runtime,
    plugins_mgr: Mutex<plugins::PluginsManager>,
    zid_str: String,
    version: String,
    metadata: serde_json::Value,
}

type Handler = Arc<dyn Fn(&AdminContext, Query) + Send + Sync>;

pub struct AdminSpace {
    zid: ZenohId,
    primitives: Mutex<Option<Arc<Face>>>,
    mappings: Mutex<HashMap<ExprId, String>>,
    handlers: HashMap<OwnedKeyExpr, Handler>,
    context: Arc<AdminContext>,
}

#[derive(Debug, Clone)]
enum PluginDiff {
    Delete(String),
    Start(crate::config::PluginLoad),
}

impl ConfigValidator for AdminSpace {
    fn check_config(
        &self,
        name: &str,
        path: &str,
        current: &serde_json::Map<String, serde_json::Value>,
        new: &serde_json::Map<String, serde_json::Value>,
    ) -> ZResult<Option<serde_json::Map<String, serde_json::Value>>> {
        let plugin_mgr = zlock!(self.context.plugins_mgr);
        let Some(plugin) = plugin_mgr.started_plugin(name) else {
            tracing::warn!("Plugin `{}` is not started", name);
            // If plugin not started, just allow any config. The plugin `name` will be attempted to start with this config
            // on config comparison (see `PluginDiff`)
            return Ok(None);
        };
        plugin.instance().config_checker(path, current, new)
    }
}

impl AdminSpace {
    fn start_plugin(
        plugin_mgr: &mut plugins::PluginsManager,
        config: &crate::config::PluginLoad,
        start_args: &Runtime,
    ) -> ZResult<()> {
        let name = &config.name;
        let declared = if let Some(declared) = plugin_mgr.plugin_mut(name) {
            tracing::warn!("Plugin `{}` was already declared", declared.name());
            declared
        } else if let Some(paths) = &config.paths {
            plugin_mgr.declare_dynamic_plugin_by_paths(name, paths)?
        } else {
            plugin_mgr.declare_dynamic_plugin_by_name(name, name)?
        };

        let loaded = if let Some(loaded) = declared.loaded_mut() {
            tracing::warn!(
                "Plugin `{}` was already loaded from {}",
                loaded.name(),
                loaded.path()
            );
            loaded
        } else {
            declared.load()?
        };

        if let Some(started) = loaded.started_mut() {
            tracing::warn!("Plugin `{}` was already started", started.name());
        } else {
            let started = loaded.start(start_args)?;
            tracing::info!(
                "Successfully started plugin `{}` from {}",
                started.name(),
                started.path()
            );
        };

        Ok(())
    }

    pub async fn start(runtime: &Runtime, plugins_mgr: plugins::PluginsManager, version: String) {
        let zid_str = runtime.state.zid.to_string();
        let metadata = runtime.state.metadata.clone();
        let root_key: OwnedKeyExpr = format!("@/router/{zid_str}").try_into().unwrap();

        let mut handlers: HashMap<_, Handler> = HashMap::new();
        handlers.insert(root_key.clone(), Arc::new(router_data));
        handlers.insert(
            format!("@/router/{zid_str}/metrics").try_into().unwrap(),
            Arc::new(router_metrics),
        );
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
            format!("@/router/{zid_str}/plugins/**").try_into().unwrap(),
            Arc::new(plugins_data),
        );
        handlers.insert(
            format!("@/router/{zid_str}/status/plugins/**")
                .try_into()
                .unwrap(),
            Arc::new(plugins_status),
        );

        let mut active_plugins = plugins_mgr
            .started_plugins_iter()
            .map(|rec| (rec.name().to_string(), rec.path().to_string()))
            .collect::<HashMap<_, _>>();

        let context = Arc::new(AdminContext {
            runtime: runtime.clone(),
            plugins_mgr: Mutex::new(plugins_mgr),
            zid_str,
            version,
            metadata,
        });
        let admin = Arc::new(AdminSpace {
            zid: runtime.zid(),
            primitives: Mutex::new(None),
            mappings: Mutex::new(HashMap::new()),
            handlers,
            context,
        });

        admin
            .context
            .runtime
            .state
            .config
            .lock()
            .set_plugin_validator(Arc::downgrade(&admin));

        let cfg_rx = admin.context.runtime.state.config.subscribe();
        tokio::task::spawn({
            let admin = admin.clone();
            async move {
                while let Ok(change) = cfg_rx.recv_async().await {
                    let change = change.strip_prefix('/').unwrap_or(&change);
                    if !change.starts_with("plugins") {
                        continue;
                    }

                    let requested_plugins = {
                        let cfg_guard = admin.context.runtime.state.config.lock();
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
                            PluginDiff::Delete(name) => {
                                active_plugins.remove(name.as_str());
                                if let Some(running) = plugins_mgr.started_plugin_mut(&name) {
                                    running.stop()
                                }
                            }
                            PluginDiff::Start(plugin) => {
                                if let Err(e) = Self::start_plugin(
                                    &mut plugins_mgr,
                                    &plugin,
                                    &admin.context.runtime,
                                ) {
                                    if plugin.required {
                                        panic!("Failed to load plugin `{}`: {}", plugin.name, e)
                                    } else {
                                        tracing::error!(
                                            "Failed to load plugin `{}`: {}",
                                            plugin.name,
                                            e
                                        )
                                    }
                                }
                            }
                        }
                    }
                }
                tracing::info!("Running plugins: {:?}", &active_plugins)
            }
        });

        let primitives = runtime.state.router.new_primitives(admin.clone());
        zlock!(admin.primitives).replace(primitives.clone());

        primitives.send_declare(Declare {
            ext_qos: ext::QoSType::declare_default(),
            ext_tstamp: None,
            ext_nodeid: ext::NodeIdType::default(),
            body: DeclareBody::DeclareQueryable(DeclareQueryable {
                id: 0, // @TODO use proper QueryableId (#703)
                wire_expr: [&root_key, "/**"].concat().into(),
                ext_info: QueryableInfo {
                    complete: 0,
                    distance: 0,
                },
            }),
        });

        primitives.send_declare(Declare {
            ext_qos: ext::QoSType::declare_default(),
            ext_tstamp: None,
            ext_nodeid: ext::NodeIdType::default(),
            body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                id: 0, // @TODO use proper SubscriberId (#703)
                wire_expr: [&root_key, "/config/**"].concat().into(),
                ext_info: SubscriberInfo::default(),
            }),
        });
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
    fn send_declare(&self, msg: Declare) {
        tracing::trace!("Recv declare {:?}", msg);
        if let DeclareBody::DeclareKeyExpr(m) = msg.body {
            match self.key_expr_to_string(&m.wire_expr) {
                Ok(s) => {
                    zlock!(self.mappings).insert(m.id, s.into());
                }
                Err(e) => error!("Unknown expr_id {}! ({})", m.id, e),
            }
        }
    }

    fn send_push(&self, msg: Push) {
        trace!("recv Push {:?}", msg);
        {
            let conf = self.context.runtime.state.config.lock();
            if !conf.adminspace.permissions().write {
                tracing::error!(
                    "Received PUT on '{}' but adminspace.permissions.write=false in configuration",
                    msg.wire_expr
                );
                return;
            }
        }

        if let Some(key) = msg
            .wire_expr
            .as_str()
            .strip_prefix(&format!("@/router/{}/config/", &self.context.zid_str))
        {
            match msg.payload {
                PushBody::Put(put) => match std::str::from_utf8(&put.payload.contiguous()) {
                    Ok(json) => {
                        tracing::trace!(
                            "Insert conf value /@/router/{}/config/{} : {}",
                            &self.context.zid_str,
                            key,
                            json
                        );
                        if let Err(e) = (&self.context.runtime.state.config).insert_json5(key, json)
                        {
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
                },
                PushBody::Del(_) => {
                    tracing::trace!(
                        "Deleting conf value /@/router/{}/config/{}",
                        &self.context.zid_str,
                        key
                    );
                    if let Err(e) = self.context.runtime.state.config.remove(key) {
                        tracing::error!("Error deleting conf value {} : {}", msg.wire_expr, e)
                    }
                }
            }
        }
    }

    fn send_request(&self, msg: Request) {
        trace!("recv Request {:?}", msg);
        if let RequestBody::Query(query) = msg.payload {
            let primitives = zlock!(self.primitives).as_ref().unwrap().clone();
            {
                let conf = self.context.runtime.state.config.lock();
                if !conf.adminspace.permissions().read {
                    tracing::error!(
                        "Received GET on '{}' but adminspace.permissions.read=false in configuration",
                        msg.wire_expr
                    );
                    primitives.send_response_final(ResponseFinal {
                        rid: msg.id,
                        ext_qos: ext::QoSType::response_final_default(),
                        ext_tstamp: None,
                    });
                    return;
                }
            }

            let key_expr = match self.key_expr_to_string(&msg.wire_expr) {
                Ok(key_expr) => key_expr.into_owned(),
                Err(e) => {
                    tracing::error!("Unknown KeyExpr: {}", e);
                    primitives.send_response_final(ResponseFinal {
                        rid: msg.id,
                        ext_qos: ext::QoSType::response_final_default(),
                        ext_tstamp: None,
                    });
                    return;
                }
            };

            let zid = self.zid;
            let parameters = query.parameters.to_owned();
            let query = Query {
                inner: Arc::new(QueryInner {
                    key_expr: key_expr.clone(),
                    parameters,
                    value: query
                        .ext_body
                        .map(|b| Value::from(b.payload).encoding(b.encoding)),
                    qid: msg.id,
                    zid,
                    primitives,
                    #[cfg(feature = "unstable")]
                    attachment: query.ext_attachment.map(Into::into),
                }),
            };

            for (key, handler) in &self.handlers {
                if key_expr.intersects(key) {
                    handler(&self.context, query.clone());
                }
            }
        }
    }

    fn send_response(&self, msg: Response) {
        trace!("recv Response {:?}", msg);
    }

    fn send_response_final(&self, msg: ResponseFinal) {
        trace!("recv ResponseFinal {:?}", msg);
    }

    fn send_close(&self) {
        trace!("recv Close");
    }
}

impl crate::net::primitives::EPrimitives for AdminSpace {
    #[inline]
    fn send_declare(&self, ctx: crate::net::routing::RoutingContext<Declare>) {
        (self as &dyn Primitives).send_declare(ctx.msg)
    }

    #[inline]
    fn send_push(&self, msg: Push) {
        (self as &dyn Primitives).send_push(msg)
    }

    #[inline]
    fn send_request(&self, ctx: crate::net::routing::RoutingContext<Request>) {
        (self as &dyn Primitives).send_request(ctx.msg)
    }

    #[inline]
    fn send_response(&self, ctx: crate::net::routing::RoutingContext<Response>) {
        (self as &dyn Primitives).send_response(ctx.msg)
    }

    #[inline]
    fn send_response_final(&self, ctx: crate::net::routing::RoutingContext<ResponseFinal>) {
        (self as &dyn Primitives).send_response_final(ctx.msg)
    }

    #[inline]
    fn send_close(&self) {
        (self as &dyn Primitives).send_close()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn router_data(context: &AdminContext, query: Query) {
    let reply_key: OwnedKeyExpr = format!("@/router/{}", context.zid_str).try_into().unwrap();

    let transport_mgr = context.runtime.manager().clone();

    // plugins info
    let plugins: serde_json::Value = {
        let plugins_mgr = zlock!(context.plugins_mgr);
        plugins_mgr
            .started_plugins_iter()
            .map(|rec| (rec.name(), json!({ "path": rec.path() })))
            .collect()
    };

    // locators info
    let locators: Vec<serde_json::Value> = transport_mgr
        .get_locators()
        .iter()
        .map(|locator| json!(locator.as_str()))
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
            let stats = crate::prelude::Parameters::decode(&query.selector())
                .any(|(k, v)| k.as_ref() == "_stats" && v != "false");
            if stats {
                json.as_object_mut().unwrap().insert(
                    "stats".to_string(),
                    transport
                        .get_stats()
                        .map_or_else(|_| json!({}), |p| json!(p.report())),
                );
            }
        }
        json
    };
    let transports: Vec<serde_json::Value> = zenoh_runtime::ZRuntime::Net
        .block_in_place(transport_mgr.get_transports_unicast())
        .iter()
        .map(transport_to_json)
        .collect();

    #[allow(unused_mut)]
    let mut json = json!({
        "zid": context.zid_str,
        "version": context.version,
        "metadata": context.metadata,
        "locators": locators,
        "sessions": transports,
        "plugins": plugins,
    });

    #[cfg(feature = "stats")]
    {
        let stats = crate::prelude::Parameters::decode(&query.selector())
            .any(|(k, v)| k.as_ref() == "_stats" && v != "false");
        if stats {
            json.as_object_mut().unwrap().insert(
                "stats".to_string(),
                json!(transport_mgr.get_stats().report()),
            );
        }
    }

    tracing::trace!("AdminSpace router_data: {:?}", json);
    if let Err(e) = query
        .reply(Ok(Sample::new(
            reply_key,
            Value::from(json.to_string().as_bytes().to_vec())
                .encoding(KnownEncoding::AppJson.into()),
        )))
        .res()
    {
        tracing::error!("Error sending AdminSpace reply: {:?}", e);
    }
}

fn router_metrics(context: &AdminContext, query: Query) {
    let reply_key: OwnedKeyExpr = format!("@/router/{}/metrics", context.zid_str)
        .try_into()
        .unwrap();
    #[allow(unused_mut)]
    let mut metrics = format!(
        r#"# HELP zenoh_build Informations about zenoh.
# TYPE zenoh_build gauge
zenoh_build{{version="{}"}} 1
"#,
        context.version
    );

    #[cfg(feature = "stats")]
    metrics.push_str(
        &context
            .runtime
            .manager()
            .get_stats()
            .report()
            .openmetrics_text(),
    );

    if let Err(e) = query
        .reply(Ok(Sample::new(
            reply_key,
            Value::from(metrics.as_bytes().to_vec()).encoding(KnownEncoding::TextPlain.into()),
        )))
        .res()
    {
        tracing::error!("Error sending AdminSpace reply: {:?}", e);
    }
}

fn routers_linkstate_data(context: &AdminContext, query: Query) {
    let reply_key: OwnedKeyExpr = format!("@/router/{}/linkstate/routers", context.zid_str)
        .try_into()
        .unwrap();

    let tables = zread!(context.runtime.state.router.tables.tables);

    if let Err(e) = query
        .reply(Ok(Sample::new(
            reply_key,
            Value::from(
                tables
                    .hat_code
                    .info(&tables, WhatAmI::Router)
                    .as_bytes()
                    .to_vec(),
            )
            .encoding(KnownEncoding::TextPlain.into()),
        )))
        .res()
    {
        tracing::error!("Error sending AdminSpace reply: {:?}", e);
    }
}

fn peers_linkstate_data(context: &AdminContext, query: Query) {
    let reply_key: OwnedKeyExpr = format!("@/router/{}/linkstate/peers", context.zid_str)
        .try_into()
        .unwrap();

    let tables = zread!(context.runtime.state.router.tables.tables);

    if let Err(e) = query
        .reply(Ok(Sample::new(
            reply_key,
            Value::from(
                tables
                    .hat_code
                    .info(&tables, WhatAmI::Peer)
                    .as_bytes()
                    .to_vec(),
            )
            .encoding(KnownEncoding::TextPlain.into()),
        )))
        .res()
    {
        tracing::error!("Error sending AdminSpace reply: {:?}", e);
    }
}

fn subscribers_data(context: &AdminContext, query: Query) {
    let tables = zread!(context.runtime.state.router.tables.tables);
    for sub in tables.hat_code.get_subscriptions(&tables) {
        let key = KeyExpr::try_from(format!(
            "@/router/{}/subscriber/{}",
            context.zid_str,
            sub.0.expr()
        ))
        .unwrap();
        if query.key_expr().intersects(&key) {
            if let Err(e) = query
                .reply(Ok(Sample::new(
                    key,
                    Value::from(serde_json::to_string(&sub.1).unwrap_or("{}".to_string()))
                        .encoding(KnownEncoding::AppJson.into()),
                )))
                .res()
            {
                tracing::error!("Error sending AdminSpace reply: {:?}", e);
            }
        }
    }
}

fn queryables_data(context: &AdminContext, query: Query) {
    let tables = zread!(context.runtime.state.router.tables.tables);
    for qabl in tables.hat_code.get_queryables(&tables) {
        let key = KeyExpr::try_from(format!(
            "@/router/{}/queryable/{}",
            context.zid_str,
            qabl.0.expr()
        ))
        .unwrap();
        if query.key_expr().intersects(&key) {
            if let Err(e) = query
                .reply(Ok(Sample::new(
                    key,
                    Value::from(serde_json::to_string(&qabl.1).unwrap_or("{}".to_string()))
                        .encoding(KnownEncoding::AppJson.into()),
                )))
                .res()
            {
                tracing::error!("Error sending AdminSpace reply: {:?}", e);
            }
        }
    }
}

fn plugins_data(context: &AdminContext, query: Query) {
    let guard = zlock!(context.plugins_mgr);
    let root_key = format!("@/router/{}/plugins", &context.zid_str);
    let root_key = unsafe { keyexpr::from_str_unchecked(&root_key) };
    tracing::debug!("requested plugins status {:?}", query.key_expr());
    if let [names, ..] = query.key_expr().strip_prefix(root_key)[..] {
        let statuses = guard.plugins_status(names);
        for status in statuses {
            tracing::debug!("plugin status: {:?}", status);
            let key = root_key.join(status.name()).unwrap();
            let status = serde_json::to_value(status).unwrap();
            if let Err(e) = query.reply(Ok(Sample::new(key, Value::from(status)))).res() {
                tracing::error!("Error sending AdminSpace reply: {:?}", e);
            }
        }
    }
}

fn plugins_status(context: &AdminContext, query: Query) {
    let selector = query.selector();
    let guard = zlock!(context.plugins_mgr);
    let mut root_key = format!("@/router/{}/status/plugins/", &context.zid_str);

    for plugin in guard.started_plugins_iter() {
        with_extended_string(&mut root_key, &[plugin.name()], |plugin_key| {
            // @TODO: response to "__version__", this need not to be implemented by each plugin
            with_extended_string(plugin_key, &["/__path__"], |plugin_path_key| {
                if let Ok(key_expr) = KeyExpr::try_from(plugin_path_key.clone()) {
                    if query.key_expr().intersects(&key_expr) {
                        if let Err(e) = query
                            .reply(Ok(Sample::new(
                                key_expr,
                                Value::from(plugin.path()).encoding(KnownEncoding::AppJson.into()),
                            )))
                            .res()
                        {
                            tracing::error!("Error sending AdminSpace reply: {:?}", e);
                        }
                    }
                } else {
                    tracing::error!("Error: invalid plugin path key {}", plugin_path_key);
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
                plugin.instance().adminspace_getter(&selector, plugin_key)
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
                                tracing::error!("Error sending AdminSpace reply: {:?}", e);
                            }
                        } else {
                            tracing::error!("Error: plugin {} replied with an invalid key", plugin_key);
                        }
                    }
                }
                Ok(Err(e)) => {
                    tracing::error!("Plugin {} bailed from responding to {}: {}", plugin.name(), query.key_expr(), e)
                }
                Err(e) => match e
                    .downcast_ref::<String>()
                    .map(|s| s.as_str())
                    .or_else(|| e.downcast_ref::<&str>().copied())
                {
                    Some(e) => tracing::error!("Plugin {} panicked while responding to {}: {}", plugin.name(), query.key_expr(), e),
                    None => tracing::error!("Plugin {} panicked while responding to {}. The panic message couldn't be recovered.", plugin.name(), query.key_expr()),
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
