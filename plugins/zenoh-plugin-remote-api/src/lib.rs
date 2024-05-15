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
use ::serde::{Deserialize, Serialize};
use async_std::net::TcpListener;
use async_std::sync::RwLock;
use async_std::task::{self, JoinHandle};
use async_tungstenite::tungstenite::protocol::frame::coding::Control;
use futures::prelude::*;
use futures::StreamExt;
use futures::{channel::mpsc::unbounded, future, pin_mut};
// use serde::Serialize;
use async_tungstenite::tungstenite::{self, Message};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::net::SocketAddr;
use std::os::unix::net::SocketAddr;
use std::sync::Arc;
use tracing::{debug, error, info};
use uuid::{serde, Uuid};
use zenoh::plugins::{RunningPluginTrait, ZenohPlugin};
use zenoh::prelude::r#async::*;
use zenoh::runtime::Runtime;
use zenoh::Session;
use zenoh_plugin_trait::{plugin_long_version, plugin_version, Plugin, PluginControl};
use zenoh_result::ZResult;

mod config;
pub use config::Config;

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
    type Instance = zenoh::plugins::RunningPlugin;
    const DEFAULT_NAME: &'static str = "rest";
    const PLUGIN_VERSION: &'static str = plugin_version!();
    const PLUGIN_LONG_VERSION: &'static str = plugin_long_version!();

    fn start(name: &str, runtime: &Self::StartArgs) -> ZResult<zenoh::plugins::RunningPlugin> {
        // Try to initiate login.
        // Required in case of dynamic lib, otherwise no logs.
        // But cannot be done twice in case of static link.
        zenoh_util::try_init_log_from_env();

        // Run WebServer
        let join_handle = task::spawn(async {
            run_websocket_server().await;
        });
        // Return WebServer And State
        Ok(Box::new(RunningPlugin(join_handle)))
    }
}

// TODO: Bring Config Back
// struct RunningPlugin(Config);
struct RunningPlugin(JoinHandle<()>);

impl PluginControl for RunningPlugin {}

impl RunningPluginTrait for RunningPlugin {
    fn adminspace_getter<'a>(
        &'a self,
        selector: &'a Selector<'a>,
        plugin_status_key: &str,
    ) -> ZResult<Vec<zenoh::plugins::Response>> {
        let mut responses = Vec::new();
        // TODO :
        Ok(responses)
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

#[derive(Debug, Serialize, Deserialize)]
enum RemoteAPIMsg {
    Data(DataMsg),
    Control(ControlMsg),
}

#[derive(Debug, Serialize, Deserialize)]
enum DataMsg {
    // Put(KeyExpr<'static>, Vec<u8>),
    // SubscriberMessage(Subscriber, Vec<u8>),
}
#[derive(Debug, Serialize, Deserialize)]
struct Subscriber(Uuid);

#[derive(Debug, Serialize, Deserialize)]
struct KeyExprWrapper(KeyExpr<'static>);

#[derive(Debug, Serialize, Deserialize)]
enum ControlMsg {
    // Session
    OpenSession,
    Session(Uuid),
    CloseSession(Uuid),

    // KeyExpr
    CreateKeyExpr,
    KeyExpr(KeyExprWrapper),
    DeleteKeyExpr(),

    // Subscriber
    CreateSubscriber,
    Subscriber(),
    DeleteSubscriber(),
}

type StateMap = Arc<RwLock<HashMap<SocketAddr, RemoteState>>>;

struct RemoteState {
    session_id: Uuid,
    session: Session, // keyExpr: Vec<KeyExpr>,
}

// TODO:
// What we want for this function achieve is to start a Zenoh session
// Listen on the Zenoh Session
pub async fn run_websocket_server() {
    async_std::task::spawn(async move {
        let addr = "127.0.0.1:10000".to_string();
        println!("Spawning Remote API Plugin on {:?}", addr);
        // Server
        let server = TcpListener::bind(addr).await.unwrap();
        let hm: HashMap<SocketAddr, RemoteState> = HashMap::new();
        let state_map = Arc::new(RwLock::new(hm));
        // let state_map
        while let Some(res) = server.incoming().next().await {
            let raw_stream = res.unwrap();
            println!("raw_stream {:?}", raw_stream.peer_addr());
            let sock_adress;
            match raw_stream.peer_addr() {
                Ok(sock_addr) => {
                    let mut write_guard = state_map.write().await;
                    let config = zenoh::config::default();
                    let session = zenoh::open(config).res().await.unwrap();

                    let state = RemoteState {
                        session_id: Uuid::new_v4(),
                        session,
                    };

                    sock_adress = Arc::new(sock_addr.clone());
                    if let Some(remote_state) = write_guard.insert(sock_addr, state) {
                        println!("TODO: remote State existed in Map already, Error in Logic");
                    }
                }
                Err(err) => {
                    println!("Could not Get Peer Address");
                    return;
                }
            }
            let ws_stream = async_tungstenite::accept_async(raw_stream)
                .await
                .expect("Error during the websocket handshake occurred");

            let (ch_tx, ch_rx) = unbounded();

            let (ws_tx, ws_rx) = ws_stream.split();
            let outgoing_ws = ch_rx.map(Ok).forward(ws_tx);

            let sock_adress = sock_adress.clone();
            let state_map_cl = state_map.clone();

            let incoming_ws = ws_rx
                .try_filter(|msg| future::ready(!msg.is_close()))
                .try_for_each(|msg| {
                    println!("Received a message from {}", msg.to_text().unwrap());
                    let state_map_ref = &state_map_cl;
                    let sock_adress_ref = sock_adress.as_ref();
                    if let Some(response) =
                        handle_message(msg, sock_adress_ref, state_map_ref).await
                    {
                        ch_tx.unbounded_send(response).unwrap(); // TODO: Remove Unwrap
                    };

                    future::ok(())
                });

            //
            pin_mut!(outgoing_ws, incoming_ws);
            future::select(outgoing_ws, incoming_ws).await;

            state_map.write().await.remove(&*sock_adress);
            println!("disconnected");
        }
    });
}

async fn handle_message(
    msg: Message,
    sock_addr: &SocketAddr,
    state_map: &StateMap,
) -> Option<Message> {
    match msg.clone() {
        tungstenite::Message::Text(text) => match serde_json::from_str::<RemoteAPIMsg>(&text) {
            Ok(msg) => match msg {
                RemoteAPIMsg::Control(ctrl_msg) => {
                    handle_control_message(ctrl_msg, sock_addr, state_map).await
                }
                RemoteAPIMsg::Data(data_msg) => handle_data_message(data_msg),
            },
            Err(err) => {
                debug!(
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

async fn handle_control_message(
    ctrl_msg: ControlMsg,
    sock_addr: &SocketAddr,
    state_map: &StateMap,
) -> Option<Message> {
    match ctrl_msg {
        ControlMsg::OpenSession => {
            let read = state_map.read().await;
            if let Some(state_map) = read.get(sock_addr) {
                let json: String = serde_json::to_string(&RemoteAPIMsg::Control(
                    ControlMsg::Session(state_map.session_id),
                ))
                .unwrap();
                Some(Message::Text(json))
            } else {
                println!("State Map Does not contain SocketAddr");
                None
            }
        }
        ControlMsg::Session(id) => todo!(),
        ControlMsg::CloseSession(id) => todo!(),

        ControlMsg::CreateKeyExpr => todo!(),
        ControlMsg::KeyExpr(k) => todo!(),
        ControlMsg::DeleteKeyExpr() => todo!(),
        ControlMsg::CreateSubscriber => todo!(),
        ControlMsg::Subscriber() => todo!(),
        ControlMsg::DeleteSubscriber() => todo!(),
    }
}

fn handle_data_message(data_msg: DataMsg) -> Option<Message> {
    match data_msg{
        // DataMsg::Put(_, _) => todo!(),
        // DataMsg::SubscriberMessage(_, _) => todo!(),
    }
    None
}
