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

use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufReader, ErrorKind},
    net::SocketAddr,
    path::Path,
    sync::Arc,
};

use flume::Sender;
use futures::{future, pin_mut, StreamExt, TryStreamExt};
use interface::RemoteAPIMsg;
use rustls_pemfile::{certs, private_key};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::RwLock,
    task::JoinHandle,
};
use tokio_rustls::{
    rustls::{
        self,
        pki_types::{CertificateDer, PrivateKeyDer},
    },
    server::TlsStream,
    TlsAcceptor,
};
use tokio_tungstenite::tungstenite::protocol::Message;
use tracing::{debug, error};
use uuid::Uuid;
use zenoh::{
    internal::{
        plugins::{RunningPluginTrait, ZenohPlugin},
        runtime::Runtime,
    },
    pubsub::{Publisher, Subscriber},
    query::{Query, Queryable},
    Session,
};
use zenoh_plugin_trait::{plugin_long_version, plugin_version, Plugin, PluginControl};
use zenoh_result::{bail, zerror, ZResult};

mod config;
pub use config::Config;

mod handle_control_message;
mod handle_data_message;
mod interface;
use crate::{
    handle_control_message::handle_control_message, handle_data_message::handle_data_message,
};

const WORKER_THREAD_NUM: usize = 2;
const MAX_BLOCK_THREAD_NUM: usize = 50;
const GIT_VERSION: &str = git_version::git_version!(prefix = "v", cargo_prefix = "v");

lazy_static::lazy_static! {
    static ref LONG_VERSION: String = format!("{} built with {}", GIT_VERSION, env!("RUSTC_VERSION"));
}

fn load_certs(path: &Path) -> io::Result<Vec<CertificateDer<'static>>> {
    certs(&mut BufReader::new(File::open(path)?)).collect()
}

fn load_key(path: &Path) -> io::Result<PrivateKeyDer<'static>> {
    private_key(&mut BufReader::new(File::open(path)?))
        .unwrap()
        .ok_or(io::Error::new(
            ErrorKind::Other,
            "No private key found".to_string(),
        ))
}

pub struct RemoteApiPlugin {}

#[cfg(feature = "dynamic_plugin")]
zenoh_plugin_trait::declare_plugin!(RemoteApiPlugin);

impl ZenohPlugin for RemoteApiPlugin {}

impl Plugin for RemoteApiPlugin {
    type StartArgs = Runtime;
    type Instance = zenoh::internal::plugins::RunningPlugin;
    const DEFAULT_NAME: &'static str = "remote_api";
    const PLUGIN_VERSION: &'static str = plugin_version!();
    const PLUGIN_LONG_VERSION: &'static str = plugin_long_version!();

    fn start(
        name: &str,
        runtime: &Self::StartArgs,
    ) -> ZResult<zenoh::internal::plugins::RunningPlugin> {
        // Try to initiate login.
        // Required in case of dynamic lib, otherwise no logs.
        // But cannot be done twice in case of static link.
        zenoh_util::try_init_log_from_env();
        tracing::info!("Starting {name}");

        let runtime_conf = runtime.config().lock();

        let plugin_conf = runtime_conf
            .plugin(name)
            .ok_or_else(|| zerror!("Plugin `{}`: missing config", name))?;

        let conf: Config = serde_json::from_value(plugin_conf.clone())
            .map_err(|e| zerror!("Plugin `{}` configuration error: {}", name, e))?;

        let wss_config = match conf.secure_websocket {
            Some(wss_config) => {
                tracing::info!("Loading certs from : {} ...", wss_config.certificate_path);
                let certs = load_certs(Path::new(&wss_config.certificate_path))
                    .map_err(|err| zerror!("Could not Load Cert `{}`", err))?;
                tracing::info!(
                    "Loading Private Key from : {} ...",
                    wss_config.private_key_path
                );
                let key = load_key(Path::new(&wss_config.private_key_path))
                    .map_err(|err| zerror!("Could not Load Private Key `{}`", err))?;
                Some((certs, key))
            }
            None => None,
        };

        let plugin_runtime: tokio::runtime::Runtime =
            match tokio::runtime::Builder::new_multi_thread()
                .worker_threads(WORKER_THREAD_NUM)
                .max_blocking_threads(MAX_BLOCK_THREAD_NUM)
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(err) => {
                    bail!("Unable to create tokio runtime, {err}");
                }
            };

        let weak_runtime = Runtime::downgrade(runtime);
        if let Some(runtime) = weak_runtime.upgrade() {
            // Create Map Of Websocket State
            let hm: HashMap<SocketAddr, RemoteState> = HashMap::new();
            let state_map = Arc::new(RwLock::new(hm));

            // Run WebServer
            let runtime_cl = runtime.clone();
            let state_map_cl = state_map.clone();

            let join_handle = run_websocket_server(
                conf.websocket_port,
                &plugin_runtime,
                runtime_cl,
                state_map_cl,
                wss_config,
            );

            // Return WebServer And State
            let running_plugin = RunningPluginInner {
                _zenoh_runtime: runtime,
                _plugin_runtime: plugin_runtime,
                _websocket_server: join_handle,
                _state_map: state_map,
            };
            Ok(Box::new(RunningPlugin(running_plugin)))
        } else {
            bail!("Cannot Get Zenoh Instance of Runtime !")
        }
    }
}

type StateMap = Arc<RwLock<HashMap<SocketAddr, RemoteState>>>;

struct RunningPluginInner {
    _zenoh_runtime: Runtime,
    _plugin_runtime: tokio::runtime::Runtime,
    _websocket_server: JoinHandle<()>,
    _state_map: StateMap,
}

struct RunningPlugin(RunningPluginInner);

impl PluginControl for RunningPlugin {}

impl RunningPluginTrait for RunningPlugin {
    // TODO: Do we want to support changing of config ?
    fn config_checker(
        &self,
        _path: &str,
        _current: &serde_json::Map<String, serde_json::Value>,
        _new: &serde_json::Map<String, serde_json::Value>,
    ) -> ZResult<Option<serde_json::Map<String, serde_json::Value>>> {
        bail!("Runtime configuration change not supported");
    }
}

// Sender
struct RemoteState {
    websocket_tx: Sender<RemoteAPIMsg>,
    session_id: Uuid,
    session: Arc<Session>,
    // key_expr: HashSet<KeyExpr<'static>>,
    // PubSub
    subscribers: HashMap<Uuid, Subscriber<'static, ()>>,
    publishers: HashMap<Uuid, Publisher<'static>>,
    // Queryable
    queryables: HashMap<Uuid, Queryable<'static, ()>>,
    unanswered_queries: Arc<std::sync::RwLock<HashMap<Uuid, Query>>>,
}

pub trait Streamable:
    tokio::io::AsyncRead + tokio::io::AsyncWrite + std::marker::Send + Unpin
{
}
impl Streamable for TcpStream {}
impl Streamable for TlsStream<TcpStream> {}

// Listen on the Zenoh Session
fn run_websocket_server(
    ws_port: String,
    plugin_runtime: &tokio::runtime::Runtime,
    zenoh_runtime: Runtime,
    state_map: StateMap,
    opt_certs: Option<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)>,
) -> JoinHandle<()> {
    let mut opt_tls_acceptor: Option<TlsAcceptor> = None;

    if let Some((certs, key)) = opt_certs {
        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))
            .expect("Could not build TLS Configuration from Certficiate/Key Combo :");
        opt_tls_acceptor = Some(TlsAcceptor::from(Arc::new(config)));
    }

    plugin_runtime.spawn(async move {
        tracing::info!("Spawning Remote API Plugin on {:?}", ws_port);

        // let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 10000);
        // let tcp = TcpListener::bind(socket_addr).await;

        let tcp = TcpListener::bind(ws_port).await;

        let server: TcpListener = match tcp {
            Ok(x) => x,
            Err(err) => {
                tracing::error!("Unable to start TcpListener {err}");
                return;
            }
        };

        while let Ok((tcp_stream, sock_addr)) = server.accept().await {
            println!("raw {sock_addr:?}");
            let sock_adress = Arc::new(sock_addr);
            let (ws_ch_tx, ws_ch_rx) = flume::unbounded::<RemoteAPIMsg>();

            let mut write_guard = state_map.write().await;

            let session = match zenoh::session::init(zenoh_runtime.clone()).await {
                Ok(session) => session.into_arc(),
                Err(err) => {
                    tracing::error!("Unable to get Zenoh session from Runtime {err}");
                    continue;
                }
            };

            let state: RemoteState = RemoteState {
                websocket_tx: ws_ch_tx.clone(),
                session_id: Uuid::new_v4(),
                session,
                subscribers: HashMap::new(),
                publishers: HashMap::new(),
                queryables: HashMap::new(),
                unanswered_queries: Arc::new(std::sync::RwLock::new(HashMap::new())),
            };

            // if remote state exists in map already. Ignore it and reinitialize
            let _ = write_guard.insert(sock_addr, state);
            drop(write_guard);

            let streamable: Box<dyn Streamable> = match &opt_tls_acceptor {
                Some(acceptor) => match acceptor.accept(tcp_stream).await {
                    Ok(tls_stream) => Box::new(tls_stream),
                    Err(err) => {
                        error!("Could not secure TcpStream -> TlsStream {:?}", err);
                        continue;
                    }
                },
                None => Box::new(tcp_stream),
            };

            let ws_stream = tokio_tungstenite::accept_async(streamable)
                .await
                .expect("Error during the websocket handshake occurred");

            let (ws_tx, ws_rx) = ws_stream.split();

            let ch_rx_stream = ws_ch_rx
                .into_stream()
                .map(|remote_api_msg| {
                    // TODO: Take Care of unwrap.
                    let val = serde_json::to_string(&remote_api_msg).unwrap();
                    Ok(Message::Text(val))
                })
                .forward(ws_tx);

            let sock_adress_cl = sock_adress.clone();

            let state_map_cl_outer = state_map.clone();

            //  Incomming message from Websocket
            let incoming_ws = tokio::task::spawn(async move {
                let mut non_close_messages = ws_rx.try_filter(|msg| future::ready(!msg.is_close()));
                let state_map_cl = state_map_cl_outer.clone();
                let sock_adress_ref = sock_adress_cl.clone();
                while let Ok(Some(msg)) = non_close_messages.try_next().await {
                    if let Some(response) =
                        handle_message(msg, *sock_adress_ref, state_map_cl.clone()).await
                    {
                        if let Err(err) = ws_ch_tx.send(response) {
                            error!("WS Send Error: {err:?}");
                        };
                    };
                }
            });

            pin_mut!(ch_rx_stream, incoming_ws);
            future::select(ch_rx_stream, incoming_ws).await;

            state_map.write().await.remove(sock_adress.as_ref());
            tracing::info!("Disconnected {}", sock_adress.as_ref());
        }
    })
}

async fn handle_message(
    msg: Message,
    sock_addr: SocketAddr,
    state_map: StateMap,
) -> Option<RemoteAPIMsg> {
    match msg {
        Message::Text(text) => match serde_json::from_str::<RemoteAPIMsg>(&text) {
            Ok(msg) => match msg {
                RemoteAPIMsg::Control(ctrl_msg) => {
                    match handle_control_message(ctrl_msg, sock_addr, state_map).await {
                        Ok(ok) => return ok.map(RemoteAPIMsg::Control),
                        Err(err) => {
                            tracing::error!(err);
                        }
                    }
                }
                RemoteAPIMsg::Data(data_msg) => {
                    if let Err(err) = handle_data_message(data_msg, sock_addr, state_map).await {
                        tracing::error!(err);
                    }
                }
            },
            Err(err) => {
                error!(
                    "RemoteAPI: WS Message Cannot be Deserialized to RemoteAPIMsg {}",
                    err
                );
            }
        },
        _ => {
            debug!("RemoteAPI: WS Message Not Text");
        }
    };
    None
}
