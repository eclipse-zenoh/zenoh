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
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)
use std::{
    convert::{Infallible, TryFrom},
    fmt::Write,
    future::Future,
    net::SocketAddr,
    pin::Pin,
    str::FromStr,
    sync::atomic::{AtomicUsize, Ordering},
    task::{Context, Poll},
    time::Duration,
};

use axum::{
    extract::{FromRequest, FromRequestParts, Path, Request, State},
    http::{header, request::Parts, HeaderValue, Method, StatusCode, Uri},
    response::{sse::Event, Html, IntoResponse, Response, Sse},
    routing::get,
    Json, Router,
};
use base64::Engine;
use futures::{FutureExt, Stream, StreamExt, TryFutureExt};
use mime::Mime;
use serde::Serialize;
use tokio::{net::TcpListener, task::JoinHandle, time::timeout};
use tower_http::{
    cors::{Any, CorsLayer},
    set_header::SetResponseHeaderLayer,
    trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer},
};
use tracing::Level;
use zenoh::{
    bytes::{Encoding, ZBytes},
    internal::{
        bail,
        plugins::{RunningPluginTrait, ZenohPlugin},
        runtime::DynamicRuntime,
        zerror,
    },
    key_expr::{keyexpr, KeyExpr},
    query::{Parameters, QueryConsolidation, Selector, ZenohParameters},
    sample::Sample,
    session::Session,
    Result as ZResult,
};
use zenoh_plugin_trait::{plugin_long_version, plugin_version, Plugin, PluginControl};

mod config;
pub use config::Config;
use zenoh::{
    handlers::{fifo::RecvStream, FifoChannelHandler},
    pubsub::Subscriber,
    time::Timestamp,
};

const GIT_VERSION: &str = git_version::git_version!(prefix = "v", cargo_prefix = "v");
lazy_static::lazy_static! {
    static ref LONG_VERSION: String = format!("{} built with {}", GIT_VERSION, env!("RUSTC_VERSION"));
}
const RAW_KEY: &str = "_raw";

lazy_static::lazy_static! {
    static ref WORKER_THREAD_NUM: AtomicUsize = AtomicUsize::new(config::DEFAULT_WORK_THREAD_NUM);
    static ref MAX_BLOCK_THREAD_NUM: AtomicUsize = AtomicUsize::new(config::DEFAULT_MAX_BLOCK_THREAD_NUM);
    // The global runtime is used in the dynamic plugins, which we can't get the current runtime
    static ref TOKIO_RUNTIME: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
               .worker_threads(WORKER_THREAD_NUM.load(Ordering::SeqCst))
               .max_blocking_threads(MAX_BLOCK_THREAD_NUM.load(Ordering::SeqCst))
               .enable_all()
               .build()
               .expect("Unable to create runtime");
}

#[inline(always)]
pub(crate) fn blockon_runtime<F: Future>(task: F) -> F::Output {
    // Check whether able to get the current runtime
    match tokio::runtime::Handle::try_current() {
        Ok(rt) => {
            // Able to get the current runtime (standalone binary), use the current runtime
            tokio::task::block_in_place(|| rt.block_on(task))
        }
        Err(_) => {
            // Unable to get the current runtime (dynamic plugins), reuse the global runtime
            tokio::task::block_in_place(|| TOKIO_RUNTIME.block_on(task))
        }
    }
}

pub(crate) fn spawn_runtime<F>(task: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    // Check whether able to get the current runtime
    match tokio::runtime::Handle::try_current() {
        Ok(rt) => {
            // Able to get the current runtime (standalone binary), spawn on the current runtime
            rt.spawn(task)
        }
        Err(_) => {
            // Unable to get the current runtime (dynamic plugins), spawn on the global runtime
            TOKIO_RUNTIME.spawn(task)
        }
    }
}

#[derive(Serialize)]
struct JSONSample {
    key: String,
    value: serde_json::Value,
    encoding: String,
    timestamp: Option<String>,
}

impl JSONSample {
    fn new(
        key: impl Into<String>,
        payload: &ZBytes,
        encoding: &Encoding,
        timestamp: Option<&Timestamp>,
    ) -> Self {
        JSONSample {
            key: key.into(),
            value: Self::payload_to_json(payload, encoding),
            encoding: encoding.to_string(),
            timestamp: timestamp.map(|ts| ts.to_string()),
        }
    }

    fn payload_to_json(payload: &ZBytes, encoding: &Encoding) -> serde_json::Value {
        if payload.is_empty() {
            return serde_json::Value::Null;
        }
        let base64_encode = |data: &[u8]| base64::engine::general_purpose::STANDARD.encode(data);
        match encoding {
            // If it is a JSON try to deserialize as json, if it fails fallback to base64
            &Encoding::APPLICATION_JSON | &Encoding::TEXT_JSON | &Encoding::TEXT_JSON5 => {
                let bytes = payload.to_bytes();
                serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                    tracing::warn!(
                        "Encoding is JSON but data is not JSON, converting to base64, Error: {e:?}"
                    );
                    serde_json::Value::String(base64_encode(&bytes))
                })
            }
            &Encoding::TEXT_PLAIN | &Encoding::ZENOH_STRING => serde_json::Value::String(
                String::from_utf8(payload.to_bytes().into_owned()).unwrap_or_else(|e| {
                    tracing::warn!(
                    "Encoding is String but data is not String, converting to base64, Error: {e:?}"
                );
                    base64_encode(e.as_bytes())
                }),
            ),
            // otherwise convert to JSON string
            _ => serde_json::Value::String(base64_encode(&payload.to_bytes())),
        }
    }
}

impl From<&Sample> for JSONSample {
    fn from(value: &Sample) -> Self {
        Self::new(
            value.key_expr().as_str(),
            value.payload(),
            value.encoding(),
            value.timestamp(),
        )
    }
}

#[cfg(feature = "dynamic_plugin")]
zenoh_plugin_trait::declare_plugin!(RestPlugin);

pub struct RestPlugin {}

impl ZenohPlugin for RestPlugin {}

impl Plugin for RestPlugin {
    type StartArgs = DynamicRuntime;
    type Instance = zenoh::internal::plugins::RunningPlugin;
    const DEFAULT_NAME: &'static str = "rest";
    const PLUGIN_VERSION: &'static str = plugin_version!();
    const PLUGIN_LONG_VERSION: &'static str = plugin_long_version!();

    fn start(
        name: &str,
        runtime: &Self::StartArgs,
    ) -> ZResult<zenoh::internal::plugins::RunningPlugin> {
        // Try to initiate login.
        // Required in case of dynamic lib, otherwise no logs.
        // But cannot be done twice in case of static link.
        zenoh::init_log_from_env_or("error");
        tracing::debug!("REST plugin {}", LONG_VERSION.as_str());

        let plugin_conf = runtime
            .get_config()
            .get_plugin_config(name)
            .map_err(|_| zerror!("Plugin `{}`: missing config", name))?;

        let conf: Config = serde_json::from_value(plugin_conf)
            .map_err(|e| zerror!("Plugin `{}` configuration error: {}", name, e))?;
        WORKER_THREAD_NUM.store(conf.work_thread_num, Ordering::SeqCst);
        MAX_BLOCK_THREAD_NUM.store(conf.max_block_thread_num, Ordering::SeqCst);

        let task = run(runtime.clone(), conf.addr);
        let task =
            blockon_runtime(async { timeout(Duration::from_millis(1), spawn_runtime(task)).await });

        // The spawn task (spawn_runtime(task)).await) should not return immediately. The server should block inside.
        // If it returns immediately (for example, address already in use), we can get the error inside Ok
        if let Ok(Ok(Err(e))) = task {
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
        key_expr: &'a KeyExpr<'a>,
        plugin_status_key: &str,
    ) -> ZResult<Vec<zenoh::internal::plugins::Response>> {
        let mut responses = Vec::new();
        let mut key = String::from(plugin_status_key);
        with_extended_string(&mut key, &["/version"], |key| {
            if keyexpr::new(key.as_str()).unwrap().intersects(key_expr) {
                responses.push(zenoh::internal::plugins::Response::new(
                    key.clone(),
                    GIT_VERSION.into(),
                ))
            }
        });
        with_extended_string(&mut key, &["/port"], |port_key| {
            if keyexpr::new(port_key.as_str())
                .unwrap()
                .intersects(key_expr)
            {
                responses.push(zenoh::internal::plugins::Response::new(
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

fn app(session: Session) -> Router {
    Router::new()
        .route(
            "/{*key_expr}",
            get(subscribe_or_query)
                .post(subscribe_or_query)
                .put(publish)
                .patch(publish)
                .delete(publish),
        )
        .with_state(AppState { session })
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().include_headers(true))
                .on_request(DefaultOnRequest::new().level(Level::TRACE))
                .on_response(
                    DefaultOnResponse::new()
                        .level(Level::TRACE)
                        .include_headers(true),
                ),
        )
        .layer(
            CorsLayer::new()
                .allow_methods([
                    Method::GET,
                    Method::POST,
                    Method::PUT,
                    Method::PATCH,
                    Method::DELETE,
                ])
                .allow_origin(Any)
                .allow_credentials(false),
        )
        .layer(SetResponseHeaderLayer::overriding(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        ))
}

#[derive(Clone)]
struct AppState {
    session: Session,
}

async fn subscribe(state: AppState, key_expr: KeyExpr<'static>) -> Response {
    let subscriber = match state.session.declare_subscriber(&key_expr).await {
        Ok(sub) => sub,
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    };
    struct SubscriberStream {
        stream: RecvStream<'static, Sample>,
        _subscriber: Subscriber<FifoChannelHandler<Sample>>,
    }
    impl Stream for SubscriberStream {
        type Item = Sample;

        fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            self.stream.poll_next_unpin(cx)
        }
    }
    let stream = SubscriberStream {
        stream: subscriber.handler().clone().into_stream(),
        _subscriber: subscriber,
    };
    Sse::new(stream.map(|sample| Event::default().json_data(JSONSample::from(&sample))))
        .into_response()
}

async fn query(
    state: AppState,
    accept: Accept,
    key_expr: KeyExpr<'static>,
    encoding: Encoding,
    uri: Uri,
    body: ZBytes,
) -> Response {
    let parameters = Parameters::from(uri.query().unwrap_or_default());
    let consolidation = if parameters.time_range().is_some() {
        QueryConsolidation::from(zenoh::query::ConsolidationMode::None)
    } else {
        QueryConsolidation::from(zenoh::query::ConsolidationMode::Latest)
    };
    let mut query = state
        .session
        .get(Selector::borrowed(&key_expr, &parameters))
        .consolidation(consolidation)
        .with(flume::unbounded());
    if !body.is_empty() {
        query = query.payload(body).encoding(encoding);
    }
    match query.await {
        Ok(receiver) => match accept {
            Accept::Raw => match receiver.recv_async().await {
                Ok(reply) => {
                    let (encoding, payload) = match reply.result() {
                        Ok(sample) => (sample.encoding(), sample.payload()),
                        Err(err) => (err.encoding(), err.payload()),
                    };
                    let content_type = Some([(header::CONTENT_TYPE, encoding.to_string())])
                        .filter(|[(_, e)]| Mime::from_str(e).is_ok());
                    (content_type, payload.to_bytes().to_vec()).into_response()
                }
                Err(_) => StatusCode::OK.into_response(),
            },
            Accept::Html => {
                let mut html = "<dl>\n".to_string();
                while let Ok(reply) = receiver.recv_async().await {
                    let (key, payload) = match reply.result() {
                        Ok(sample) => (sample.key_expr().as_str(), sample.payload().to_bytes()),
                        Err(err) => ("ERROR", err.payload().to_bytes()),
                    };
                    let payload = String::from_utf8_lossy(&payload);
                    write!(&mut html, "<dt>{key}</dt>\n<dd>{payload}</dd>\n").unwrap();
                }
                Html(html + "</dl>\n").into_response()
            }
            Accept::Json => receiver
                .stream()
                .map(|reply| match reply.into_result() {
                    Ok(sample) => JSONSample::from(&sample),
                    Err(err) => JSONSample::new("ERROR", err.payload(), err.encoding(), None),
                })
                .collect::<Vec<JSONSample>>()
                .map(Json)
                .await
                .into_response(),
            _ => unreachable!(),
        },
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn subscribe_or_query(
    State(state): State<AppState>,
    accept: Accept,
    KeyExprPath(key_expr): KeyExprPath,
    EncodingHeader(encoding): EncodingHeader,
    uri: Uri,
    ZBytesBody(body): ZBytesBody,
) -> Response {
    match accept {
        Accept::EventStream => subscribe(state, key_expr).await,
        accept => query(state, accept, key_expr, encoding, uri, body).await,
    }
}

async fn publish(
    State(state): State<AppState>,
    method: Method,
    KeyExprPath(key_expr): KeyExprPath,
    EncodingHeader(encoding): EncodingHeader,
    ZBytesBody(bytes): ZBytesBody,
) -> Response {
    // @TODO: Define the right congestion control value
    let res = if method == Method::DELETE {
        state.session.delete(key_expr).await
    } else {
        state.session.put(key_expr, bytes).encoding(encoding).await
    };
    match res {
        Ok(_) => StatusCode::OK.into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

pub async fn run(runtime: DynamicRuntime, addr: SocketAddr) -> ZResult<()> {
    // Try to initiate login.
    // Required in case of dynamic lib, otherwise no logs.
    // But cannot be done twice in case of static link.
    zenoh::init_log_from_env_or("error");

    match tokio::try_join!(
        zenoh::session::init(runtime),
        TcpListener::bind(addr).map_err(Into::into)
    ) {
        Ok((session, listener)) => axum::serve(listener, app(session)).await?,
        Err(err) => {
            tracing::error!("Unable to start http server for REST: {:?}", err);
            return Err(err);
        }
    }
    Ok(())
}

struct KeyExprPath(KeyExpr<'static>);

impl FromRequestParts<AppState> for KeyExprPath {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Path(mut key_expr): Path<String> = Path::from_request_parts(parts, state)
            .await
            .map_err(IntoResponse::into_response)?;
        if let Some(suffix) = key_expr.strip_prefix("@/local") {
            if suffix.is_empty() || suffix.starts_with('/') {
                key_expr = format!("@/{zid}{suffix}", zid = state.session.zid());
            }
        }
        match KeyExpr::try_from(key_expr) {
            Ok(key_expr) => Ok(KeyExprPath(key_expr)),
            Err(err) => Err((StatusCode::BAD_REQUEST, err.to_string()).into_response()),
        }
    }
}

struct EncodingHeader(Encoding);

impl FromRequestParts<AppState> for EncodingHeader {
    type Rejection = Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let encoding = match parts.headers.get(header::CONTENT_TYPE) {
            Some(h) => Encoding::from(String::from_utf8_lossy(h.as_bytes()).as_ref()),
            None => Encoding::default(),
        };
        Ok(EncodingHeader(encoding))
    }
}

enum Accept {
    EventStream,
    Raw,
    Html,
    Json,
}

impl FromRequestParts<AppState> for Accept {
    type Rejection = Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let accept = match parts
            .headers
            .get(header::ACCEPT)
            .and_then(|h| h.to_str().ok()?.split(',').next()?.split(';').next())
        {
            Some("text/event-stream") => Accept::EventStream,
            _ if Parameters::from(parts.uri.query().unwrap_or_default()).contains_key(RAW_KEY) => {
                Accept::Raw
            }
            Some("text/html") => Accept::Html,
            _ => Accept::Json,
        };
        Ok(accept)
    }
}

struct ZBytesBody(ZBytes);

impl FromRequest<AppState> for ZBytesBody {
    type Rejection = (StatusCode, String);

    async fn from_request(req: Request, _state: &AppState) -> Result<Self, Self::Rejection> {
        let mut stream = req.into_body().into_data_stream();
        let mut writer = ZBytes::writer();
        while let Some(bytes) = stream.next().await {
            match bytes {
                Ok(bytes) => writer.append(bytes.into()),
                Err(err) => return Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string())),
            }
        }
        Ok(ZBytesBody(writer.finish()))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::{
        body::{Body, Bytes},
        http::{header, Method, Request},
    };
    use futures::{FutureExt, StreamExt};
    use tokio::time::timeout;
    use tower::ServiceExt;
    use zenoh::{bytes::Encoding, sample::SampleKind, Config, Session, Wait};

    use crate::app;

    const TEST_PORTS: [u16; 3] = [42000, 42001, 42002];

    async fn setup(port: u16) -> (Session, Session) {
        let mut config1 = Config::default();
        config1.scouting.multicast.set_enabled(Some(false)).unwrap();
        config1
            .listen
            .endpoints
            .set(vec![format!("tcp/127.0.0.1:{}", port).parse().unwrap()])
            .unwrap();

        let mut config2 = Config::default();
        config2.scouting.multicast.set_enabled(Some(false)).unwrap();
        config2
            .connect
            .endpoints
            .set(vec![format!("tcp/127.0.0.1:{}", port).parse().unwrap()])
            .unwrap();
        let (s1, s2) = tokio::try_join!(zenoh::open(config1), zenoh::open(config2)).unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        (s1, s2)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn publish() {
        let (pub_session, sub_session) = setup(TEST_PORTS[0]).await;
        let subscriber = sub_session.declare_subscriber("test/**").await.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        for method in [Method::PUT, Method::PATCH] {
            let response = app(pub_session.clone())
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri("/test/publish")
                        .header(header::CONTENT_TYPE, "text/plain")
                        .body(Body::from("payload"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert!(response.status().is_success());
            tokio::time::sleep(Duration::from_secs(1)).await;
            let sample = subscriber.try_recv().unwrap().unwrap();
            assert_eq!(sample.kind(), SampleKind::Put);
            assert_eq!(sample.key_expr().as_str(), "test/publish");
            assert_eq!(sample.payload().try_to_string().unwrap(), "payload");
            assert_eq!(sample.encoding(), &Encoding::TEXT_PLAIN);
        }
        let response = app(pub_session.clone())
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri("/test/publish")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(response.status().is_success());
        tokio::time::sleep(Duration::from_secs(1)).await;
        let sample = subscriber.try_recv().unwrap().unwrap();
        assert_eq!(sample.kind(), SampleKind::Delete);
        assert_eq!(sample.key_expr().as_str(), "test/publish");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn subscribe() {
        let (pub_session, sub_session) = setup(TEST_PORTS[1]).await;
        let response = app(sub_session.clone())
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/test/subscribe")
                    .header(header::ACCEPT, "text/event-stream")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(response.status().is_success());
        let mut stream = response.into_body().into_data_stream();
        tokio::time::sleep(Duration::from_secs(1)).await;
        for i in 0..2 {
            pub_session
                .put("test/subscribe", format!("payload {i}"))
                .encoding(Encoding::TEXT_PLAIN)
                .await
                .unwrap();
            let sample = timeout(Duration::from_secs(1), stream.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(
                    std::str::from_utf8(&sample)
                        .unwrap()
                        .strip_prefix("data: ")
                        .unwrap()
                )
                .unwrap(),
                serde_json::json!({
                    "key": "test/subscribe",
                    "value": format!("payload {i}"),
                    "encoding": "text/plain",
                    "timestamp": null,
                }),
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn query() {
        let (get_session, queryable_session) = setup(TEST_PORTS[2]).await;
        let _queryable = queryable_session
            .declare_queryable("test/**")
            .callback(|q| {
                q.reply(q.key_expr(), "reply")
                    .encoding(Encoding::TEXT_PLAIN)
                    .wait()
                    .unwrap()
            })
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let check_json: fn(Bytes) = |body| {
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
                serde_json::json!([{
                    "key": "test/query",
                    "value": "reply",
                    "encoding": "text/plain",
                    "timestamp": null,
                }]),
            )
        };
        let check_html: fn(Bytes) =
            |body: Bytes| assert_eq!(body, "<dl>\n<dt>test/query</dt>\n<dd>reply</dd>\n</dl>\n");
        let check_raw: fn(Bytes) = |body| assert_eq!(body, "reply");
        for (query, accept, check) in [
            ("", "application/json", check_json),
            ("", "text/html; charset=utf-8", check_html),
            ("?_raw=true", "text/plain", check_raw),
        ] {
            let response = app(get_session.clone())
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(format!("/test/query{query}"))
                        .header(header::ACCEPT, accept)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert!(response.status().is_success());
            assert_eq!(
                response.headers().get(header::CONTENT_TYPE).unwrap(),
                accept
            );
            let body = response
                .into_body()
                .into_data_stream()
                .next()
                .now_or_never()
                .unwrap()
                .unwrap()
                .unwrap();
            check(body);
        }
    }
}
