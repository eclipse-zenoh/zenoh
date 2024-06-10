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
use async_tungstenite::tungstenite::{self, Message};

use flume::Sender;
use futures::prelude::*;
use futures::{future, pin_mut};

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;

use tracing::{debug, error};

use uhlc::Timestamp;
use uuid::Uuid;

use zenoh::plugins::{RunningPluginTrait, ZenohPlugin};
use zenoh::prelude::r#async::*;
use zenoh::runtime::Runtime;
use zenoh::subscriber::Subscriber;
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
    const DEFAULT_NAME: &'static str = "remote_api";
    const PLUGIN_VERSION: &'static str = plugin_version!();
    const PLUGIN_LONG_VERSION: &'static str = plugin_long_version!();

    fn start(name: &str, runtime: &Self::StartArgs) -> ZResult<zenoh::plugins::RunningPlugin> {
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

#[derive(Debug, Serialize, Deserialize)]
enum RemoteAPIMsg {
    Data(DataMsg),
    Control(ControlMsg),
}

#[derive(Debug, Serialize, Deserialize)]
enum DataMsg {
    Sample(SampleWS), // SubscriberMessage(Uuid, Vec<u8>),
}
// #[derive(Debug, Serialize, Deserialize)]
// struct Subscriber(Uuid);

#[derive(Debug, Serialize, Deserialize)]
struct KeyExprWrapper(KeyExpr<'static>);

#[derive(Debug, Serialize, Deserialize)]
enum ControlMsg {
    // Session
    OpenSession,
    Session(Uuid),
    CloseSession(Uuid),

    // KeyExpr
    CreateKeyExpr(String),
    KeyExpr(KeyExprWrapper),
    DeleteKeyExpr(KeyExprWrapper),

    // Subscriber
    DeclareSubscriber(String),
    // CreateSubscriber(KeyExprWrapper),
    Subscriber(Uuid),
    UndeclareSubscriber(),

    //
    Error(),
}

#[derive(Debug, Serialize, Deserialize)]
enum ErrorMsg {}

#[derive(Debug, Serialize, Deserialize)]
struct SampleWS {
    key_expr: KeyExpr<'static>,
    value: Vec<u8>,
    kind: SampleKindWS,
    timestamp: Option<Timestamp>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum SampleKindWS {
    Put = 0,
    Delete = 1,
}

impl From<SampleKind> for SampleKindWS {
    fn from(sk: SampleKind) -> Self {
        match sk {
            SampleKind::Put => SampleKindWS::Put,
            SampleKind::Delete => SampleKindWS::Delete,
        }
    }
}
impl From<Sample> for SampleWS {
    fn from(s: Sample) -> Self {
        let z_buf = s.value.payload;

        // TODO allocate with Capacity ?
        let vec_buf = z_buf
            .slices()
            .into_iter()
            .fold(Vec::new(), |mut acc, item| {
                acc.extend(item.iter().cloned());
                acc
            });

        SampleWS {
            key_expr: s.key_expr,
            value: vec_buf,
            kind: s.kind.into(),
            timestamp: s.timestamp,
        }
    }
}

type StateMap = Arc<RwLock<HashMap<SocketAddr, RemoteState>>>;

// sender
struct RemoteState {
    channel_tx: Sender<RemoteAPIMsg>,
    session_id: Uuid,
    session: Arc<Session>,               // keyExpr: Vec<KeyExpr>,
    key_expr: HashSet<KeyExpr<'static>>, // keyExpr: Vec<KeyExpr>,
    // subscribers: HashMap<Uuid, Arc<Subscriber<'static, Receiver<Sample>>>>,
    subscribers: HashMap<Uuid, Arc<Subscriber<'static, ()>>>,
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
                    let session = zenoh::open(config).res().await.unwrap().into_arc();

                    let state = RemoteState {
                        channel_tx: ch_tx.clone(),
                        session_id: Uuid::new_v4(),
                        session,
                        key_expr: HashSet::new(),
                        subscribers: HashMap::new(),
                    };

                    sock_adress = Arc::new(sock_addr.clone());
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

            state_map.write().await.remove((&sock_adress).as_ref());
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
                        .and_then(|x| Some(RemoteAPIMsg::Control(x)))
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
        ControlMsg::Session(id) => todo!(),
        ControlMsg::CloseSession(id) => todo!(),
        ControlMsg::CreateKeyExpr(key_expr_str) => {
            let mut state_writer = state_map.write().await;
            if let Some(remote_state) = state_writer.get_mut(&sock_addr) {
                let key_expr = KeyExpr::new(key_expr_str).unwrap();
                remote_state.key_expr.insert(key_expr.clone());
                return Some(ControlMsg::KeyExpr(KeyExprWrapper(key_expr)));
            } else {
                println!("State Map Does not contain SocketAddr");
                None
            }
        }

        ControlMsg::DeleteKeyExpr(_) => todo!(),
        ControlMsg::DeclareSubscriber(key_expr_str) => {
            let mut state_writer = state_map.write().await;

            if let Some(remote_state) = state_writer.get_mut(&sock_addr) {
                let key_expr = KeyExpr::new(key_expr_str).unwrap();
                let ch_tx = remote_state.channel_tx.clone();

                let subscriber = remote_state
                    .session
                    .declare_subscriber(key_expr)
                    .callback(move |sample| {
                        let sample_ws = SampleWS::from(sample);
                        let remote_api_message = RemoteAPIMsg::Data(DataMsg::Sample(sample_ws));
                        if let Err(e) = ch_tx.send(remote_api_message) {
                            error!("Forward Sample Channel error: {e}");
                        };
                    })
                    .res()
                    .await
                    .unwrap();

                let sub_id = Uuid::new_v4();
                let arc_subscriber = Arc::new(subscriber);
                remote_state.subscribers.insert(sub_id, arc_subscriber);

                Some(ControlMsg::Subscriber(sub_id))
            } else {
                println!("State Map Does not contain SocketAddr");
                None
            }
        }
        ControlMsg::UndeclareSubscriber() => todo!(),
        // Backend should not receive this, make it unrepresentable
        ControlMsg::KeyExpr(_) => todo!(),
        ControlMsg::Subscriber(_) => todo!(),
        ControlMsg::Error() => todo!(),
    }
}

fn handle_data_message(data_msg: DataMsg) -> Option<RemoteAPIMsg> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_messages() {
        let json: String =
            serde_json::to_string(&RemoteAPIMsg::Control(ControlMsg::OpenSession)).unwrap();
        eprintln!("{}", json);
        let json: String =
            serde_json::to_string(&RemoteAPIMsg::Control(ControlMsg::Session(Uuid::new_v4())))
                .unwrap();
        eprintln!("{}", json);
        let json: String = serde_json::to_string(&RemoteAPIMsg::Control(ControlMsg::CloseSession(
            Uuid::new_v4(),
        )))
        .unwrap();
        eprintln!("{}", json);
        let json: String = serde_json::to_string(&RemoteAPIMsg::Control(
            ControlMsg::CreateKeyExpr("demo/tech".into()),
        ))
        .unwrap();
        eprintln!("{}", json);
        let key_expr = KeyExpr::new("demo/test").unwrap();
        let json: String = serde_json::to_string(&RemoteAPIMsg::Control(ControlMsg::KeyExpr(
            KeyExprWrapper(key_expr),
        )))
        .unwrap();
        eprintln!("{}", json);

        let json: String = serde_json::to_string(&RemoteAPIMsg::Control(
            ControlMsg::DeclareSubscriber("hello".into()),
        ))
        .unwrap();
        eprintln!("{}", json);

        assert_eq!(1, 2);
    }
}
