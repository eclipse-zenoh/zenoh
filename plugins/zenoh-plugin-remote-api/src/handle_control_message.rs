use std::{error::Error, net::SocketAddr};

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
) -> Result<Option<ControlMsg>, Box<dyn Error + Send + Sync>> {
    // Access State Structure
    let mut state_writer = state_map.write().await;
    let state_map = match state_writer.get_mut(&sock_addr) {
        Some(state_map) => state_map,
        None => {
            tracing::warn!("State Map Does not contain SocketAddr");
            return Ok(None);
        }
    };

    // Handle Control Message
    match ctrl_msg {
        ControlMsg::OpenSession => {
            return Ok(Some(ControlMsg::Session(state_map.session_id)));
        }
        ControlMsg::CloseSession => {
            if let Some(state_map) = state_writer.remove(&sock_addr) {
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
            } else {
                warn!("State Map Does not contain SocketAddr");
            }
        }
        //
        // ControlMsg::CreateKeyExpr(key_expr_str) => {
        //     let key_expr = KeyExpr::new(key_expr_str).unwrap();
        //     remote_state.key_expr.insert(key_expr.clone());
        //     Some(ControlMsg::KeyExpr(key_expr.to_string()))
        // }
        //
        ControlMsg::Get {
            key_expr,
            parameters,
            id,
        } => {
            let selector = Selector::owned(key_expr, parameters.unwrap_or_default());
            let receiver: flume::Receiver<zenoh::query::Reply> =
                state_map.session.get(selector).await?;
            let mut receiving = true;
            while receiving {
                match receiver.recv_async().await {
                    Ok(reply) => {
                        let reply_ws = ReplyWS::from((reply, id));
                        let remote_api_msg = RemoteAPIMsg::Data(DataMsg::GetReply(reply_ws));
                        if let Err(err) = state_map.websocket_tx.send(remote_api_msg) {
                            tracing::error!("{}", err);
                        }
                    }
                    Err(_) => receiving = false,
                }
            }

            let remote_api_msg = RemoteAPIMsg::Control(ControlMsg::GetFinished { id });
            state_map.websocket_tx.send(remote_api_msg)?;
        }
        ControlMsg::Put { key_expr, payload } => state_map.session.put(key_expr, payload).await?,
        ControlMsg::Delete { key_expr } => state_map.session.delete(key_expr).await?,
        // SUBSCRIBER
        ControlMsg::DeclareSubscriber {
            key_expr: key_expr_str,
            id: subscriber_uuid,
        } => {
            let key_expr = KeyExpr::new(key_expr_str).unwrap();
            let ch_tx = state_map.websocket_tx.clone();
            let res_subscriber = state_map
                .session
                .declare_subscriber(key_expr)
                .callback(move |sample| {
                    let sample_ws = SampleWS::from(sample);
                    let remote_api_message =
                        RemoteAPIMsg::Data(DataMsg::Sample(sample_ws, subscriber_uuid));
                    if let Err(e) = ch_tx.send(remote_api_message) {
                        error!("Forward Sample Channel error: {e}");
                    };
                })
                .await;

            state_map
                .subscribers
                .insert(subscriber_uuid, res_subscriber?);

            return Ok(Some(ControlMsg::Subscriber(subscriber_uuid)));
        }
        ControlMsg::UndeclareSubscriber(uuid) => {
            if let Some(subscriber) = state_map.subscribers.remove(&uuid) {
                subscriber.undeclare().await?
            } else {
                warn!("UndeclareSubscriber: No Subscriber with UUID {uuid}");
            }
        }
        // Publisher
        ControlMsg::DeclarePublisher { key_expr, id: uuid } => {
            let publisher = state_map.session.declare_publisher(key_expr).await?;
            state_map.publishers.insert(uuid, publisher);
        }
        ControlMsg::UndeclarePublisher(uuid) => {
            if let Some(publisher) = state_map.publishers.remove(&uuid) {
                publisher.undeclare().await?;
            } else {
                warn!("UndeclarePublisher: No Publisher with UUID {uuid}");
            }
        }
        // Backend should not receive this, make it unrepresentable
        ControlMsg::DeclareQueryable {
            key_expr,
            complete,
            id: queryable_uuid,
        } => {
            let unanswered_queries = state_map.unanswered_queries.clone();
            let session = state_map.session.clone();
            let ch_tx = state_map.websocket_tx.clone();

            let queryable = session
                .declare_queryable(&key_expr)
                .complete(complete)
                .callback(move |query| {
                    let query_uuid = Uuid::new_v4();
                    let queryable_msg = QueryableMsg::Query {
                        queryable_uuid,
                        query: QueryWS::from((&query, query_uuid)),
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
                .await?;

            state_map.queryables.insert(queryable_uuid, queryable);
        }
        ControlMsg::UndeclareQueryable(uuid) => {
            if let Some(queryable) = state_map.queryables.remove(&uuid) {
                queryable.undeclare().await?;
            };
        }
        msg @ (ControlMsg::GetFinished { id: _ }
        | ControlMsg::Session(_)
        | ControlMsg::Subscriber(_)) => {
            // make server recieving these types unrepresentable
            error!("Backend should not recieve this message Type: {msg:?}");
        }
    };
    Ok(None)
}
