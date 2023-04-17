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
use base64::{engine::general_purpose::STANDARD as b64_std_engine, Engine};
use futures::StreamExt;
use http_types::Method;
use std::convert::TryFrom;
use std::str::FromStr;
use std::sync::Arc;
use tide::http::Mime;
use tide::sse::Sender;
use tide::{Request, Response, Server, StatusCode};
use zenoh::plugins::{Plugin, RunningPluginTrait, ZenohPlugin};
use zenoh::prelude::r#async::*;
use zenoh::query::{QueryConsolidation, Reply};
use zenoh::runtime::Runtime;
use zenoh::selector::TIME_RANGE_KEY;
use zenoh::Session;
use zenoh_cfg_properties::Properties;
use zenoh_result::{bail, zerror, ZResult};

mod config;
pub use config::Config;

const GIT_VERSION: &str = git_version::git_version!(prefix = "v", cargo_prefix = "v");
lazy_static::lazy_static! {
    static ref LONG_VERSION: String = format!("{} built with {}", GIT_VERSION, env!("RUSTC_VERSION"));
}

fn value_to_json(value: Value) -> String {
    // @TODO: transcode to JSON when implemented in Value
    match &value.encoding {
        p if p.starts_with(KnownEncoding::TextPlain) => {
            // convert to Json string for special characters escaping
            serde_json::json!(value.to_string()).to_string()
        }
        p if p.starts_with(KnownEncoding::AppProperties) => {
            // convert to Json string for special characters escaping
            serde_json::json!(*Properties::from(value.to_string())).to_string()
        }
        p if p.starts_with(KnownEncoding::AppJson)
            || p.starts_with(KnownEncoding::AppXWwwFormUrlencoded)
            || p.starts_with(KnownEncoding::AppInteger)
            || p.starts_with(KnownEncoding::AppFloat) =>
        {
            value.to_string()
        }
        _ => {
            format!(r#""{}""#, b64_std_engine.encode(value.payload.contiguous()))
        }
    }
}

fn sample_to_json(sample: Sample) -> String {
    let encoding = sample.value.encoding.to_string();
    format!(
        r#"{{ "key": "{}", "value": {}, "encoding": "{}", "time": "{}" }}"#,
        sample.key_expr.as_str(),
        value_to_json(sample.value),
        encoding,
        if let Some(ts) = sample.timestamp {
            ts.to_string()
        } else {
            "None".to_string()
        }
    )
}

fn result_to_json(sample: Result<Sample, Value>) -> String {
    match sample {
        Ok(sample) => sample_to_json(sample),
        Err(err) => {
            let encoding = err.encoding.to_string();
            format!(
                r#"{{ "key": "ERROR", "value": {}, "encoding": "{}"}}"#,
                value_to_json(err),
                encoding,
            )
        }
    }
}

async fn to_json(results: flume::Receiver<Reply>) -> String {
    let values = results
        .stream()
        .filter_map(move |reply| async move { Some(result_to_json(reply.sample)) })
        .collect::<Vec<String>>()
        .await
        .join(",\n");
    format!("[\n{values}\n]\n")
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

fn method_to_kind(method: Method) -> SampleKind {
    match method {
        Method::Put => SampleKind::Put,
        Method::Delete => SampleKind::Delete,
        _ => SampleKind::default(),
    }
}

fn response(status: StatusCode, content_type: Mime, body: &str) -> Response {
    Response::builder(status)
        .header("content-length", body.len().to_string())
        .header("Access-Control-Allow-Origin", "*")
        .content_type(content_type)
        .body(body)
        .build()
}

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
    type RunningPlugin = zenoh::plugins::RunningPlugin;
    const STATIC_NAME: &'static str = "rest";

    fn start(name: &str, runtime: &Self::StartArgs) -> ZResult<zenoh::plugins::RunningPlugin> {
        // Try to initiate login.
        // Required in case of dynamic lib, otherwise no logs.
        // But cannot be done twice in case of static link.
        let _ = env_logger::try_init();
        log::debug!("REST plugin {}", LONG_VERSION.as_str());

        let runtime_conf = runtime.config.lock();
        let plugin_conf = runtime_conf
            .plugin(name)
            .ok_or_else(|| zerror!("Plugin `{}`: missing config", name))?;

        let conf: Config = serde_json::from_value(plugin_conf.clone())
            .map_err(|e| zerror!("Plugin `{}` configuration error: {}", name, e))?;
        async_std::task::spawn(run(runtime.clone(), conf.clone()));
        Ok(Box::new(RunningPlugin(conf)))
    }
}

struct RunningPlugin(Config);
impl RunningPluginTrait for RunningPlugin {
    fn config_checker(&self) -> zenoh::plugins::ValidationFunction {
        Arc::new(|_, _, _| {
            bail!("zenoh-plugin-rest doesn't accept any runtime configuration changes")
        })
    }

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

async fn query(req: Request<(Arc<Session>, String)>) -> tide::Result<Response> {
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
                        match sender
                            .send(&sample.kind.to_string(), sample_to_json(sample), None)
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
        let url = req.url();
        let key_expr = match path_to_key_expr(url.path(), &req.state().1) {
            Ok(ke) => ke,
            Err(e) => {
                return Ok(response(
                    StatusCode::BadRequest,
                    Mime::from_str("text/plain").unwrap(),
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
        match req
            .state()
            .0
            .get(&selector)
            .consolidation(consolidation)
            .res()
            .await
        {
            Ok(receiver) => {
                if first_accept == "text/html" {
                    Ok(response(
                        StatusCode::Ok,
                        Mime::from_str("text/html").unwrap(),
                        &to_html(receiver).await,
                    ))
                } else {
                    Ok(response(
                        StatusCode::Ok,
                        Mime::from_str("application/json").unwrap(),
                        &to_json(receiver).await,
                    ))
                }
            }
            Err(e) => Ok(response(
                StatusCode::InternalServerError,
                Mime::from_str("text/plain").unwrap(),
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
                        Mime::from_str("text/plain").unwrap(),
                        &e.to_string(),
                    ))
                }
            };
            let encoding: Encoding = req
                .content_type()
                .map(|m| m.essence().to_owned().into())
                .unwrap_or_default();

            // @TODO: Define the right congestion control value
            match req
                .state()
                .0
                .put(&key_expr, bytes)
                .encoding(encoding)
                .kind(method_to_kind(req.method()))
                .res()
                .await
            {
                Ok(_) => Ok(Response::new(StatusCode::Ok)),
                Err(e) => Ok(response(
                    StatusCode::InternalServerError,
                    Mime::from_str("text/plain").unwrap(),
                    &e.to_string(),
                )),
            }
        }
        Err(e) => Ok(response(
            StatusCode::NoContent,
            Mime::from_str("text/plain").unwrap(),
            &e.to_string(),
        )),
    }
}

pub async fn run(runtime: Runtime, conf: Config) {
    // Try to initiate login.
    // Required in case of dynamic lib, otherwise no logs.
    // But cannot be done twice in case of static link.
    let _ = env_logger::try_init();

    let zid = runtime.zid.to_string();
    let session = zenoh::init(runtime).res().await.unwrap();

    let mut app = Server::with_state((Arc::new(session), zid));
    app.with(
        tide::security::CorsMiddleware::new()
            .allow_methods(
                "GET, PUT, PATCH, DELETE"
                    .parse::<http_types::headers::HeaderValue>()
                    .unwrap(),
            )
            .allow_origin(tide::security::Origin::from("*"))
            .allow_credentials(false),
    );

    app.at("/").get(query).put(write).patch(write).delete(write);
    app.at("*").get(query).put(write).patch(write).delete(write);

    if let Err(e) = app.listen(conf.http_port).await {
        log::error!("Unable to start http server for REST: {:?}", e);
    }
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
