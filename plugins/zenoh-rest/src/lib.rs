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
#![feature(async_closure)]

use async_std::channel::Receiver;
use async_std::sync::Arc;
use clap::{Arg, ArgMatches};
use futures::prelude::*;
use std::convert::TryFrom;
use std::str::FromStr;
use tide::http::Mime;
use tide::sse::Sender;
use tide::{Request, Response, Server, StatusCode};
use zenoh::net::*;
use zenoh::{Change, Selector, Value};
use zenoh_router::runtime::Runtime;

const PORT_SEPARATOR: char = ':';
const DEFAULT_HTTP_HOST: &str = "0.0.0.0";
const DEFAULT_HTTP_PORT: &str = "8000";

const SSE_SUB_INFO: SubInfo = SubInfo {
    reliability: Reliability::Reliable,
    mode: SubMode::Push,
    period: None,
};

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

fn get_kind_str(sample: &Sample) -> String {
    let kind = match &sample.data_info {
        Some(info) => info.kind.unwrap_or(data_kind::DEFAULT),
        None => data_kind::DEFAULT,
    };
    data_kind::to_string(kind)
}

fn value_to_json(value: Value) -> String {
    // TODO: transcode to JSON when implemented in Value
    use Value::*;

    match value {
        Raw(_, _)
        | Custom {
            encoding_descr: _,
            data: _,
        } => {
            // encode value as a String, possibly encoding as base64
            let (_, _, s) = value.encode_to_string();
            format!(r#""{}""#, s)
        }
        StringUtf8(s) => {
            // convert to Json string for special characters escaping
            let js = serde_json::json!(s);
            js.to_string()
        }
        Properties(p) => {
            // convert to Json string for special characters escaping
            let js = serde_json::json!(*p);
            js.to_string()
        }
        Json(s) => s,
        Integer(i) => format!(r#"{}"#, i),
        Float(f) => format!(r#"{}"#, f),
    }
}

fn sample_to_json(sample: Sample) -> String {
    let res_name = sample.res_name.clone();
    if let Ok(change) = Change::from_sample(sample, true) {
        let (encoding, value) = match change.value {
            Some(v) => (v.encoding_descr(), value_to_json(v)),
            None => ("None".to_string(), r#""""#.to_string()),
        };
        format!(
            r#"{{ "key": "{}", "value": {}, "encoding": "{}", "time": "{}" }}"#,
            change.path, value, encoding, change.timestamp
        )
    } else {
        format!(
            r#"{{ "key": "{}", "value": {}, "encoding": "{}", "time": "{}" }}"#,
            res_name, "ERROR: Failed to decode Sample", "Unkown", "None"
        )
    }
}

async fn to_json(results: Receiver<Reply>) -> String {
    let values = results
        .filter_map(async move |reply| Some(sample_to_json(reply.data)))
        .collect::<Vec<String>>()
        .await
        .join(",\n");
    format!("[\n{}\n]\n", values)
}

fn sample_to_html(sample: Sample) -> String {
    format!(
        "<dt>{}</dt>\n<dd>{}</dd>\n",
        sample.res_name,
        String::from_utf8_lossy(&sample.payload.to_vec())
    )
}

async fn to_html(results: Receiver<Reply>) -> String {
    let values = results
        .filter_map(async move |reply| Some(sample_to_html(reply.data)))
        .collect::<Vec<String>>()
        .await
        .join("\n");
    format!("<dl>\n{}\n</dl>\n", values)
}

fn enc_from_mime(mime: Option<Mime>) -> ZInt {
    use zenoh::net::encoding::*;
    match mime {
        Some(mime) => match from_str(mime.essence()) {
            Ok(encoding) => encoding,
            _ => match mime.basetype() {
                "text" => TEXT_PLAIN,
                &_ => APP_OCTET_STREAM,
            },
        },
        None => APP_OCTET_STREAM,
    }
}

fn response(status: StatusCode, content_type: Mime, body: &str) -> Response {
    let mut res = Response::new(status);
    res.set_content_type(content_type);
    res.set_body(body);
    res
}

#[no_mangle]
pub fn get_expected_args<'a, 'b>() -> Vec<Arg<'a, 'b>> {
    vec![
        Arg::from_usage("--rest-http-port 'The REST plugin's http port'")
            .default_value(DEFAULT_HTTP_PORT),
    ]
}

#[no_mangle]
pub fn start(runtime: Runtime, args: &'static ArgMatches<'_>) {
    async_std::task::spawn(run(runtime, args));
}

async fn run(runtime: Runtime, args: &'static ArgMatches<'_>) {
    env_logger::init();

    let http_port = parse_http_port(args.value_of("rest-http-port").unwrap());

    let pid = runtime.get_pid_str().await;
    let session = Session::init(runtime, true, vec![], vec![]).await;

    let mut app = Server::with_state((Arc::new(session), pid));

    app.at("*")
        .get(async move |req: Request<(Arc<Session>, String)>| {
            log::trace!("REST: {:?}", req);
            // Reconstruct Selector from req.url() (no easier way...)
            let url = req.url();
            let mut s = String::with_capacity(url.as_str().len());
            s.push_str(url.path());
            if let Some(q) = url.query() {
                s.push('?');
                s.push_str(q);
            }
            let selector = match Selector::try_from(s) {
                Ok(sel) => sel,
                Err(e) => {
                    return Ok(response(
                        StatusCode::BadRequest,
                        Mime::from_str("text/plain").unwrap(),
                        &e.to_string(),
                    ))
                }
            };

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
            match &first_accept[..] {
                "text/event-stream" => Ok(tide::sse::upgrade(
                    req,
                    async move |req: Request<(Arc<Session>, String)>, sender: Sender| {
                        let resource = path_to_resource(req.url().path(), &req.state().1);
                        async_std::task::spawn(async move {
                            log::debug!(
                                "Subscribe to {} for SSE stream (task {})",
                                resource,
                                async_std::task::current().id()
                            );
                            let sender = &sender;
                            let mut sub = req
                                .state()
                                .0
                                .declare_subscriber(&resource, &SSE_SUB_INFO)
                                .await
                                .unwrap();
                            loop {
                                let sample = sub.stream().next().await.unwrap();
                                let send = async {
                                    if let Err(e) = sender
                                        .send(&get_kind_str(&sample), sample_to_json(sample), None)
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
                                    if let Err(e) = sub.undeclare().await {
                                        log::error!("Error undeclaring subscriber: {}", e);
                                    }
                                    break;
                                }
                            }
                        });
                        Ok(())
                    },
                )),

                "text/html" => {
                    let resource = path_to_resource(selector.path_expr.as_str(), &req.state().1);
                    let consolidation = if selector.has_time_range() {
                        QueryConsolidation::none()
                    } else {
                        QueryConsolidation::default()
                    };
                    match req
                        .state()
                        .0
                        .query(
                            &resource,
                            &selector.predicate,
                            QueryTarget::default(),
                            consolidation,
                        )
                        .await
                    {
                        Ok(stream) => Ok(response(
                            StatusCode::Ok,
                            Mime::from_str("text/html").unwrap(),
                            &to_html(stream).await,
                        )),
                        Err(e) => Ok(response(
                            StatusCode::InternalServerError,
                            Mime::from_str("text/plain").unwrap(),
                            &e.to_string(),
                        )),
                    }
                }

                _ => {
                    let resource = path_to_resource(selector.path_expr.as_str(), &req.state().1);
                    let consolidation = if selector.has_time_range() {
                        QueryConsolidation::none()
                    } else {
                        QueryConsolidation::default()
                    };
                    match req
                        .state()
                        .0
                        .query(
                            &resource,
                            &selector.predicate,
                            QueryTarget::default(),
                            consolidation,
                        )
                        .await
                    {
                        Ok(stream) => Ok(response(
                            StatusCode::Ok,
                            Mime::from_str("application/json").unwrap(),
                            &to_json(stream).await,
                        )),
                        Err(e) => Ok(response(
                            StatusCode::InternalServerError,
                            Mime::from_str("text/plain").unwrap(),
                            &e.to_string(),
                        )),
                    }
                }
            }
        });

    app.at("*")
        .put(async move |mut req: Request<(Arc<Session>, String)>| {
            log::trace!("REST: {:?}", req);
            match req.body_bytes().await {
                Ok(bytes) => {
                    let resource = path_to_resource(req.url().path(), &req.state().1);
                    match req
                        .state()
                        .0
                        .write_ext(
                            &resource,
                            bytes.into(),
                            enc_from_mime(req.content_type()),
                            data_kind::PUT,
                            CongestionControl::Drop, // TODO: Define the right congestion control value for the put
                        )
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
        });

    app.at("*")
        .patch(async move |mut req: Request<(Arc<Session>, String)>| {
            log::trace!("REST: {:?}", req);
            match req.body_bytes().await {
                Ok(bytes) => {
                    let resource = path_to_resource(req.url().path(), &req.state().1);
                    match req
                        .state()
                        .0
                        .write_ext(
                            &resource,
                            bytes.into(),
                            enc_from_mime(req.content_type()),
                            data_kind::PATCH,
                            CongestionControl::Drop, // TODO: Define the right congestion control value for the delete
                        )
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
        });

    app.at("*")
        .delete(async move |req: Request<(Arc<Session>, String)>| {
            log::trace!("REST: {:?}", req);
            let resource = path_to_resource(req.url().path(), &req.state().1);
            match req
                .state()
                .0
                .write_ext(
                    &resource,
                    RBuf::new(),
                    enc_from_mime(req.content_type()),
                    data_kind::DELETE,
                    CongestionControl::Drop, // TODO: Define the right congestion control value for the delete
                )
                .await
            {
                Ok(_) => Ok(Response::new(StatusCode::Ok)),
                Err(e) => Ok(response(
                    StatusCode::InternalServerError,
                    Mime::from_str("text/plain").unwrap(),
                    &e.to_string(),
                )),
            }
        });

    if let Err(e) = app.listen(http_port).await {
        log::error!("Unable to start http server for REST : {:?}", e);
    }
}

fn path_to_resource(path: &str, pid: &str) -> ResKey {
    if path == "/@/router/local" {
        ResKey::from(format!("/@/router/{}", pid))
    } else if let Some(suffix) = path.strip_prefix("/@/router/local/") {
        ResKey::from(format!("/@/router/{}/{}", pid, suffix))
    } else {
        ResKey::from(path)
    }
}
