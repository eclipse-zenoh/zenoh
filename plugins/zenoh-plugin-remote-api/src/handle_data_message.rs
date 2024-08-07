use std::{error::Error, net::SocketAddr};

use tracing::{error, warn};
use zenoh::{bytes::EncodingBuilderTrait, query::Query, sample::SampleBuilderTrait};

use crate::{
    interface::{DataMsg, QueryReplyVariant, QueryableMsg},
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
        DataMsg::PublisherPut {
            id,
            payload,
            attachment,
            encoding,
        } => {
            if let Some(publisher) = state_map.publishers.get(&id) {
                let mut put_builder = publisher.put(payload);
                if let Some(payload) = attachment {
                    put_builder = put_builder.attachment(payload);
                }
                if let Some(encoding) = encoding {
                    put_builder = put_builder.encoding(encoding);
                }
                if let Err(err) = put_builder.await {
                    error!("PublisherPut {id}, {err}");
                }
            } else {
                warn!("Publisher {id}, does not exist in State");
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
                        QueryReplyVariant::Reply { key_expr, payload } => {
                            q.reply(key_expr, payload).await?
                        }
                        QueryReplyVariant::ReplyErr { payload } => q.reply_err(payload).await?,
                        QueryReplyVariant::ReplyDelete { key_expr } => {
                            q.reply_del(key_expr).await?
                        }
                    }
                } else {
                    tracing::error!("Query id not found in map {}", reply.query_uuid);
                };
            }
            QueryableMsg::Query {
                queryable_uuid: _,
                query: _,
            } => {
                warn!("Plugin should not receive Query from Client, This should go via Get API");
            }
        },
        data_msg => {
            error!("Server Should not recieved a {data_msg:?} Variant from client");
        }
    }
    Ok(())
}
