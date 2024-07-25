use std::net::SocketAddr;

use tracing::{error, warn};
use zenoh::query::Query;

use crate::{
    interface::{DataMsg, QueryableMsg, RemoteAPIMsg},
    StateMap,
};

pub async fn handle_data_message(
    data_msg: DataMsg,
    sock_addr: SocketAddr,
    state_map: StateMap,
) -> Option<RemoteAPIMsg> {
    match data_msg {
        DataMsg::PublisherPut(payload, publisher_uuid) => {
            let state_reader = state_map.read().await;
            if let Some(state) = state_reader.get(&sock_addr) {
                if let Some(publisher) = state.publishers.get(&publisher_uuid) {
                    if let Err(err) = publisher.put(payload).await {
                        error!("PublisherPut {publisher_uuid}, {err}");
                    }
                } else {
                    warn!("Publisher {publisher_uuid}, does not exist in State");
                }
            } else {
                warn!("No state in map for Socket Address {sock_addr}");
            }
            None
        }
        DataMsg::Put { key_expr, payload } => {
            let mut state_reader = state_map.write().await;
            if let Some(state) = state_reader.get_mut(&sock_addr) {
                if let Err(err) = state.session.put(key_expr, payload).await {
                    error!("Session Put Failed ! {}", err)
                };
            }
            None
        }
        // DataMsg::Get { key_expr, id } => {
        //     let mut state_reader = state_map.write().await;
        //     if let Some(state) = state_reader.get_mut(&sock_addr) {
        //         if let Err(err) = state.session.get(key_expr, payload).await {
        //             error!("Session Put Failed ! {}", err)
        //         };
        //     }
        //     None
        // },
        DataMsg::Delete { key_expr } => {
            let mut state_reader = state_map.write().await;
            if let Some(state) = state_reader.get_mut(&sock_addr) {
                if let Err(err) = state.session.delete(key_expr).await {
                    error!("Session Delete Failed ! {}", err)
                };
            }
            None
        }
        DataMsg::Queryable(queryable_msg) => match queryable_msg {
            QueryableMsg::Query {
                queryable_uuid: _,
                query: _,
            } => {
                warn!("Plugin should not receive Query from Client");
                None
            }
            QueryableMsg::Reply { reply } => {
                let mut state_reader = state_map.write().await;
                if let Some(state) = state_reader.get_mut(&sock_addr) {
                    let query: Option<Query>;

                    match state.unanswered_queries.write() {
                        Ok(mut wr) => {
                            query = wr.remove(&reply.query_uuid);
                        }
                        Err(err) => {
                            tracing::error!("unanswered Queries RwLock Poinsened {err}");
                            return None;
                        }
                    }

                    if let Some(q) = query {
                        match reply.result {
                            Ok(ok) => {
                                if let Err(err) = q.reply(q.key_expr(), ok.value).await {
                                    tracing::error!("Query Could not Send Reply {}", err);
                                };
                            }
                            Err(err) => {
                                if let Err(err) = q.reply_err(err.payload).await {
                                    tracing::error!("Query Could not Send Reply {}", err);
                                };
                            }
                        }
                    } else {
                        tracing::error!("Query id not found in map {}", reply.query_uuid);
                    };
                }
                None
            }
        },
        DataMsg::Sample(sample, publisher_uuid) => {
            warn!("Server has Should not recieved A Sample from client");
            None
        }
    }
}
