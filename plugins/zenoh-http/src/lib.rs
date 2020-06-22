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

use futures::prelude::*;
use clap::{Arg, ArgMatches};
use zenoh::net::*;
use zenoh_protocol::core::ZInt;
use zenoh_protocol::proto::kind;
use zenoh_router::runtime::Runtime;
use tide::{Request, Response, Server, StatusCode};
use tide::http::Mime;
use std::str::FromStr;

const PORT_SEPARATOR: char = ':';
const DEFAULT_HTTP_HOST: &str = "0.0.0.0";
const DEFAULT_HTTP_PORT: &str = "8000";

fn parse_http_port(arg: &str) -> String {
    match arg.split(':').count() {
        1 => {
            match arg.parse::<u16>() {
                Ok(_) => {[DEFAULT_HTTP_HOST, arg].join(&PORT_SEPARATOR.to_string())} // port only
                Err(_) => {[arg, DEFAULT_HTTP_PORT].join(&PORT_SEPARATOR.to_string())} // host only
            }
        }
        _ => {arg.to_string()}
    }
}

async fn to_json(results: async_std::sync::Receiver<Reply>) -> String{
    let values = results.filter_map(async move |reply| match reply {
        Reply::ReplyData {reskey, payload, ..} => 
            Some(format!("{{ \"key\": \"{}\",\n  \"value\": \"{}\",\n  \"time\": \"{}\" }}",
                        reskey, String::from_utf8_lossy(&payload.to_vec()), "None")), // TODO timestamp
        _ => None,
    }).collect::<Vec<String>>().await.join(",\n");
    format!("[\n{}\n]\n", values)
}

async fn to_html(results: async_std::sync::Receiver<Reply>) -> String{
    let values = results.filter_map(async move |reply| match reply {
        Reply::ReplyData {reskey, payload, ..} => 
            Some(format!("<dt>{}</dt>\n<dd>{}</dd>\n",
                        reskey, String::from_utf8_lossy(&payload.to_vec()))), 
        _ => None,
    }).collect::<Vec<String>>().await.join("\n");
    format!("<dl>\n{}\n</dl>\n", values)
}

fn enc_from_mime(mime: Option<Mime>) -> ZInt {
    match mime {
        Some(mime) => {
            match zenoh_protocol::proto::encoding::from_str(mime.essence()) {
                Ok(encoding) => encoding,
                _ => match mime.basetype() {
                    "text" => zenoh_protocol::proto::encoding::TEXT_PLAIN,
                    &_ => zenoh_protocol::proto::encoding::APP_OCTET_STREAM,
                }
            }
        }
        None => zenoh_protocol::proto::encoding::APP_OCTET_STREAM
    }
}

fn response(status: StatusCode, content_type: Mime, body: &str) -> Response {
    let mut res = Response::new(status);
    res.set_content_type(content_type);
    res.set_body(body);
    res
}

#[no_mangle]
pub fn get_expected_args<'a, 'b>() -> Vec<Arg<'a, 'b>>
{
    vec![
        Arg::from_usage("--http-port 'The listening http port'")
        .default_value(DEFAULT_HTTP_PORT)
    ]
}

#[no_mangle]
pub fn start(runtime: Runtime, args: ArgMatches<'static>)
{
    async_std::task::spawn(run(runtime, args));
}

async fn run(runtime: Runtime, args: ArgMatches<'_>) {
    env_logger::init();

    let http_port = parse_http_port(args.value_of("http-port").unwrap());

    let session = Session::init(runtime).await;

    let mut app = Server::with_state(session);

    app.at("*").get(async move |req: Request<Session>| {
        log::trace!("Http {:?}", req);
        let path = req.url().path();
        let predicate = req.url().query().or(Some("")).unwrap();
        match req.state().query(
                &path.into(), &predicate,
                QueryTarget::default(),
                QueryConsolidation::default()).await {
            Ok(stream) => {
                match req.header("accept") {
                    Some(accept) => {
                        match accept[0].to_string().split(';').next().unwrap().split(',').next().unwrap() {
                            "text/html" => Ok(response(StatusCode::Ok, Mime::from_str("text/html").unwrap(), &to_html(stream).await)),
                            _ => Ok(response(StatusCode::Ok, Mime::from_str("application/json").unwrap(), &to_json(stream).await)),
                        }
                    }
                    None => Ok(response(StatusCode::Ok, Mime::from_str("application/json").unwrap(), &to_json(stream).await))
                }
            },
            Err(e) => 
                Ok(response(StatusCode::InternalServerError, Mime::from_str("text/plain").unwrap(), &e.to_string())),
        }
    });

    app.at("*").put(async move |mut req: Request<Session>| { 
        log::trace!("Http {:?}", req);
        match req.body_bytes().await {
            Ok(bytes) => {
                let path = req.url().path();
                match req.state().write_wo(&path.into(), bytes.into(), 
                        enc_from_mime(req.content_type()), kind::PUT).await {
                    Ok(_) => Ok(Response::new(StatusCode::Ok)),
                    Err(e) => 
                        Ok(response(StatusCode::InternalServerError, Mime::from_str("text/plain").unwrap(), &e.to_string())),
                }
            },
            Err(e) => 
                Ok(response(StatusCode::NoContent, Mime::from_str("text/plain").unwrap(), &e.to_string())),
        }
    });

    app.at("*").patch(async move |mut req: Request<Session>| { 
        log::trace!("Http {:?}", req);
        match req.body_bytes().await {
            Ok(bytes) => {
                let path = req.url().path();
                match req.state().write_wo(&path.into(), bytes.into(), 
                        enc_from_mime(req.content_type()), kind::UPDATE).await {
                    Ok(_) => Ok(Response::new(StatusCode::Ok)),
                    Err(e) => 
                        Ok(response(StatusCode::InternalServerError, Mime::from_str("text/plain").unwrap(), &e.to_string())),
                }
            },
            Err(e) => 
                Ok(response(StatusCode::NoContent, Mime::from_str("text/plain").unwrap(), &e.to_string())),
        }
    });

    app.at("*").delete(async move |req: Request<Session>| { 
        log::trace!("Http {:?}", req);
        let path = req.url().path();
        match req.state().write_wo(&path.into(), RBuf::new(), 
                enc_from_mime(req.content_type()), kind::REMOVE).await {
            Ok(_) => Ok(Response::new(StatusCode::Ok)),
            Err(e) => 
                Ok(response(StatusCode::InternalServerError, Mime::from_str("text/plain").unwrap(), &e.to_string())),
        }
    });

    if let Err(e) = app.listen(http_port).await {
        log::error!("Unable to start http server : {:?}", e);
    }
}

