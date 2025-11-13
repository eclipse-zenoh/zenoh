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
use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    mem,
    sync::{Arc, Mutex},
};

use itertools::Itertools;
use serde_json::json;
use tracing::{error, trace};
use zenoh_buffers::buffer::SplitBuffer;
use zenoh_config::{unwrap_or_default, wrappers::ZenohId, ConfigValidator, WhatAmI};
use zenoh_core::Wait;
use zenoh_keyexpr::keyexpr;
use zenoh_link::Link;
#[cfg(all(feature = "plugins", feature = "runtime_plugins"))]
use zenoh_plugin_trait::PluginDiff;
#[cfg(feature = "plugins")]
use zenoh_plugin_trait::{PluginControl, PluginStatus};
use zenoh_protocol::{
    core::{key_expr::OwnedKeyExpr, ExprId, Reliability, WireExpr, ZenohIdProto, EMPTY_EXPR_ID},
    network::{
        declare::{queryable::ext::QueryableInfoType, QueryableId},
        ext, Declare, DeclareBody, DeclareQueryable, DeclareSubscriber, Interest, Push, Request,
        Response, ResponseFinal,
    },
    zenoh::{PushBody, RequestBody},
};
use zenoh_result::ZResult;
#[cfg(feature = "stats")]
use zenoh_transport::stats::TransportStats;
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast, TransportPeer};

use super::{routing::dispatcher::face::Face, Runtime};
#[cfg(all(feature = "plugins", feature = "runtime_plugins"))]
use crate::api::plugins::PluginsManager;
#[cfg(all(feature = "plugins", feature = "runtime_plugins"))]
use crate::internal::runtime::DynamicRuntime;
use crate::{
    api::{
        bytes::ZBytes,
        key_expr::KeyExpr,
        queryable::{Query, QueryInner},
    },
    bytes::Encoding,
    net::primitives::Primitives,
};

pub struct AdminContext {
    runtime: Runtime,
    version: String,
}

type Handler = Arc<dyn Fn(&AdminContext, Query) + Send + Sync>;

pub struct AdminSpace {
    zid: ZenohId,
    queryable_id: QueryableId,
    primitives: Mutex<Option<Arc<Face>>>,
    mappings: Mutex<HashMap<ExprId, String>>,
    handlers: HashMap<OwnedKeyExpr, Handler>,
    context: Arc<AdminContext>,
}

impl ConfigValidator for AdminSpace {
    fn check_config(
        &self,
        name: &str,
        path: &str,
        current: &serde_json::Map<String, serde_json::Value>,
        new: &serde_json::Map<String, serde_json::Value>,
    ) -> ZResult<Option<serde_json::Map<String, serde_json::Value>>> {
        #[cfg(feature = "plugins")]
        {
            let plugins_mgr = self.context.runtime.plugins_manager();
            let Some(plugin) = plugins_mgr.started_plugin(name) else {
                tracing::warn!("Plugin `{}` is not started", name);
                // If plugin not started, just allow any config. The plugin `name` will be attempted to start with this config
                // on config comparison (see `PluginDiff`)
                return Ok(None);
            };
            plugin
                .instance()
                .config_checker(path, &current.into(), &new.into())
                .map(|m| m.map(|kv| kv.into()))
        }
        #[cfg(not(feature = "plugins"))]
        {
            let _ = (name, path, current, new);
            Ok(None)
        }
    }
}

impl AdminSpace {
    #[cfg(all(feature = "plugins", feature = "runtime_plugins"))]
    fn start_plugin(
        plugin_mgr: &mut PluginsManager,
        config: &zenoh_config::PluginLoad,
        start_args: &DynamicRuntime,
        required: bool,
    ) -> ZResult<()> {
        let id = &config.id;
        let name = &config.name;
        let declared = if let Some(declared) = plugin_mgr.plugin_mut(id) {
            tracing::warn!("Plugin `{}` was already declared", declared.id());
            declared
        } else if let Some(paths) = &config.paths {
            plugin_mgr.declare_dynamic_plugin_by_paths(id, name, paths, required)?
        } else {
            plugin_mgr.declare_dynamic_plugin_by_name(id, name, required)?
        };

        let loaded = if let Some(loaded) = declared.loaded_mut() {
            tracing::warn!(
                "Plugin `{}` was already loaded from {}",
                loaded.id(),
                loaded.path()
            );
            loaded
        } else {
            match declared.load()? {
                Some(loaded) => loaded,
                None => {
                    tracing::warn!(
                        "Plugin `{}` will not be loaded as plugin loading is disabled",
                        config.name
                    );
                    return Ok(());
                }
            }
        };

        if let Some(started) = loaded.started_mut() {
            tracing::warn!("Plugin `{}` was already started", started.id());
        } else {
            let started = loaded.start(start_args)?;
            tracing::info!(
                "Successfully started plugin `{}` from {}",
                started.id(),
                started.path()
            );
        };

        Ok(())
    }

    pub async fn start(runtime: &Runtime, version: String) {
        let zid_str = runtime.state.zid.to_string();
        let whatami_str = runtime.state.whatami.to_str();
        let config = &mut runtime.config().lock().0;
        let root_key: OwnedKeyExpr = format!("@/{zid_str}/{whatami_str}").try_into().unwrap();

        let mut handlers: HashMap<_, Handler> = HashMap::new();
        handlers.insert(root_key.clone(), Arc::new(local_data));
        handlers.insert(
            format!("@/{zid_str}/{whatami_str}/metrics")
                .try_into()
                .unwrap(),
            Arc::new(metrics),
        );
        if runtime.state.whatami == WhatAmI::Router {
            handlers.insert(
                format!("@/{zid_str}/{whatami_str}/linkstate/routers")
                    .try_into()
                    .unwrap(),
                Arc::new(routers_linkstate_data),
            );
        }
        if runtime.state.whatami != WhatAmI::Client
            && unwrap_or_default!(config.routing().peer().mode()) == *"linkstate"
        {
            handlers.insert(
                format!("@/{zid_str}/{whatami_str}/linkstate/peers")
                    .try_into()
                    .unwrap(),
                Arc::new(peers_linkstate_data),
            );
        }
        handlers.insert(
            format!("@/{zid_str}/{whatami_str}/subscriber/**")
                .try_into()
                .unwrap(),
            Arc::new(subscribers_data),
        );
        handlers.insert(
            format!("@/{zid_str}/{whatami_str}/publisher/**")
                .try_into()
                .unwrap(),
            Arc::new(publishers_data),
        );
        handlers.insert(
            format!("@/{zid_str}/{whatami_str}/queryable/**")
                .try_into()
                .unwrap(),
            Arc::new(queryables_data),
        );
        handlers.insert(
            format!("@/{zid_str}/{whatami_str}/querier/**")
                .try_into()
                .unwrap(),
            Arc::new(queriers_data),
        );
        if runtime.state.whatami == WhatAmI::Router {
            handlers.insert(
                format!("@/{zid_str}/{whatami_str}/route/successor/**")
                    .try_into()
                    .unwrap(),
                Arc::new(route_successor),
            );
        }

        #[cfg(feature = "plugins")]
        handlers.insert(
            format!("@/{zid_str}/{whatami_str}/plugins/**")
                .try_into()
                .unwrap(),
            Arc::new(plugins_data),
        );

        #[cfg(feature = "plugins")]
        handlers.insert(
            format!("@/{zid_str}/{whatami_str}/status/plugins/**")
                .try_into()
                .unwrap(),
            Arc::new(plugins_status),
        );

        #[cfg(all(feature = "plugins", feature = "runtime_plugins"))]
        let mut active_plugins = runtime
            .plugins_manager()
            .started_plugins_iter()
            .map(|rec| (rec.id().to_string(), rec.path().to_string()))
            .collect::<HashMap<_, _>>();

        let context = Arc::new(AdminContext {
            runtime: runtime.clone(),
            version,
        });
        let admin = Arc::new(AdminSpace {
            zid: runtime.zid(),
            queryable_id: runtime.next_id(),
            primitives: Mutex::new(None),
            mappings: Mutex::new(HashMap::new()),
            handlers,
            context,
        });

        config.set_plugin_validator(Arc::downgrade(&admin));

        #[cfg(all(feature = "plugins", feature = "runtime_plugins"))]
        {
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
                            cfg_guard.0.plugins().load_requests().collect::<Vec<_>>()
                        };
                        let mut diffs = Vec::new();
                        for plugin in active_plugins.keys() {
                            if !requested_plugins.iter().any(|r| &r.id == plugin) {
                                diffs.push(PluginDiff::Delete(plugin.clone()))
                            }
                        }
                        for request in requested_plugins {
                            if let Some(active) = active_plugins.get(&request.id) {
                                if request
                                    .paths
                                    .as_ref()
                                    .map(|p| p.contains(active))
                                    .unwrap_or(true)
                                {
                                    continue;
                                }
                                diffs.push(PluginDiff::Delete(request.id.clone()))
                            }
                            diffs.push(PluginDiff::Start(request))
                        }
                        let mut plugins_mgr = admin.context.runtime.plugins_manager();
                        for diff in diffs {
                            match diff {
                                PluginDiff::Delete(id) => {
                                    active_plugins.remove(id.as_str());
                                    if let Some(running) = plugins_mgr.started_plugin_mut(&id) {
                                        running.stop()
                                    }
                                }
                                PluginDiff::Start(plugin) => {
                                    let dynamic_runtime = admin.context.runtime.clone().into();
                                    if let Err(e) = Self::start_plugin(
                                        &mut plugins_mgr,
                                        &plugin,
                                        &dynamic_runtime,
                                        plugin.required,
                                    ) {
                                        if plugin.required {
                                            panic!("Failed to load plugin `{}`: {}", plugin.id, e)
                                        } else {
                                            tracing::error!(
                                                "Failed to load plugin `{}`: {}",
                                                plugin.id,
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
        }

        let primitives = runtime.state.router.new_primitives(admin.clone());
        zlock!(admin.primitives).replace(primitives.clone());

        primitives.send_declare(&mut Declare {
            interest_id: None,

            ext_qos: ext::QoSType::DECLARE,
            ext_tstamp: None,
            ext_nodeid: ext::NodeIdType::DEFAULT,
            body: DeclareBody::DeclareQueryable(DeclareQueryable {
                id: runtime.next_id(),
                wire_expr: [&root_key, "/**"].concat().into(),
                ext_info: QueryableInfoType::DEFAULT,
            }),
        });

        primitives.send_declare(&mut Declare {
            interest_id: None,
            ext_qos: ext::QoSType::DECLARE,
            ext_tstamp: None,
            ext_nodeid: ext::NodeIdType::DEFAULT,
            body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                id: runtime.next_id(),
                wire_expr: [&root_key, "/config/**"].concat().into(),
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
    fn send_interest(&self, msg: &mut Interest) {
        tracing::trace!("Recv interest {:?}", msg);
    }

    fn send_declare(&self, msg: &mut Declare) {
        tracing::trace!("Recv declare {:?}", msg);
        if let DeclareBody::DeclareKeyExpr(m) = &msg.body {
            match self.key_expr_to_string(&m.wire_expr) {
                Ok(s) => {
                    zlock!(self.mappings).insert(m.id, s.into());
                }
                Err(e) => error!("Unknown expr_id {}! ({})", m.id, e),
            }
        }
    }

    fn send_push(&self, msg: &mut Push, _reliability: Reliability) {
        trace!("recv Push {:?}", msg);
        {
            let conf = &self.context.runtime.state.config.lock().0;
            if !conf.adminspace.permissions().write {
                tracing::error!(
                    "Received PUT on '{}' but adminspace.permissions.write=false in configuration",
                    msg.wire_expr
                );
                return;
            }
        }

        if let Some(key) = msg.wire_expr.as_str().strip_prefix(&format!(
            "@/{}/{}/config/",
            self.context.runtime.state.zid, self.context.runtime.state.whatami,
        )) {
            match &msg.payload {
                PushBody::Put(put) => match std::str::from_utf8(&put.payload.contiguous()) {
                    Ok(json) => {
                        tracing::trace!(
                            "Insert conf value @/{}/{}/config/{} : {}",
                            self.context.runtime.state.zid,
                            self.context.runtime.state.whatami,
                            key,
                            json
                        );
                        if let Err(e) = self.context.runtime.state.config.insert_json5(key, json) {
                            error!(
                                "Error inserting conf value @/{}/{}/config/{} : {} - {}",
                                self.context.runtime.state.zid,
                                self.context.runtime.state.whatami,
                                key,
                                json,
                                e
                            );
                        }
                    }
                    Err(e) => error!(
                        "Received non utf8 conf value on @/{}/{}/config/{} : {}",
                        self.context.runtime.state.zid, self.context.runtime.state.whatami, key, e
                    ),
                },
                PushBody::Del(_) => {
                    tracing::trace!(
                        "Deleting conf value /@/{}/{}/config/{}",
                        self.context.runtime.state.zid,
                        self.context.runtime.state.whatami,
                        key
                    );
                    if let Err(e) = self.context.runtime.state.config.remove(key) {
                        tracing::error!("Error deleting conf value {} : {}", msg.wire_expr, e)
                    }
                }
            }
        }
    }

    fn send_request(&self, msg: &mut Request) {
        trace!("recv Request {:?}", msg);
        match &mut msg.payload {
            RequestBody::Query(query) => {
                let primitives = zlock!(self.primitives).as_ref().unwrap().clone();
                {
                    let conf = &self.context.runtime.state.config.lock().0;
                    if !conf.adminspace.permissions().read {
                        tracing::error!(
                        "Received GET on '{}' but adminspace.permissions.read=false in configuration",
                        msg.wire_expr
                    );
                        primitives.send_response_final(&mut ResponseFinal {
                            rid: msg.id,
                            ext_qos: ext::QoSType::RESPONSE_FINAL,
                            ext_tstamp: None,
                        });
                        return;
                    }
                }

                let key_expr = match self.key_expr_to_string(&msg.wire_expr) {
                    Ok(key_expr) => key_expr.into_owned(),
                    Err(e) => {
                        tracing::error!("Unknown KeyExpr: {}", e);
                        primitives.send_response_final(&mut ResponseFinal {
                            rid: msg.id,
                            ext_qos: ext::QoSType::RESPONSE_FINAL,
                            ext_tstamp: None,
                        });
                        return;
                    }
                };

                let zid = self.zid;
                let query = Query {
                    inner: Arc::new(QueryInner {
                        key_expr: key_expr.clone(),
                        parameters: mem::take(&mut query.parameters).into(),
                        qid: msg.id,
                        zid: zid.into(),
                        primitives,
                    }),
                    eid: self.queryable_id,
                    value: mem::take(&mut query.ext_body)
                        .map(|b| (b.payload.into(), b.encoding.into())),
                    attachment: query.ext_attachment.take().map(Into::into),
                };

                for (key, handler) in &self.handlers {
                    if key_expr.intersects(key) {
                        handler(&self.context, query.clone());
                    }
                }
            }
        }
    }

    fn send_response(&self, msg: &mut Response) {
        trace!("recv Response {:?}", msg);
    }

    fn send_response_final(&self, msg: &mut ResponseFinal) {
        trace!("recv ResponseFinal {:?}", msg);
    }

    fn send_close(&self) {
        trace!("recv Close");
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl crate::net::primitives::EPrimitives for AdminSpace {
    #[inline]
    fn send_interest(&self, ctx: crate::net::routing::RoutingContext<&mut Interest>) {
        (self as &dyn Primitives).send_interest(ctx.msg)
    }

    #[inline]
    fn send_declare(&self, ctx: crate::net::routing::RoutingContext<&mut Declare>) {
        (self as &dyn Primitives).send_declare(ctx.msg)
    }

    #[inline]
    fn send_push(&self, msg: &mut Push, reliability: Reliability) {
        (self as &dyn Primitives).send_push(msg, reliability)
    }

    #[inline]
    fn send_request(&self, msg: &mut Request) {
        (self as &dyn Primitives).send_request(msg)
    }

    #[inline]
    fn send_response(&self, msg: &mut Response) {
        (self as &dyn Primitives).send_response(msg)
    }

    #[inline]
    fn send_response_final(&self, msg: &mut ResponseFinal) {
        (self as &dyn Primitives).send_response_final(msg)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn local_data(context: &AdminContext, query: Query) {
    let reply_key: OwnedKeyExpr = format!(
        "@/{}/{}",
        context.runtime.state.zid, context.runtime.state.whatami
    )
    .try_into()
    .unwrap();

    let transport_mgr = context.runtime.manager().clone();

    #[cfg(feature = "stats")]
    let export_stats = query
        .parameters()
        .iter()
        .any(|(k, v)| k == "_stats" && v != "false");
    #[cfg(feature = "stats")]
    let insert_stats = |mut json: serde_json::Value, stats: Option<&Arc<TransportStats>>| {
        if export_stats {
            json.as_object_mut()
                .unwrap()
                .insert("stats".into(), json!(stats.map(|s| s.report())));
        }
        json
    };

    // plugins info
    #[cfg(feature = "plugins")]
    let plugins: serde_json::Value = {
        let plugins_mgr = context.runtime.plugins_manager();
        plugins_mgr
            .started_plugins_iter()
            .map(|rec| (rec.id(), json!({"name":rec.name(), "path": rec.path() })))
            .collect()
    };
    #[cfg(not(all(feature = "unstable", feature = "plugins")))]
    let plugins = serde_json::Value::Null;

    // locators info
    let locators: Vec<serde_json::Value> = transport_mgr
        .get_locators()
        .iter()
        .map(|locator| json!(locator.as_str()))
        .collect();

    let links_info = context.runtime.get_links_info();
    // transports info
    let transport_unicast_to_json = |transport: &TransportUnicast| {
        let link_to_json = |link: &Link| {
            json!({
                "src": link.src.to_string(),
                "dst": link.dst.to_string()
            })
        };
        #[cfg(not(feature = "stats"))]
        let links = transport
            .get_links()
            .unwrap_or_default()
            .iter()
            .map(link_to_json)
            .collect_vec();
        #[cfg(feature = "stats")]
        let links = transport
            .get_link_stats()
            .unwrap_or_default()
            .iter()
            .map(|(link, stats)| insert_stats(link_to_json(link), Some(stats)))
            .collect_vec();
        #[cfg(feature = "shared-memory")]
        let shm = transport.is_shm().unwrap_or_default();
        #[cfg(not(feature = "shared-memory"))]
        let shm = false;
        let json = json!({
            "peer": transport.get_zid().map_or_else(|_| "unknown".to_string(), |p| p.to_string()),
            "whatami": transport.get_whatami().map_or_else(|_| "unknown".to_string(), |p| p.to_string()),
            "links": links,
            "weight": transport.get_zid().ok().and_then(|zid| links_info.get(&zid)),
            "shm": shm,
        });
        #[cfg(feature = "stats")]
        let json = insert_stats(json, transport.get_stats().ok().as_ref());
        json
    };
    let transport_multicast_peer_to_json =
        |transport: &TransportMulticast, mcast_peer: &TransportPeer| {
            let link_to_json = |link: &Link| {
                json!({
                    "src": link.src.to_string(),
                    "dst": link.dst.to_string()
                })
            };
            let links = mcast_peer.links.iter().map(link_to_json).collect_vec();
            let json = json!({
                "peer": mcast_peer.zid.to_string(),
                "whatami": mcast_peer.whatami.to_string(),
                "group": transport
                    .get_link()
                    .ok()
                    .and_then(|t| t.group.map(|g| g.to_string()))
                    .unwrap_or("unknown".to_string()),

                "links": links,
            });
            #[cfg(feature = "stats")]
            let json = insert_stats(json, transport.get_stats().ok().as_ref());
            json
        };
    let mut transports: Vec<serde_json::Value> = vec![];
    zenoh_runtime::ZRuntime::Net.block_in_place(async {
        for transport in transport_mgr.get_transports_unicast().await {
            transports.push(transport_unicast_to_json(&transport));
        }
        for mcast_transport in transport_mgr.get_transports_multicast().await {
            if let Ok(peers) = mcast_transport.get_peers() {
                for mcast_peer in &peers {
                    transports.push(transport_multicast_peer_to_json(
                        &mcast_transport,
                        mcast_peer,
                    ));
                }
            }
        }
    });
    let json = json!({
        "zid": context.runtime.state.zid,
        "version": context.version,
        "metadata": context.runtime.config().lock().0.metadata(),
        "locators": locators,
        "sessions": transports,
        "plugins": plugins,
    });
    #[cfg(feature = "stats")]
    let json = insert_stats(json, Some(&transport_mgr.get_stats()));

    tracing::trace!("AdminSpace router_data: {:?}", json);
    let payload = match serde_json::to_vec(&json) {
        Ok(bytes) => ZBytes::from(bytes),
        Err(e) => {
            tracing::error!("Error serializing AdminSpace reply: {:?}", e);
            return;
        }
    };
    if let Err(e) = query
        .reply(reply_key, payload)
        .encoding(Encoding::APPLICATION_JSON)
        .wait()
    {
        tracing::error!("Error sending AdminSpace reply: {:?}", e);
    }
}

fn metrics(context: &AdminContext, query: Query) {
    let reply_key: OwnedKeyExpr = format!(
        "@/{}/{}/metrics",
        context.runtime.state.zid, context.runtime.state.whatami
    )
    .try_into()
    .unwrap();
    #[allow(unused_mut)]
    let mut metrics = format!(
        r#"# HELP zenoh_build Information about zenoh.
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
        .reply(reply_key, metrics)
        .encoding(Encoding::TEXT_PLAIN)
        .wait()
    {
        tracing::error!("Error sending AdminSpace reply: {:?}", e);
    }
}

fn routers_linkstate_data(context: &AdminContext, query: Query) {
    let reply_key: OwnedKeyExpr = format!(
        "@/{}/{}/linkstate/routers",
        context.runtime.state.zid, context.runtime.state.whatami
    )
    .try_into()
    .unwrap();

    let tables = &context.runtime.state.router.tables;
    let rtables = zread!(tables.tables);

    if let Err(e) = query
        .reply(reply_key, tables.hat_code.info(&rtables, WhatAmI::Router))
        .encoding(Encoding::TEXT_PLAIN)
        .wait()
    {
        tracing::error!("Error sending AdminSpace reply: {:?}", e);
    }
}

fn peers_linkstate_data(context: &AdminContext, query: Query) {
    let reply_key: OwnedKeyExpr = format!(
        "@/{}/{}/linkstate/peers",
        context.runtime.state.zid, context.runtime.state.whatami
    )
    .try_into()
    .unwrap();

    let tables = &context.runtime.state.router.tables;
    let rtables = zread!(tables.tables);

    if let Err(e) = query
        .reply(reply_key, tables.hat_code.info(&rtables, WhatAmI::Peer))
        .encoding(Encoding::TEXT_PLAIN)
        .wait()
    {
        tracing::error!("Error sending AdminSpace reply: {:?}", e);
    }
}

fn subscribers_data(context: &AdminContext, query: Query) {
    let tables = &context.runtime.state.router.tables;
    let rtables = zread!(tables.tables);
    for sub in tables.hat_code.get_subscriptions(&rtables) {
        let key = KeyExpr::try_from(format!(
            "@/{}/{}/subscriber/{}",
            context.runtime.state.zid,
            context.runtime.state.whatami,
            sub.0.expr()
        ))
        .unwrap();
        if query.key_expr().intersects(&key) {
            let payload =
                ZBytes::from(serde_json::to_string(&sub.1).unwrap_or_else(|_| "{}".to_string()));
            if let Err(e) = query
                .reply(key, payload)
                .encoding(Encoding::APPLICATION_JSON)
                .wait()
            {
                tracing::error!("Error sending AdminSpace reply: {:?}", e);
            }
        }
    }
}

fn publishers_data(context: &AdminContext, query: Query) {
    let tables = &context.runtime.state.router.tables;
    let rtables = zread!(tables.tables);
    for sub in tables.hat_code.get_publications(&rtables) {
        let key = KeyExpr::try_from(format!(
            "@/{}/{}/publisher/{}",
            context.runtime.state.zid,
            context.runtime.state.whatami,
            sub.0.expr()
        ))
        .unwrap();
        if query.key_expr().intersects(&key) {
            let payload =
                ZBytes::from(serde_json::to_string(&sub.1).unwrap_or_else(|_| "{}".to_string()));
            if let Err(e) = query
                .reply(key, payload)
                .encoding(Encoding::APPLICATION_JSON)
                .wait()
            {
                tracing::error!("Error sending AdminSpace reply: {:?}", e);
            }
        }
    }
}

fn queryables_data(context: &AdminContext, query: Query) {
    let tables = &context.runtime.state.router.tables;
    let rtables = zread!(tables.tables);
    for qabl in tables.hat_code.get_queryables(&rtables) {
        let key = KeyExpr::try_from(format!(
            "@/{}/{}/queryable/{}",
            context.runtime.state.zid,
            context.runtime.state.whatami,
            qabl.0.expr()
        ))
        .unwrap();
        if query.key_expr().intersects(&key) {
            let payload =
                ZBytes::from(serde_json::to_string(&qabl.1).unwrap_or_else(|_| "{}".to_string()));
            if let Err(e) = query
                .reply(key, payload)
                .encoding(Encoding::APPLICATION_JSON)
                .wait()
            {
                tracing::error!("Error sending AdminSpace reply: {:?}", e);
            }
        }
    }
}

fn queriers_data(context: &AdminContext, query: Query) {
    let tables = &context.runtime.state.router.tables;
    let rtables = zread!(tables.tables);
    for sub in tables.hat_code.get_queriers(&rtables) {
        let key = KeyExpr::try_from(format!(
            "@/{}/{}/querier/{}",
            context.runtime.state.zid,
            context.runtime.state.whatami,
            sub.0.expr()
        ))
        .unwrap();
        if query.key_expr().intersects(&key) {
            let payload =
                ZBytes::from(serde_json::to_string(&sub.1).unwrap_or_else(|_| "{}".to_string()));
            if let Err(e) = query
                .reply(key, payload)
                .encoding(Encoding::APPLICATION_JSON)
                .wait()
            {
                tracing::error!("Error sending AdminSpace reply: {:?}", e);
            }
        }
    }
}

fn route_successor(context: &AdminContext, query: Query) {
    let reply = |keyexpr: &keyexpr, successor: ZenohIdProto| {
        if let Err(e) = query
            .reply(keyexpr, serde_json::to_vec(&json!(successor)).unwrap())
            .encoding(Encoding::APPLICATION_JSON)
            .wait()
        {
            tracing::error!("Error sending AdminSpace reply: {:?}", e);
        }
    };
    let prefix = format!("@/{}/router/route/successor", context.runtime.zid());
    let tables = &context.runtime.state.router.tables;
    let rtables = zread!(tables.tables);
    // Try to shortcut full successor retrieval if suffix matches 'src/<zid>/dst/<zid>' pattern.

    let suffix = query.key_expr().as_str().strip_prefix(&prefix);
    if let Some((src, dst)) = suffix.and_then(|s| s.strip_prefix("/src/")?.split_once("/dst/")) {
        if let (Ok(src_zid), Ok(dst_zid)) = (src.parse(), dst.parse()) {
            if let Some(successor) = tables.hat_code.route_successor(&rtables, src_zid, dst_zid) {
                reply(query.key_expr(), successor);
                return;
            }
        }
    }
    // Reply with every successor suffix matching the keyexpr.
    let successors = tables.hat_code.route_successors(&rtables);
    drop(rtables);
    for entry in successors.iter() {
        let keyexpr = KeyExpr::new(format!(
            "{prefix}/src/{src}/dst/{dst}",
            src = entry.source,
            dst = entry.destination
        ))
        .unwrap();
        if query.key_expr().intersects(&keyexpr) {
            reply(&keyexpr, entry.successor);
        }
    }
}

#[cfg(feature = "plugins")]
fn plugins_data(context: &AdminContext, query: Query) {
    let guard = context.runtime.plugins_manager();
    let root_key = format!(
        "@/{}/{}/plugins",
        &context.runtime.state.zid, context.runtime.state.whatami
    );
    let root_key = unsafe { keyexpr::from_str_unchecked(&root_key) };
    tracing::debug!("requested plugins status {:?}", query.key_expr());
    if let [names, ..] = query.key_expr().strip_prefix(root_key)[..] {
        let statuses = guard.plugins_status(names);
        for status in statuses {
            tracing::debug!("plugin status: {:?}", status);
            let key = root_key.join(status.id()).unwrap();
            match serde_json::to_vec(&status) {
                Ok(bytes) => {
                    if let Err(e) = query
                        .reply(key, bytes)
                        .encoding(Encoding::APPLICATION_JSON)
                        .wait()
                    {
                        tracing::error!("Error sending AdminSpace reply: {:?}", e);
                    }
                }
                Err(e) => tracing::debug!("Admin query error: {}", e),
            }
        }
    }
}

#[cfg(feature = "plugins")]
fn plugins_status(context: &AdminContext, query: Query) {
    let key_expr = query.key_expr();
    let guard = context.runtime.plugins_manager();
    let mut root_key = format!(
        "@/{}/{}/status/plugins/",
        &context.runtime.state.zid, context.runtime.state.whatami
    );

    for plugin in guard.started_plugins_iter() {
        with_extended_string(&mut root_key, &[plugin.id()], |plugin_key| {
            // @TODO: response to "__version__", this need not to be implemented by each plugin
            with_extended_string(plugin_key, &["/__path__"], |plugin_path_key| {
                if let Ok(key_expr) = KeyExpr::try_from(plugin_path_key.clone()) {
                    if query.key_expr().intersects(&key_expr) {
                        if let Err(e) = query
                            .reply(key_expr, plugin.path())
                            .encoding(Encoding::TEXT_PLAIN)
                            .wait()
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
                plugin.instance().adminspace_getter(key_expr, plugin_key)
            })) {
                Ok(Ok(responses)) => {
                    for response in responses {
                        if let Ok(key_expr) = KeyExpr::try_from(response.key) {
                            match serde_json::to_vec::<serde_json::Value>(&response.value.into()) {
                                Ok(bytes) => {
                                    if let Err(e) = query.reply(key_expr, bytes).encoding(Encoding::APPLICATION_JSON).wait() {
                                        tracing::error!("Error sending AdminSpace reply: {:?}", e);
                                    }
                                }
                                Err(e) => tracing::debug!("Admin query error: {}", e),
                            }
                        } else {
                            tracing::error!("Error: plugin {} replied with an invalid key", plugin_key);
                        }
                    }
                }
                Ok(Err(e)) => {
                    tracing::error!("Plugin {} bailed from responding to {}: {}", plugin.id(), query.key_expr(), e)
                }
                Err(e) => match e
                    .downcast_ref::<String>()
                    .map(|s| s.as_str())
                    .or_else(|| e.downcast_ref::<&str>().copied())
                {
                    Some(e) => tracing::error!("Plugin {} panicked while responding to {}: {}", plugin.id(), query.key_expr(), e),
                    None => tracing::error!("Plugin {} panicked while responding to {}. The panic message couldn't be recovered.", plugin.id(), query.key_expr()),
                },
            }
        });
    }
}

#[cfg(feature = "plugins")]
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
