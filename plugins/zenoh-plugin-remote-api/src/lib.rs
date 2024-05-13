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
use async_std::task::{self, JoinHandle};
use futures::prelude::*;
use futures::StreamExt;
use futures::{channel::mpsc::unbounded, future, pin_mut};
// use serde::Serialize;
use async_tungstenite::tungstenite;
use std::collections::HashMap;
use std::convert::TryFrom;
use tracing::{debug, info};
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

// type StateMap = HashMap<(),()>;

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
    Put(KeyExpr<'static>, Vec<u8>),
    SubscriberMessage(Subscriber, Vec<u8>),
}

#[derive(Debug, Serialize, Deserialize)]
struct Subscriber(Uuid);

#[derive(Debug, Serialize, Deserialize)]
struct KeyExpr2(KeyExpr<'static>);

#[derive(Debug, Serialize, Deserialize)]
enum ControlMsg {
    CreateSession,
    Session(Subscriber),
    DeleteSession(Subscriber),

    // KeyExpr
    CreateKeyExpr,
    KeyExpr(),
    DeleteKeyExpr(),

    // Subscriber
    CreateSubscriber,
    Subscriber(),
    DeleteSubscriber(),
}

// TODO:
// What we want for this function achieve is to start a Zenoh session
// Listen on the Zenoh Session
pub async fn run_websocket_server() {
    async_std::task::spawn(async move {
        let addr = "127.0.0.1:3012".to_string();
        info!("Spawning Remote API Plugin on {:?}", addr);
        // println!("Spawning Remote API Plugin on {:?}",addr);

        let server = TcpListener::bind(addr).await.unwrap();

        while let Some(res) = server.incoming().next().await {
            let raw_stream = res.unwrap();
            println!("raw_stream {:?}", raw_stream.peer_addr());

            let ws_stream = async_tungstenite::accept_async(raw_stream)
                .await
                .expect("Error during the websocket handshake occurred");

            let (ch_tx, ch_rx) = unbounded();

            let (ws_tx, ws_rx) = ws_stream.split();

            let outgoing_ws = ch_rx.map(Ok).forward(ws_tx);

            let incoming_ws = ws_rx
                .try_filter(|msg| future::ready(!msg.is_close()))
                .try_for_each(|msg| {
                    println!("Received a message from {}", msg.to_text().unwrap());

                    // TODO: Remove Clone
                    match msg.clone() {
                        // tungstenite::Message::Binary(bytes) => {
                        //     #TODO: Use Bincode or another format
                        // },
                        tungstenite::Message::Text(text) => {
                            match serde_json::from_str::<RemoteAPIMsg>(&text){
                                Ok(msg) => match msg {
                                    RemoteAPIMsg::Control(ctrl_msg) => {
                                        handle_control_message(ctrl_msg);
                                    },
                                    RemoteAPIMsg::Data(data_msg) => match data_msg{
                                        DataMsg::Put(key_expr, data) => {
                                            todo!();
                                        },
                                        DataMsg::SubscriberMessage(sub, data) => {
                                            todo!();
                                        },
                                    },
                                },
                                Err(err) => {
                                    debug!("RemoteAPI: WS Message Cannot be Deserialized to RemoteAPIMsg {}" , err);
                                },
                            };
                        },
                        _ => {
                            debug!("RemoteAPI: WS Message Not Text");
                        }
                    };


                    ch_tx.unbounded_send(msg).unwrap();
                    future::ok(())
                });
            //
            pin_mut!(outgoing_ws, incoming_ws);
            future::select(outgoing_ws, incoming_ws).await;
            println!("disconnected");
        }
    });
}

fn handle_control_message(ctrl_msg: ControlMsg) {
    match ctrl_msg {
        ControlMsg::CreateSession => todo!(),
        ControlMsg::Session(_) => todo!(),
        ControlMsg::DeleteSession(_) => todo!(),
        ControlMsg::CreateKeyExpr => todo!(),
        ControlMsg::KeyExpr() => todo!(),
        ControlMsg::DeleteKeyExpr() => todo!(),
        ControlMsg::CreateSubscriber => todo!(),
        ControlMsg::Subscriber() => todo!(),
        ControlMsg::DeleteSubscriber() => todo!(),
    }
}
