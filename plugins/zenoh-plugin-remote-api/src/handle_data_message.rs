use std::{error::Error, net::SocketAddr};

use tracing::{error, warn};
use zenoh::query::Query;

use crate::{
    interface::{DataMsg, QueryableMsg},
    StateMap,
};

pub async fn handle_data_message(
    data_msg: DataMsg,
    sock_addr: SocketAddr,
    state_map: StateMap,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Access State Structure
    let state_writer = state_map.read().await;
    let state_map = match state_writer.get(&sock_addr) {
        Some(x) => x,
        None => {
            tracing::warn!("State Map Does not contain SocketAddr");
            return Ok(());
        }
    };

    // Data Message
    match data_msg {
        DataMsg::PublisherPut(payload, publisher_uuid) => {
            if let Some(publisher) = state_map.publishers.get(&publisher_uuid) {
                if let Err(err) = publisher.put(payload).await {
                    error!("PublisherPut {publisher_uuid}, {err}");
                }
            } else {
                warn!("Publisher {publisher_uuid}, does not exist in State");
            }
        }
        DataMsg::Queryable(queryable_msg) => match queryable_msg {
            QueryableMsg::Reply { reply } => {
                let query: Option<Query> = match state_map.unanswered_queries.write() {
                    Ok(mut wr) => wr.remove(&reply.query_uuid),
                    Err(err) => {
                        tracing::error!("unanswered Queries RwLock Poisened {err}");
                        return Ok(());
                    }
                };

                if let Some(q) = query {
                    match reply.result {
                        Ok(ok) => q.reply(q.key_expr(), ok.value).await?,
                        Err(err) => q.reply_err(err.payload).await?,
                    }
                } else {
                    tracing::error!("Query id not found in map {}", reply.query_uuid);
                };
            }
            QueryableMsg::Query {
                queryable_uuid: _,
                query: _,
            } => {
                warn!("Plugin should not receive Query from Client");
            }
        },
        data_msg => {
            error!("Server Should not recieved a {data_msg:?} Variant from client");
        }
    }
    Ok(())
}
