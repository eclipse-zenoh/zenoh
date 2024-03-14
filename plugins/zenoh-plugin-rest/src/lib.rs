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
use async_std::prelude::FutureExt;
use base64::Engine;
use futures::StreamExt;
use http_types::Method;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::convert::TryFrom;
use std::str::FromStr;
use std::sync::Arc;
use tide::http::Mime;
use tide::sse::Sender;
use tide::{Request, Response, Server, StatusCode};
use zenoh::plugins::{RunningPluginTrait, ZenohPlugin};
use zenoh::prelude::r#async::*;
use zenoh::query::{QueryConsolidation, Reply};
use zenoh::runtime::Runtime;
use zenoh::selector::TIME_RANGE_KEY;
use zenoh::Session;
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

fn payload_to_string(payload: &Payload) -> String {
    String::from_utf8(payload.contiguous().to_vec()).unwrap_or(base64_encode(&payload.contiguous()))
}

fn payload_to_json(payload: Payload, encoding: &Encoding) -> serde_json::Value {
    match payload.len() {
        // If the value is empty return a JSON null
        0 => serde_json::Value::Null,
        // if it is not check the encoding
        _ => {
            match encoding {
                // If it is a JSON try to deserialize as json, if it fails fallback to base64
                &Encoding::APPLICATION_JSON | &Encoding::TEXT_JSON | &Encoding::TEXT_JSON5 => {
                    serde_json::from_slice::<serde_json::Value>(&payload.contiguous())
                        .unwrap_or(serde_json::Value::String(payload_to_string(&payload)))
                }
                // otherwise convert to JSON string
                _ => serde_json::Value::String(payload_to_string(&payload)),
            }
        }
    }
}

fn sample_to_json(sample: Sample) -> JSONSample {
    JSONSample {
        key: sample.key_expr.as_str().to_string(),
        value: payload_to_json(sample.payload, &sample.encoding),
        encoding: sample.encoding.to_string(),
        time: sample.timestamp.map(|ts| ts.to_string()),
    }
}

fn result_to_json(sample: Result<Sample, Value>) -> JSONSample {
    match sample {
        Ok(sample) => sample_to_json(sample),
        Err(err) => JSONSample {
            key: "ERROR".into(),
            value: payload_to_json(err.payload, &err.encoding),
            encoding: err.encoding.to_string(),
            time: None,
        },
    }
}

async fn to_json(results: flume::Receiver<Reply>) -> String {
    let values = results
        .stream()
        .filter_map(move |reply| async move { Some(result_to_json(reply.sample)) })
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

fn sample_to_html(sample: Sample) -> String {
    format!(
        "<dt>{}</dt>\n<dd>{}</dd>\n",
        sample.key_expr.as_str(),
        String::from_utf8_lossy(&sample.payload.contiguous())
    )
}

fn result_to_html(sample: Result<Sample, Value>) -> String {
    match sample {
        Ok(sample) => sample_to_html(sample),
        Err(err) => {
            format!(
                "<dt>ERROR</dt>\n<dd>{}</dd>\n",
                String::from_utf8_lossy(&err.payload.contiguous())
            )
        }
    }
}

async fn to_html(results: flume::Receiver<Reply>) -> String {
    let values = results
        .stream()
        .filter_map(move |reply| async move { Some(result_to_html(reply.sample)) })
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
        Ok(reply) => match reply.sample {
            Ok(sample) => response(
                StatusCode::Ok,
                Cow::from(&sample.encoding).as_ref(),
                String::from_utf8_lossy(&sample.payload.contiguous()).as_ref(),
            ),
            Err(value) => response(
                StatusCode::Ok,
                Cow::from(&value.encoding).as_ref(),
                String::from_utf8_lossy(&value.payload.contiguous()).as_ref(),
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

#[cfg(feature = "no_mangle")]
zenoh_plugin_trait::declare_plugin!(RestPlugin);

pub struct RestPlugin {}
#[derive(Clone, Copy, Debug)]
struct StrError {
    err: &'static str,
}
impl std::error::Error for StrError {}
impl std::fmt::Display for StrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.err)
    }
}
#[derive(Debug, Clone)]
struct StringError {
    err: String,
}
impl std::error::Error for StringError {}
impl std::fmt::Display for StringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.err)
    }
}

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
        let _ = env_logger::try_init();
        log::debug!("REST plugin {}", LONG_VERSION.as_str());

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
                .intersects(&selector.key_expr)
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
                .intersects(&selector.key_expr)
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
    log::trace!("Incoming GET request: {:?}", req);

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
                    log::debug!(
                        "Subscribe to {} for SSE stream (task {})",
                        key_expr,
                        async_std::task::current().id()
                    );
                    let sender = &sender;
                    let sub = req
                        .state()
                        .0
                        .declare_subscriber(&key_expr)
                        .res()
                        .await
                        .unwrap();
                    loop {
                        let sample = sub.recv_async().await.unwrap();
                        let kind = sample.kind;
                        let json_sample =
                            serde_json::to_string(&sample_to_json(sample)).unwrap_or("{}".into());

                        match sender
                            .send(&kind.to_string(), json_sample, None)
                            .timeout(std::time::Duration::new(10, 0))
                            .await
                        {
                            Ok(Ok(_)) => {}
                            Ok(Err(e)) => {
                                log::debug!(
                                    "SSE error ({})! Unsubscribe and terminate (task {})",
                                    e,
                                    async_std::task::current().id()
                                );
                                if let Err(e) = sub.undeclare().res().await {
                                    log::error!("Error undeclaring subscriber: {}", e);
                                }
                                break;
                            }
                            Err(_) => {
                                log::debug!(
                                    "SSE timeout! Unsubscribe and terminate (task {})",
                                    async_std::task::current().id()
                                );
                                if let Err(e) = sub.undeclare().res().await {
                                    log::error!("Error undeclaring subscriber: {}", e);
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
            Selector::from(key_expr).with_parameters(q)
        } else {
            key_expr.into()
        };
        let consolidation = if selector.decode().any(|(k, _)| k.as_ref() == TIME_RANGE_KEY) {
            QueryConsolidation::from(zenoh::query::ConsolidationMode::None)
        } else {
            QueryConsolidation::from(zenoh::query::ConsolidationMode::Latest)
        };
        let raw = selector.decode().any(|(k, _)| k.as_ref() == RAW_KEY);
        let mut query = req.state().0.get(&selector).consolidation(consolidation);
        if !body.is_empty() {
            let encoding: Encoding = req
                .content_type()
                .map(|m| Encoding::from(m.to_string()))
                .unwrap_or_default();
            query = query.with_value(Value::from(body).with_encoding(encoding));
        }
        match query.res().await {
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
    log::trace!("Incoming PUT request: {:?}", req);
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
                SampleKind::Put => {
                    session
                        .put(&key_expr, bytes)
                        .with_encoding(encoding)
                        .res()
                        .await
                }
                SampleKind::Delete => session.delete(&key_expr).res().await,
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
    let _ = env_logger::try_init();

    let zid = runtime.zid().to_string();
    let session = zenoh::init(runtime).res().await.unwrap();

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
        log::error!("Unable to start http server for REST: {:?}", e);
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
