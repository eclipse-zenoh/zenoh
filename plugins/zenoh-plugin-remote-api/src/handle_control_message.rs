use std::net::SocketAddr;

use tracing::{error, warn};
use uuid::Uuid;
use zenoh::{key_expr::KeyExpr, query::Selector, session::SessionDeclarations};

use crate::{
    interface::{ControlMsg, DataMsg, QueryWS, QueryableMsg, RemoteAPIMsg, ReplyWS, SampleWS},
    StateMap,
};

pub async fn handle_control_message(
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
                tracing::error!("State Map Does not contain SocketAddr");
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
        //
        // ControlMsg::CreateKeyExpr(key_expr_str) => {
        //     let mut state_writer = state_map.write().await;
        //     if let Some(remote_state) = state_writer.get_mut(&sock_addr) {
        //         let key_expr = KeyExpr::new(key_expr_str).unwrap();
        //         remote_state.key_expr.insert(key_expr.clone());
        //         Some(ControlMsg::KeyExpr(key_expr.to_string()))
        //     } else {
        //         println!("State Map Does not contain SocketAddr");
        //         None
        //     }
        // }
        //
        ControlMsg::Get {
            key_expr,
            parameters,
            id,
        } => {
            println!("key_expr {:?}", key_expr);
            println!("parameters {:?}", parameters);
            println!("id {:?}", id);

            let mut state_reader = state_map.write().await;
            if let Some(state) = state_reader.get_mut(&sock_addr) {
                let selector = Selector::owned(key_expr, parameters.unwrap_or_default());

                match state.session.get(selector).await {
                    Ok(receiver) => {
                        let mut receiving = true;
                        while receiving {
                            match receiver.recv_async().await {
                                Ok(reply) => {
                                    println!("Reply Receieved {:?}", reply);
                                    let reply_ws = ReplyWS::from((reply, id));
                                    let remote_api_msg =
                                        RemoteAPIMsg::Data(DataMsg::GetReply(reply_ws));
                                    if let Err(err) = state.websocket_tx.send(remote_api_msg) {
                                        tracing::error!("{}", err);
                                    }
                                }
                                Err(_) => receiving = false,
                            }
                        }

                        let remote_api_msg = RemoteAPIMsg::Control(ControlMsg::GetFinished { id });
                        if let Err(err) = state.websocket_tx.send(remote_api_msg) {
                            tracing::error!("{}", err);
                        };
                        println!("End End Reply");
                    }
                    Err(err) => {
                        error!("Session err Failed ! {}", err)
                    }
                };
            }
            None
        }
        ControlMsg::Put { key_expr, payload } => {
            let mut state_reader = state_map.write().await;
            if let Some(state) = state_reader.get_mut(&sock_addr) {
                if let Err(err) = state.session.put(key_expr, payload).await {
                    error!("Session Put Failed ! {}", err)
                };
            }
            None
        }
        ControlMsg::Delete { key_expr } => {
            let mut state_reader = state_map.write().await;
            if let Some(state) = state_reader.get_mut(&sock_addr) {
                if let Err(err) = state.session.delete(key_expr).await {
                    error!("Session Delete Failed ! {}", err)
                };
            }
            None
        }
        // SUBSCRIBER
        ControlMsg::DeclareSubscriber {
            key_expr: key_expr_str,
            id: subscriber_uuid,
        } => {
            let mut state_writer = state_map.write().await;
            if let Some(remote_state) = state_writer.get_mut(&sock_addr) {
                let key_expr = KeyExpr::new(key_expr_str).unwrap();
                let ch_tx = remote_state.websocket_tx.clone();
                let subscriber_uuid_cl = subscriber_uuid.clone();
                let res_subscriber = remote_state
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
                    .await;

                match res_subscriber {
                    Ok(subscriber) => {
                        remote_state.subscribers.insert(subscriber_uuid, subscriber);
                    }
                    Err(err) => {
                        tracing::error!("Error {}", err)
                    }
                }

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
        // Publisher
        ControlMsg::DeclarePublisher { key_expr, id: uuid } => {
            println!("Declare Publisher {}  {}", key_expr, uuid);
            let mut state_reader = state_map.write().await;
            if let Some(state) = state_reader.get_mut(&sock_addr) {
                match state.session.declare_publisher(key_expr.clone()).await {
                    Ok(publisher) => {
                        state.publishers.insert(uuid, publisher);
                        tracing::info!("Publisher Created {uuid:?} : {key_expr:?}");
                    }
                    Err(err) => {
                        tracing::error!("Could not Create Publisher {err}");
                    }
                };
            }
            None
        }
        ControlMsg::UndeclarePublisher(uuid) => {
            let mut state_reader = state_map.write().await;
            if let Some(state) = state_reader.get_mut(&sock_addr) {
                if let Some(publisher) = state.publishers.remove(&uuid) {
                    if let Err(err) = publisher.undeclare().await {
                        error!("UndeclarePublisher Error: {err}");
                    };
                } else {
                    warn!("UndeclarePublisher: No Publisher with UUID {uuid}");
                }
            }
            None
        }
        // Backend should not receive this, make it unrepresentable
        ControlMsg::DeclareQueryable {
            key_expr,
            complete,
            id: queryable_uuid,
        } => {
            println!("Declare Queryable {}  {}", key_expr, queryable_uuid);
            let mut state_reader = state_map.write().await;
            if let Some(state) = state_reader.get_mut(&sock_addr) {
                let unanswered_queries = state.unanswered_queries.clone();
                let session = state.session.clone();
                let ch_tx = state.websocket_tx.clone();

                let queryable_res = session
                    .declare_queryable(&key_expr)
                    .complete(complete)
                    .callback(move |query| {
                        println!("Query Received {}", query);

                        let query_uuid = Uuid::new_v4();
                        let query_ws: QueryWS = QueryWS::from((&query, query_uuid));
                        let queryable_msg = QueryableMsg::Query {
                            queryable_uuid,
                            query: query_ws,
                        };

                        let remote_msg = RemoteAPIMsg::Data(DataMsg::Queryable(queryable_msg));
                        if let Err(err) = ch_tx.send(remote_msg) {
                            tracing::error!("Could not send Queryable Message on WS {}", err);
                        };

                        match unanswered_queries.write() {
                            Ok(mut rw_lock) => {
                                rw_lock.insert(query_uuid, query);
                            }
                            Err(err) => tracing::error!("Query RwLock has been poisoned {err:?}"),
                        }
                    })
                    .await;

                match queryable_res {
                    Ok(queryable) => {
                        state.queryables.insert(queryable_uuid, queryable);
                    }
                    Err(err) => {
                        tracing::error!("Could not Create Publisher {err}");
                    }
                }
            }
            None
        }
        ControlMsg::UndeclareQueryable(uuid) => {
            let mut state_reader = state_map.write().await;
            if let Some(state) = state_reader.get_mut(&sock_addr) {
                if let Some(queryable) = state.queryables.remove(&uuid) {
                    let x = queryable.undeclare().await;
                };
            }
            None
        }
        msg @ (ControlMsg::GetFinished { id: _ }
        | ControlMsg::Session(_)
        | ControlMsg::Subscriber(_)) => {
            // make server recieving these types unrepresentable
            error!("Backend should not recieve this message Type: {msg:?}");
            None
        }
    }
}
