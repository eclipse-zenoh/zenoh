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
//

use async_std::sync::Arc;
use clap::{Arg, ArgMatches};
use futures::prelude::*;
use http_types::Method;
use std::str::FromStr;
use tide::http::Mime;
use tide::sse::Sender;
use tide::{Request, Response, Server, StatusCode};
use zenoh::net::runtime::Runtime;
use zenoh::prelude::*;
use zenoh::query::{QueryConsolidation, ReplyReceiver};
use zenoh::Session;
use zenoh_plugin_trait::prelude::*;

const PORT_SEPARATOR: char = ':';
const DEFAULT_HTTP_HOST: &str = "0.0.0.0";
const DEFAULT_HTTP_PORT: &str = "8000";

fn parse_http_port(arg: &str) -> String {
    match arg.split(':').count() {
        1 => {
            match arg.parse::<u16>() {
                Ok(_) => [DEFAULT_HTTP_HOST, arg].join(&PORT_SEPARATOR.to_string()), // port only
                Err(_) => [arg, DEFAULT_HTTP_PORT].join(&PORT_SEPARATOR.to_string()), // host only
            }
        }
        _ => arg.to_string(),
    }
}

fn value_to_json(value: Value) -> String {
    // @TODO: transcode to JSON when implemented in Value
    match &value.encoding {
        p if p.starts_with(&Encoding::STRING) => {
            // convert to Json string for special characters escaping
            serde_json::json!(value.to_string()).to_string()
        }
        p if p.starts_with(&Encoding::APP_PROPERTIES) => {
            // convert to Json string for special characters escaping
            serde_json::json!(*crate::Properties::from(value.to_string())).to_string()
        }
        p if p.starts_with(&Encoding::APP_JSON) => value.to_string(),
        p if p.starts_with(&Encoding::APP_INTEGER) || p.starts_with(&Encoding::APP_FLOAT) => {
            value.to_string()
        }
        _ => {
            format!(r#""{}""#, value.to_string())
        }
    }
}

fn sample_to_json(sample: Sample) -> String {
    let encoding = sample.value.encoding.to_string();
    format!(
        r#"{{ "key": "{}", "value": {}, "encoding": "{}", "time": "{}" }}"#,
        sample.res_name,
        value_to_json(sample.value),
        encoding,
        if let Some(ts) = sample.timestamp {
            ts.to_string()
        } else {
            "None".to_string()
        }
    )
}

async fn to_json(results: ReplyReceiver) -> String {
    let values = results
        .filter_map(move |reply| async move { Some(sample_to_json(reply.data)) })
        .collect::<Vec<String>>()
        .await
        .join(",\n");
    format!("[\n{}\n]\n", values)
}

fn sample_to_html(sample: Sample) -> String {
    format!(
        "<dt>{}</dt>\n<dd>{}</dd>\n",
        sample.res_name,
        String::from_utf8_lossy(&sample.value.payload.contiguous())
    )
}

async fn to_html(results: ReplyReceiver) -> String {
    let values = results
        .filter_map(move |reply| async move { Some(sample_to_html(reply.data)) })
        .collect::<Vec<String>>()
        .await
        .join("\n");
    format!("<dl>\n{}\n</dl>\n", values)
}

fn method_to_kind(method: Method) -> SampleKind {
    match method {
        Method::Put => SampleKind::Put,
        Method::Patch => SampleKind::Patch,
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

impl Plugin for RestPlugin {
    fn compatibility() -> zenoh_plugin_trait::PluginId {
        zenoh_plugin_trait::PluginId {
            uid: "zenoh-plugin-rest",
        }
    }

    type Requirements = Vec<Arg<'static, 'static>>;
    type StartArgs = (Runtime, ArgMatches<'static>);

    fn get_requirements() -> Self::Requirements {
        vec![
            Arg::from_usage("--rest-http-port 'The REST plugin's http port'")
                .default_value(DEFAULT_HTTP_PORT),
        ]
    }

    fn start(
        (runtime, args): &Self::StartArgs,
    ) -> Result<Box<dyn std::any::Any + Send + Sync>, Box<dyn std::error::Error>> {
        match args.value_of("rest-http-port") {
            None => Err(Box::new(StrError {
                err: "No --rest-http-port argument found",
            })),
            Some(port) => {
                async_std::task::spawn(run(runtime.clone(), port.to_owned()));
                Ok(Box::new(()))
            }
        }
    }
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
                let resource = path_to_resource(req.url().path(), &req.state().1).to_owned();
                async_std::task::spawn(async move {
                    log::debug!(
                        "Subscribe to {} for SSE stream (task {})",
                        resource,
                        async_std::task::current().id()
                    );
                    let sender = &sender;
                    let mut sub = req.state().0.subscribe(&resource).await.unwrap();
                    loop {
                        let sample = sub.receiver().next().await.unwrap();
                        let send = async {
                            if let Err(e) = sender
                                .send(&sample.kind.to_string(), sample_to_json(sample), None)
                                .await
                            {
                                log::warn!("Error sending data from the SSE stream: {}", e);
                            }
                            true
                        };
                        let wait = async {
                            async_std::task::sleep(std::time::Duration::new(10, 0)).await;
                            false
                        };
                        if !async_std::prelude::FutureExt::race(send, wait).await {
                            log::debug!(
                                "SSE timeout! Unsubscribe and terminate (task {})",
                                async_std::task::current().id()
                            );
                            if let Err(e) = sub.unregister().await {
                                log::error!("Error undeclaring subscriber: {}", e);
                            }
                            break;
                        }
                    }
                });
                Ok(())
            },
        ))
    } else {
        let url = req.url();
        let resource = path_to_resource(url.path(), &req.state().1);
        let selector = if let Some(q) = url.query() {
            KeyedSelector::from(resource).with_value_selector(q)
        } else {
            resource.into()
        };
        let consolidation = if selector.has_time_range() {
            QueryConsolidation::none()
        } else {
            QueryConsolidation::default()
        };
        match req
            .state()
            .0
            .get(&selector)
            .consolidation(consolidation)
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
            let resource = path_to_resource(req.url().path(), &req.state().1);
            let encoding: Encoding = req.content_type().map(|m| m.into()).unwrap_or_default();

            // @TODO: Define the right congestion control value
            match req
                .state()
                .0
                .put(&resource, bytes)
                .encoding(encoding)
                .kind(method_to_kind(req.method()))
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

pub async fn run(runtime: Runtime, port: String) {
    // Try to initiate login.
    // Required in case of dynamic lib, otherwise no logs.
    // But cannot be done twice in case of static link.
    let _ = env_logger::try_init();

    let http_port = parse_http_port(&port);

    let pid = runtime.get_pid_str();
    let session = Session::init(runtime, true, vec![], vec![]).await;

    let mut app = Server::with_state((Arc::new(session), pid));
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

    app.at("/").get(query);
    app.at("*").get(query);

    app.at("/").put(write);
    app.at("*").put(write);

    app.at("/").patch(write);
    app.at("*").patch(write);

    app.at("/").delete(write);
    app.at("*").delete(write);

    if let Err(e) = app.listen(http_port).await {
        log::error!("Unable to start http server for REST : {:?}", e);
    }
}

fn path_to_resource<'a>(path: &'a str, pid: &str) -> ResKey<'a> {
    if path == "/@/router/local" {
        ResKey::from(format!("/@/router/{}", pid))
    } else if let Some(suffix) = path.strip_prefix("/@/router/local/") {
        ResKey::from(format!("/@/router/{}/{}", pid, suffix))
    } else {
        ResKey::from(path)
    }
}
