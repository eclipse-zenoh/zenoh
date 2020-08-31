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

use clap::{Arg, ArgMatches};
use futures::prelude::*;
use std::str::FromStr;
use tide::http::Mime;
use tide::{Request, Response, Server, StatusCode};
use zenoh::net::*;
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
    match data_kind::to_str(kind) {
        Ok(string) => string,
        _ => "PUT".to_string(),
    }
}

fn sample_to_json(sample: Sample) -> String {
    format!(
        "{{ \"key\": \"{}\", \"value\": \"{}\", \"time\": \"{}\" }}",
        sample.res_name,
        String::from_utf8_lossy(&sample.payload.to_vec()),
        sample
            .data_info
            .and_then(|i| i.timestamp)
            .map(|ts| ts.to_string())
            .unwrap_or_else(|| "None".to_string())
    )
}

async fn to_json(results: async_std::sync::Receiver<Reply>) -> String {
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

async fn to_html(results: async_std::sync::Receiver<Reply>) -> String {
    let values = results
        .filter_map(async move |reply| Some(sample_to_html(reply.data)))
        .collect::<Vec<String>>()
        .await
        .join("\n");
    format!("<dl>\n{}\n</dl>\n", values)
}

fn enc_from_mime(mime: Option<Mime>) -> ZInt {
    match mime {
        Some(mime) => match zenoh_protocol::proto::encoding::from_str(mime.essence()) {
            Ok(encoding) => encoding,
            _ => match mime.basetype() {
                "text" => zenoh_protocol::proto::encoding::TEXT_PLAIN,
                &_ => zenoh_protocol::proto::encoding::APP_OCTET_STREAM,
            },
        },
        None => zenoh_protocol::proto::encoding::APP_OCTET_STREAM,
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
    vec![Arg::from_usage("--http-port 'The listening http port'").default_value(DEFAULT_HTTP_PORT)]
}

#[no_mangle]
pub fn start(runtime: Runtime, args: &'static ArgMatches<'_>) {
    async_std::task::spawn(run(runtime, args));
}

async fn run(runtime: Runtime, args: &'static ArgMatches<'_>) {
    env_logger::init();

    let http_port = parse_http_port(args.value_of("http-port").unwrap());

    let pid = runtime.get_pid_str().await;
    let session = Session::init(runtime).await;

    let mut app = Server::with_state((session, pid));

    app.at("*")
        .get(async move |req: Request<(Session, String)>| {
            log::trace!("Http {:?}", req);

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
                    async move |req: Request<(Session, String)>, sender| {
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
                                    sender
                                        .send(&get_kind_str(&sample), sample_to_json(sample), None)
                                        .await;
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
                    let resource = path_to_resource(req.url().path(), &req.state().1);
                    let predicate = req.url().query().or(Some("")).unwrap();
                    match req
                        .state()
                        .0
                        .query(
                            &resource,
                            &predicate,
                            QueryTarget::default(),
                            QueryConsolidation::default(),
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
                    let resource = path_to_resource(req.url().path(), &req.state().1);
                    let predicate = req.url().query().or(Some("")).unwrap();
                    match req
                        .state()
                        .0
                        .query(
                            &resource,
                            &predicate,
                            QueryTarget::default(),
                            QueryConsolidation::default(),
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
        .put(async move |mut req: Request<(Session, String)>| {
            log::trace!("Http {:?}", req);
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
        .patch(async move |mut req: Request<(Session, String)>| {
            log::trace!("Http {:?}", req);
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
        .delete(async move |req: Request<(Session, String)>| {
            log::trace!("Http {:?}", req);
            let resource = path_to_resource(req.url().path(), &req.state().1);
            match req
                .state()
                .0
                .write_ext(
                    &resource,
                    RBuf::new(),
                    enc_from_mime(req.content_type()),
                    data_kind::DELETE,
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
        log::error!("Unable to start http server : {:?}", e);
    }
}

fn path_to_resource(path: &str, pid: &str) -> ResKey {
    if path == "/@/router/local" {
        ResKey::from(format!("/@/router/{}", pid))
    } else if path.starts_with("/@/router/local/") {
        ResKey::from(format!("/@/router/{}/{}", pid, &path[16..]))
    } else {
        ResKey::from(path)
    }
}
