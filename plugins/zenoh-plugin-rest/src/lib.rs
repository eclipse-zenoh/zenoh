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
//

//! ⚠️ WARNING ⚠️
//!
//! This crate is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](../zenoh/index.html)
use std::{borrow::Cow, convert::TryFrom, str::FromStr, sync::Arc};

use async_std::prelude::FutureExt;
use base64::Engine;
use futures::StreamExt;
use http_types::Method;
use serde::{Deserialize, Serialize};
use tide::{http::Mime, sse::Sender, Request, Response, Server, StatusCode};
use zenoh::{
    bytes::{StringOrBase64, ZBytes},
    encoding::Encoding,
    key_expr::{keyexpr, KeyExpr},
    plugins::{RunningPluginTrait, ZenohPlugin},
    query::{QueryConsolidation, Reply},
    runtime::Runtime,
    sample::{Sample, SampleKind, ValueBuilderTrait},
    selector::{Selector, TIME_RANGE_KEY},
    session::{Session, SessionDeclarations},
    value::Value,
};
use zenoh_plugin_trait::{plugin_long_version, plugin_version, Plugin, PluginControl};
use zenoh_result::{bail, zerror, ZResult};

mod config;
pub use config::Config;

const GIT_VERSION: &str = git_version::git_version!(prefix = "v", cargo_prefix = "v");
lazy_static::lazy_static! {
    static ref LONG_VERSION: String = format!("{} built with {}", GIT_VERSION, env!("RUSTC_VERSION"));
}
const RAW_KEY: &str = "_raw";

#[derive(Serialize, Deserialize)]
struct JSONSample {
    key: String,
    value: serde_json::Value,
    encoding: String,
    time: Option<String>,
}

pub fn base64_encode(data: &[u8]) -> String {
    use base64::engine::general_purpose;
    general_purpose::STANDARD.encode(data)
}

fn payload_to_json(payload: &ZBytes, encoding: &Encoding) -> serde_json::Value {
    match payload.is_empty() {
        // If the value is empty return a JSON null
        true => serde_json::Value::Null,
        // if it is not check the encoding
        false => {
            match encoding {
                // If it is a JSON try to deserialize as json, if it fails fallback to base64
                &Encoding::APPLICATION_JSON | &Encoding::TEXT_JSON | &Encoding::TEXT_JSON5 => {
                    payload
                        .deserialize::<serde_json::Value>()
                        .unwrap_or_else(|_| {
                            serde_json::Value::String(StringOrBase64::from(payload).into_string())
                        })
                }
                // otherwise convert to JSON string
                _ => serde_json::Value::String(StringOrBase64::from(payload).into_string()),
            }
        }
    }
}

fn sample_to_json(sample: &Sample) -> JSONSample {
    JSONSample {
        key: sample.key_expr().as_str().to_string(),
        value: payload_to_json(sample.payload(), sample.encoding()),
        encoding: sample.encoding().to_string(),
        time: sample.timestamp().map(|ts| ts.to_string()),
    }
}

fn result_to_json(sample: Result<&Sample, &Value>) -> JSONSample {
    match sample {
        Ok(sample) => sample_to_json(sample),
        Err(err) => JSONSample {
            key: "ERROR".into(),
            value: payload_to_json(err.payload(), err.encoding()),
            encoding: err.encoding().to_string(),
            time: None,
        },
    }
}

async fn to_json(results: flume::Receiver<Reply>) -> String {
    let values = results
        .stream()
        .filter_map(move |reply| async move { Some(result_to_json(reply.result())) })
        .collect::<Vec<JSONSample>>()
        .await;

    serde_json::to_string(&values).unwrap_or("[]".into())
}

async fn to_json_response(results: flume::Receiver<Reply>) -> Response {
    response(
        StatusCode::Ok,
        Mime::from_str("application/json").unwrap(),
        &to_json(results).await,
    )
}

fn sample_to_html(sample: &Sample) -> String {
    format!(
        "<dt>{}</dt>\n<dd>{}</dd>\n",
        sample.key_expr().as_str(),
        sample
            .payload()
            .deserialize::<Cow<str>>()
            .unwrap_or_default()
    )
}

fn result_to_html(sample: Result<&Sample, &Value>) -> String {
    match sample {
        Ok(sample) => sample_to_html(sample),
        Err(err) => {
            format!(
                "<dt>ERROR</dt>\n<dd>{}</dd>\n",
                err.payload().deserialize::<Cow<str>>().unwrap_or_default()
            )
        }
    }
}

async fn to_html(results: flume::Receiver<Reply>) -> String {
    let values = results
        .stream()
        .filter_map(move |reply| async move { Some(result_to_html(reply.result())) })
        .collect::<Vec<String>>()
        .await
        .join("\n");
    format!("<dl>\n{values}\n</dl>\n")
}

async fn to_html_response(results: flume::Receiver<Reply>) -> Response {
    response(StatusCode::Ok, "text/html", &to_html(results).await)
}

async fn to_raw_response(results: flume::Receiver<Reply>) -> Response {
    match results.recv_async().await {
        Ok(reply) => match reply.result() {
            Ok(sample) => response(
                StatusCode::Ok,
                Cow::from(sample.encoding()).as_ref(),
                &sample
                    .payload()
                    .deserialize::<Cow<str>>()
                    .unwrap_or_default(),
            ),
            Err(value) => response(
                StatusCode::Ok,
                Cow::from(value.encoding()).as_ref(),
                &value
                    .payload()
                    .deserialize::<Cow<str>>()
                    .unwrap_or_default(),
            ),
        },
        Err(_) => response(StatusCode::Ok, "", ""),
    }
}

fn method_to_kind(method: Method) -> SampleKind {
    match method {
        Method::Put => SampleKind::Put,
        Method::Delete => SampleKind::Delete,
        _ => SampleKind::default(),
    }
}

fn response(status: StatusCode, content_type: impl TryInto<Mime>, body: &str) -> Response {
    let mut builder = Response::builder(status)
        .header("content-length", body.len().to_string())
        .header("Access-Control-Allow-Origin", "*")
        .body(body);
    if let Ok(mime) = content_type.try_into() {
        builder = builder.content_type(mime);
    }
    builder.build()
}

#[cfg(feature = "dynamic_plugin")]
zenoh_plugin_trait::declare_plugin!(RestPlugin);

pub struct RestPlugin {}

impl ZenohPlugin for RestPlugin {}

impl Plugin for RestPlugin {
    type StartArgs = Runtime;
    type Instance = zenoh::plugins::RunningPlugin;
    const DEFAULT_NAME: &'static str = "rest";
    const PLUGIN_VERSION: &'static str = plugin_version!();
    const PLUGIN_LONG_VERSION: &'static str = plugin_long_version!();

    fn start(name: &str, runtime: &Self::StartArgs) -> ZResult<zenoh::plugins::RunningPlugin> {
        // Try to initiate login.
        // Required in case of dynamic lib, otherwise no logs.
        // But cannot be done twice in case of static link.
        zenoh_util::try_init_log_from_env();
        tracing::debug!("REST plugin {}", LONG_VERSION.as_str());

        let runtime_conf = runtime.config().lock();
        let plugin_conf = runtime_conf
            .plugin(name)
            .ok_or_else(|| zerror!("Plugin `{}`: missing config", name))?;

        let conf: Config = serde_json::from_value(plugin_conf.clone())
            .map_err(|e| zerror!("Plugin `{}` configuration error: {}", name, e))?;
        let task = async_std::task::spawn(run(runtime.clone(), conf.clone()));
        let task = async_std::task::block_on(task.timeout(std::time::Duration::from_millis(1)));
        if let Ok(Err(e)) = task {
            bail!("REST server failed within 1ms: {e}")
        }
        Ok(Box::new(RunningPlugin(conf)))
    }
}

struct RunningPlugin(Config);

impl PluginControl for RunningPlugin {}

impl RunningPluginTrait for RunningPlugin {
    fn adminspace_getter<'a>(
        &'a self,
        selector: &'a Selector<'a>,
        plugin_status_key: &str,
    ) -> ZResult<Vec<zenoh::plugins::Response>> {
        let mut responses = Vec::new();
        let mut key = String::from(plugin_status_key);
        with_extended_string(&mut key, &["/version"], |key| {
            if keyexpr::new(key.as_str())
                .unwrap()
                .intersects(selector.key_expr())
            {
                responses.push(zenoh::plugins::Response::new(
                    key.clone(),
                    GIT_VERSION.into(),
                ))
            }
        });
        with_extended_string(&mut key, &["/port"], |port_key| {
            if keyexpr::new(port_key.as_str())
                .unwrap()
                .intersects(selector.key_expr())
            {
                responses.push(zenoh::plugins::Response::new(
                    port_key.clone(),
                    (&self.0).into(),
                ))
            }
        });
        Ok(responses)
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

async fn query(mut req: Request<(Arc<Session>, String)>) -> tide::Result<Response> {
    tracing::trace!("Incoming GET request: {:?}", req);

    let first_accept = match req.header("accept") {
        Some(accept) => accept[0]
            .to_string()
            .split(';')
            .next()
            .unwrap()
            .split(',')
            .next()
            .unwrap()
            .to_string(),
        None => "application/json".to_string(),
    };
    if first_accept == "text/event-stream" {
        Ok(tide::sse::upgrade(
            req,
            move |req: Request<(Arc<Session>, String)>, sender: Sender| async move {
                let key_expr = match path_to_key_expr(req.url().path(), &req.state().1) {
                    Ok(ke) => ke.into_owned(),
                    Err(e) => {
                        return Err(tide::Error::new(
                            tide::StatusCode::BadRequest,
                            anyhow::anyhow!("{}", e),
                        ))
                    }
                };
                async_std::task::spawn(async move {
                    tracing::debug!(
                        "Subscribe to {} for SSE stream (task {})",
                        key_expr,
                        async_std::task::current().id()
                    );
                    let sender = &sender;
                    let sub = req.state().0.declare_subscriber(&key_expr).await.unwrap();
                    loop {
                        let sample = sub.recv_async().await.unwrap();
                        let json_sample =
                            serde_json::to_string(&sample_to_json(&sample)).unwrap_or("{}".into());

                        match sender
                            .send(&sample.kind().to_string(), json_sample, None)
                            .timeout(std::time::Duration::new(10, 0))
                            .await
                        {
                            Ok(Ok(_)) => {}
                            Ok(Err(e)) => {
                                tracing::debug!(
                                    "SSE error ({})! Unsubscribe and terminate (task {})",
                                    e,
                                    async_std::task::current().id()
                                );
                                if let Err(e) = sub.undeclare().await {
                                    tracing::error!("Error undeclaring subscriber: {}", e);
                                }
                                break;
                            }
                            Err(_) => {
                                tracing::debug!(
                                    "SSE timeout! Unsubscribe and terminate (task {})",
                                    async_std::task::current().id()
                                );
                                if let Err(e) = sub.undeclare().await {
                                    tracing::error!("Error undeclaring subscriber: {}", e);
                                }
                                break;
                            }
                        }
                    }
                });
                Ok(())
            },
        ))
    } else {
        let body = req.body_bytes().await.unwrap_or_default();
        let url = req.url();
        let key_expr = match path_to_key_expr(url.path(), &req.state().1) {
            Ok(ke) => ke,
            Err(e) => {
                return Ok(response(
                    StatusCode::BadRequest,
                    "text/plain",
                    &e.to_string(),
                ))
            }
        };
        let query_part = url.query();
        let selector = if let Some(q) = query_part {
            Selector::new(key_expr, q)
        } else {
            key_expr.into()
        };
        let consolidation = if selector.parameters().contains_key(TIME_RANGE_KEY) {
            QueryConsolidation::from(zenoh::query::ConsolidationMode::None)
        } else {
            QueryConsolidation::from(zenoh::query::ConsolidationMode::Latest)
        };
        let raw = selector.parameters().contains_key(RAW_KEY);
        let mut query = req.state().0.get(&selector).consolidation(consolidation);
        if !body.is_empty() {
            let encoding: Encoding = req
                .content_type()
                .map(|m| Encoding::from(m.to_string()))
                .unwrap_or_default();
            query = query.payload(body).encoding(encoding);
        }
        match query.await {
            Ok(receiver) => {
                if raw {
                    Ok(to_raw_response(receiver).await)
                } else if first_accept == "text/html" {
                    Ok(to_html_response(receiver).await)
                } else {
                    Ok(to_json_response(receiver).await)
                }
            }
            Err(e) => Ok(response(
                StatusCode::InternalServerError,
                "text/plain",
                &e.to_string(),
            )),
        }
    }
}

async fn write(mut req: Request<(Arc<Session>, String)>) -> tide::Result<Response> {
    tracing::trace!("Incoming PUT request: {:?}", req);
    match req.body_bytes().await {
        Ok(bytes) => {
            let key_expr = match path_to_key_expr(req.url().path(), &req.state().1) {
                Ok(ke) => ke,
                Err(e) => {
                    return Ok(response(
                        StatusCode::BadRequest,
                        "text/plain",
                        &e.to_string(),
                    ))
                }
            };

            let encoding: Encoding = req
                .content_type()
                .map(|m| Encoding::from(m.to_string()))
                .unwrap_or_default();

            // @TODO: Define the right congestion control value
            let session = &req.state().0;
            let res = match method_to_kind(req.method()) {
                SampleKind::Put => session.put(&key_expr, bytes).encoding(encoding).await,
                SampleKind::Delete => session.delete(&key_expr).await,
            };
            match res {
                Ok(_) => Ok(Response::new(StatusCode::Ok)),
                Err(e) => Ok(response(
                    StatusCode::InternalServerError,
                    "text/plain",
                    &e.to_string(),
                )),
            }
        }
        Err(e) => Ok(response(
            StatusCode::NoContent,
            "text/plain",
            &e.to_string(),
        )),
    }
}

pub async fn run(runtime: Runtime, conf: Config) -> ZResult<()> {
    // Try to initiate login.
    // Required in case of dynamic lib, otherwise no logs.
    // But cannot be done twice in case of static link.
    zenoh_util::try_init_log_from_env();

    let zid = runtime.zid().to_string();
    let session = zenoh::session::init(runtime).await.unwrap();

    let mut app = Server::with_state((Arc::new(session), zid));
    app.with(
        tide::security::CorsMiddleware::new()
            .allow_methods(
                "GET, POST, PUT, PATCH, DELETE"
                    .parse::<http_types::headers::HeaderValue>()
                    .unwrap(),
            )
            .allow_origin(tide::security::Origin::from("*"))
            .allow_credentials(false),
    );

    app.at("/")
        .get(query)
        .post(query)
        .put(write)
        .patch(write)
        .delete(write);
    app.at("*")
        .get(query)
        .post(query)
        .put(write)
        .patch(write)
        .delete(write);

    if let Err(e) = app.listen(conf.http_port).await {
        tracing::error!("Unable to start http server for REST: {:?}", e);
        return Err(e.into());
    }
    Ok(())
}

fn path_to_key_expr<'a>(path: &'a str, zid: &str) -> ZResult<KeyExpr<'a>> {
    let path = path.strip_prefix('/').unwrap_or(path);
    if path == "@/router/local" {
        KeyExpr::try_from(format!("@/router/{zid}"))
    } else if let Some(suffix) = path.strip_prefix("@/router/local/") {
        KeyExpr::try_from(format!("@/router/{zid}/{suffix}"))
    } else {
        KeyExpr::try_from(path)
    }
}
