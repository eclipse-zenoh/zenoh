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
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::Arc,
};

use async_std::{
    net::TcpListener,
    sync::RwLock,
    task::{self, JoinHandle},
};
use async_tungstenite::tungstenite::{self, Message};
use flume::Sender;
use futures::{future, pin_mut, prelude::*};
use interface::RemoteAPIMsg;
use tracing::{debug, error};
use uuid::Uuid;
use zenoh::{
    internal::{
        plugins::{RunningPluginTrait, ZenohPlugin},
        runtime::Runtime,
    },
    key_expr::KeyExpr,
    pubsub::{Publisher, Subscriber},
    query::{Query, Queryable},
    Session,
};
use zenoh_plugin_trait::{plugin_long_version, plugin_version, Plugin, PluginControl};
use zenoh_result::{bail, ZResult};

mod config;
pub use config::Config;

mod handle_control_message;
mod handle_data_message;
mod interface;
use crate::{
    handle_control_message::handle_control_message, handle_data_message::handle_data_message,
};

const GIT_VERSION: &str = git_version::git_version!(prefix = "v", cargo_prefix = "v");
lazy_static::lazy_static! {
    static ref LONG_VERSION: String = format!("{} built with {}", GIT_VERSION, env!("RUSTC_VERSION"));
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
        let weak_runtime = Runtime::downgrade(runtime);
        if let Some(runtime) = weak_runtime.upgrade() {
            // Create Map Of Websocket State
            let hm: HashMap<SocketAddr, RemoteState> = HashMap::new();
            let state_map = Arc::new(RwLock::new(hm));

            // Run WebServer
            let runtime_cl = runtime.clone();
            let state_map_cl = state_map.clone();
            let join_handle = task::spawn(async {
                run_websocket_server(runtime_cl, state_map_cl).await;
            });

            // Return WebServer And State
            let running_plugin = RunningPluginInner {
                runtime,
                websocket_server: join_handle,
                state_map,
            };
            Ok(Box::new(RunningPlugin(running_plugin)))
        } else {
            bail!("Cannot Get Instance of Runtime !")
        }
    }
}

// TODO: Bring Config Back
// struct RunningPlugin(Config);

type StateMap = Arc<RwLock<HashMap<SocketAddr, RemoteState>>>;

struct RunningPluginInner {
    runtime: Runtime,
    websocket_server: JoinHandle<()>,
    state_map: StateMap,
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
    key_expr: HashSet<KeyExpr<'static>>,
    // PubSub
    subscribers: HashMap<Uuid, Subscriber<'static, ()>>,
    publishers: HashMap<Uuid, Publisher<'static>>,
    // Queryable
    queryables: HashMap<Uuid, Queryable<'static, ()>>,
    // queryables: HashMap<Uuid, Queryable<'static, flume::Receiver<Query>>>,
    unanswered_queries: Arc<std::sync::RwLock<HashMap<Uuid, Query>>>,
}

// Listen on the Zenoh Session
async fn run_websocket_server(runtime: Runtime, state_map: StateMap) {
    // runtime.
    async_std::task::spawn(async move {
        let addr = "127.0.0.1:10000".to_string();
        println!("Spawning Remote API Plugin on {:?}", addr);
        // Server
        let server = TcpListener::bind(addr).await.unwrap();

        // let state_map
        while let Some(res) = futures::StreamExt::next(&mut server.incoming()).await {
            let raw_stream = res.unwrap();

            println!("raw_stream {:?}", raw_stream.peer_addr());

            let sock_adress;
            let (ws_ch_tx, ws_ch_rx) = flume::unbounded::<RemoteAPIMsg>();

            // let (q_r_tx, q_r_rx) = flume::unbounded::<QueryableMsg>();

            match raw_stream.peer_addr() {
                Ok(sock_addr) => {
                    let mut write_guard = state_map.write().await;

                    let session = zenoh::session::init(runtime.clone())
                        .await
                        .unwrap()
                        .into_arc();

                    let state: RemoteState = RemoteState {
                        websocket_tx: ws_ch_tx.clone(),
                        session_id: Uuid::new_v4(),
                        session,
                        key_expr: HashSet::new(),
                        subscribers: HashMap::new(),
                        publishers: HashMap::new(),
                        queryables: HashMap::new(),
                        unanswered_queries: Arc::new(std::sync::RwLock::new(HashMap::new())),
                    };

                    sock_adress = Arc::new(sock_addr);
                    if let Some(remote_state) = write_guard.insert(sock_addr, state) {
                        error!(
                            "remote State existed in Map already {:?}",
                            remote_state.session_id
                        );
                        println!("TODO: remote State existed in Map already, Error in Logic?");
                    }
                }
                Err(err) => {
                    error!("Could not Get Peer Address {err}");
                    return;
                }
            }
            let ws_stream = async_tungstenite::accept_async(raw_stream)
                .await
                .expect("Error during the websocket handshake occurred");

            let (ws_tx, ws_rx) = ws_stream.split();

            let ch_rx_stream = ws_ch_rx
                .into_stream()
                .map(|remote_api_msg| {
                    let val = serde_json::to_string(&remote_api_msg).unwrap();
                    Ok(Message::Text(val))
                })
                .forward(ws_tx);

            let sock_adress_cl = sock_adress.clone();
            let state_map_cl_outer = state_map.clone();

            //  Incomming message from Websocket
            let incoming_ws = async_std::task::spawn(async move {
                let mut non_close_messages = ws_rx.try_filter(|msg| future::ready(!msg.is_close()));
                let state_map_cl = state_map_cl_outer.clone();
                let sock_adress_ref = sock_adress_cl.clone();
                while let Ok(Some(msg)) = non_close_messages.try_next().await {
                    if let Some(response) =
                        handle_message(msg, *sock_adress_ref, state_map_cl.clone()).await
                    {
                        ws_ch_tx.send(response).unwrap();
                    };
                }
            });

            pin_mut!(ch_rx_stream, incoming_ws);
            future::select(ch_rx_stream, incoming_ws).await;

            state_map.write().await.remove(sock_adress.as_ref());
            println!("disconnected");
        }
    });
}

async fn handle_message(
    msg: Message,
    sock_addr: SocketAddr,
    state_map: StateMap,
) -> Option<RemoteAPIMsg> {
    match msg.clone() {
        tungstenite::Message::Text(text) => match serde_json::from_str::<RemoteAPIMsg>(&text) {
            Ok(msg) => match msg {
                RemoteAPIMsg::Control(ctrl_msg) => {
                    handle_control_message(ctrl_msg, sock_addr, state_map)
                        .await
                        .map(RemoteAPIMsg::Control)
                }
                RemoteAPIMsg::Data(data_msg) => {
                    handle_data_message(data_msg, sock_addr, state_map).await
                }
            },
            Err(err) => {
                println!("{} \n {}", err, text);
                error!(
                    "RemoteAPI: WS Message Cannot be Deserialized to RemoteAPIMsg {}",
                    err
                );
                None
            }
        },
        _ => {
            debug!("RemoteAPI: WS Message Not Text");
            None
        }
    }
}
