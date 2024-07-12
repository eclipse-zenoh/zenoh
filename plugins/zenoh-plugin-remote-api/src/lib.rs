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

use async_std::net::TcpListener;
use async_std::sync::RwLock;
use async_std::task::{self, JoinHandle};
use async_tungstenite::tungstenite::{self, Message};
use interface::{ControlMsg, DataMsg, RemoteAPIMsg, SampleWS};

use flume::Sender;
use futures::prelude::*;
use futures::{future, pin_mut};

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{debug, error, warn};
use uuid::Uuid;
use zenoh::prelude::*;
use zenoh::pubsub::{Publisher, Subscriber};
use zenoh::Session;
use zenoh::{
    internal::{
        plugins::{RunningPluginTrait, ZenohPlugin},
        runtime::Runtime,
    },
    key_expr::KeyExpr,
};
use zenoh_plugin_trait::{plugin_long_version, plugin_version, Plugin, PluginControl};
use zenoh_result::ZResult;

mod config;
pub use config::Config;

mod interface;

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
        _runtime: &Self::StartArgs,
    ) -> ZResult<zenoh::internal::plugins::RunningPlugin> {
        // Try to initiate login.
        // Required in case of dynamic lib, otherwise no logs.
        // But cannot be done twice in case of static link.
        zenoh_util::try_init_log_from_env();
        tracing::info!("Starting {name}");
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

impl RunningPluginTrait for RunningPlugin {}

type StateMap = Arc<RwLock<HashMap<SocketAddr, RemoteState>>>;

// sender
struct RemoteState {
    channel_tx: Sender<RemoteAPIMsg>,
    session_id: Uuid,
    // session: Session,
    session: Arc<Session>,
    key_expr: HashSet<KeyExpr<'static>>,
    subscribers: HashMap<Uuid, Subscriber<'static, ()>>,
    publishers: HashMap<Uuid, Publisher<'static>>,
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
        while let Some(res) = futures::StreamExt::next(&mut server.incoming()).await {
            let raw_stream = res.unwrap();

            println!("raw_stream {:?}", raw_stream.peer_addr());

            let sock_adress;
            let (ch_tx, ch_rx) = flume::unbounded::<RemoteAPIMsg>();

            match raw_stream.peer_addr() {
                Ok(sock_addr) => {
                    let mut write_guard = state_map.write().await;
                    let config = zenoh::config::default();
                    let session = zenoh::open(config).await.unwrap().into_arc();
                    // let session = zenoh::open(config).await.unwrap();

                    let state: RemoteState = RemoteState {
                        channel_tx: ch_tx.clone(),
                        session_id: Uuid::new_v4(),
                        session,
                        key_expr: HashSet::new(),
                        subscribers: HashMap::new(),
                        publishers: HashMap::new(),
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

            let ch_rx_stream = ch_rx
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
                        ch_tx.send(response).unwrap(); // TODO: Remove Unwrap
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

async fn handle_control_message(
    ctrl_msg: ControlMsg,
    sock_addr: SocketAddr,
    state_map: StateMap,
) -> Option<ControlMsg> {
    match ctrl_msg {
        ControlMsg::OpenSession => {
            let state_reader = state_map.read().await;
            if let Some(state_map) = state_reader.get(&sock_addr) {
                Some(ControlMsg::Session(state_map.session_id))
            } else {
                println!("State Map Does not contain SocketAddr");
                None
            }
        }
        ControlMsg::CloseSession => {
            // session.close().res().await.unwrap();
            let mut state_write = state_map.write().await;
            if let Some(state_map) = state_write.remove(&sock_addr) {
                //  Undeclare Publishers and Subscribers
                for (_, publisher) in state_map.publishers {
                    if let Err(err) = publisher.undeclare().await {
                        tracing::error!("Close Session, Error undeclaring Publisher {err}");
                    };
                }
                for (_, subscriber) in state_map.subscribers {
                    if let Err(err) = subscriber.undeclare().await {
                        tracing::error!("Close Session, Error undeclaring Subscriber {err}");
                    };
                }

                //  Close Session
                // TODO: Close session, tie lifetime of session to statemap entry

                // let x = state_map;
                // let mut_borrow = state_map.session.borrow_mut();
                // if let Err(err)= state_map.session.close().await{
                //     tracing::error!("Could not close session {err}");
                //     Some(ControlMsg::Error(err.to_string()))
                // }else{
                //     None
                // }
                None
            } else {
                println!("State Map Does not contain SocketAddr");
                None
            }
            // None
        }
        ControlMsg::CreateKeyExpr(key_expr_str) => {
            let mut state_writer = state_map.write().await;
            if let Some(remote_state) = state_writer.get_mut(&sock_addr) {
                let key_expr = KeyExpr::new(key_expr_str).unwrap();
                remote_state.key_expr.insert(key_expr.clone());
                Some(ControlMsg::KeyExpr(key_expr.to_string()))
            } else {
                println!("State Map Does not contain SocketAddr");
                None
            }
        }
        ControlMsg::DeleteKeyExpr(_) => todo!(),
        //
        ControlMsg::DeclareSubscriber(key_expr_str, subscriber_uuid) => {
            let mut state_writer = state_map.write().await;
            println!("{}, {}", key_expr_str, subscriber_uuid);

            if let Some(remote_state) = state_writer.get_mut(&sock_addr) {
                let key_expr = KeyExpr::new(key_expr_str).unwrap();
                let ch_tx = remote_state.channel_tx.clone();

                println!("Key Expression {key_expr}");
                let subscriber_uuid_cl = subscriber_uuid.clone();

                let subscriber = remote_state
                    .session
                    .declare_subscriber(key_expr)
                    .callback(move |sample| {
                        println!("RCV sample {}", sample.key_expr());

                        match SampleWS::try_from(sample) {
                            Ok(sample_ws) => {
                                let remote_api_message = RemoteAPIMsg::Data(DataMsg::Sample(
                                    sample_ws,
                                    subscriber_uuid_cl.clone(),
                                ));
                                if let Err(e) = ch_tx.send(remote_api_message) {
                                    error!("Forward Sample Channel error: {e}");
                                };
                            }
                            Err(err) => {
                                error!("Could not convert Sample into SampleWs {:?}", err)
                            }
                        };
                    })
                    .await
                    .unwrap();

                // let arc_subscriber = Arc::new(subscriber);
                remote_state.subscribers.insert(subscriber_uuid, subscriber);

                Some(ControlMsg::Subscriber(subscriber_uuid))
            } else {
                println!("State Map Does not contain SocketAddr");
                None
            }
        }
        ControlMsg::UndeclareSubscriber(uuid) => {
            let mut state_reader = state_map.write().await;
            if let Some(state) = state_reader.get_mut(&sock_addr) {
                if let Some(subscriber) = state.subscribers.remove(&uuid) {
                    if let Err(err) = subscriber.undeclare().await {
                        tracing::error!("Subscriber Undeclaration Error :{err}");
                    };
                }
            }
            None
        }
        ControlMsg::DeclarePublisher(key_expr, uuid) => {
            println!("Declare Publisher {}  {}", key_expr, uuid);
            //
            let mut state_reader = state_map.write().await;
            //
            if let Some(state) = state_reader.get_mut(&sock_addr) {
                //
                match state.session.declare_publisher(key_expr.clone()).await {
                    Ok(publisher) => {
                        state.publishers.insert(uuid, publisher);
                        tracing::info!("Publisher Created {uuid:?} : {key_expr:?}");
                    }
                    Err(err) => {
                        tracing::error!("Could not Create Publisher {err}");
                        println!("Could not Create Publisher {err}");
                        return Some(ControlMsg::Error(err.to_string()));
                    }
                };
            }
            None
        }

        //

        // Backend should not receive this, make it unrepresentable
        ControlMsg::Session(_) | ControlMsg::KeyExpr(_) | ControlMsg::Subscriber(_) => {
            // TODO: Move these into own type
            // make server recieving these types unrepresentable
            println!("Backend should not get these types");
            error!("Backend should not get these types");
            None
        }
        ControlMsg::Error(client_err) => {
            error!("Client sent error {}", client_err);
            None
        }
    }
}

async fn handle_data_message(
    data_msg: DataMsg,
    sock_addr: SocketAddr,
    state_map: StateMap,
) -> Option<RemoteAPIMsg> {
    match data_msg {
        DataMsg::Sample(sample, publisher_uuid) => {
            warn!("Server has Recieved A Sample");
        }
        DataMsg::PublisherPut(payload, publisher_uuid) => {
            let state_reader = state_map.read().await;
            if let Some(state) = state_reader.get(&sock_addr) {
                if let Some(publisher) = state.publishers.get(&publisher_uuid) {
                    if let Err(err) = publisher.put(payload).await {
                        err.to_string();
                    }
                } else {
                    tracing::warn!("Publisher {publisher_uuid}, does not exist in State");
                }
            } else {
                tracing::warn!("No state in map for Socket Addres {sock_addr}");
                println!("No state in map for Socket Addres {sock_addr}");
            }
        }
    }
    None
}
