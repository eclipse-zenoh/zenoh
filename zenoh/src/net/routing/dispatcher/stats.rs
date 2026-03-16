use std::marker::PhantomData;

use zenoh_protocol::{
    core::Priority,
    network::{Push, Request, Response},
    zenoh::{PushBody, RequestBody, ResponseBody},
};
use zenoh_stats::{MessageLabel, SpaceLabel, StatsDirection, StatsKeys};

use crate::net::routing::dispatcher::{
    face::FaceState,
    tables::{RoutingExpr, Tables},
};

#[cfg(feature = "shared-memory")]
fn is_shm_payload(payload: &zenoh_buffers::ZBuf) -> bool {
    payload
        .zslices()
        .next()
        .and_then(|s| s.downcast_ref::<zenoh_shm::ShmBufInner>())
        .is_some()
}

pub(super) trait ObservableMessage {
    fn message(&self) -> MessageLabel;
    fn priority(&self) -> Priority;
    fn payload_size(&self) -> usize;
    fn tx_shm(&self) -> bool {
        false
    }
    fn rx_shm(&self) -> bool {
        false
    }
}

impl ObservableMessage for Push {
    fn message(&self) -> MessageLabel {
        match self.payload {
            PushBody::Put(_) => MessageLabel::Put,
            PushBody::Del(_) => MessageLabel::Del,
        }
    }
    fn priority(&self) -> Priority {
        self.ext_qos.get_priority()
    }
    fn payload_size(&self) -> usize {
        self.payload_size()
    }
    #[cfg(feature = "shared-memory")]
    fn tx_shm(&self) -> bool {
        match &self.payload {
            PushBody::Put(put) => put.ext_shm.is_some(),
            PushBody::Del(_) => false,
        }
    }
    #[cfg(feature = "shared-memory")]
    fn rx_shm(&self) -> bool {
        match &self.payload {
            PushBody::Put(put) => is_shm_payload(&put.payload),
            PushBody::Del(_) => false,
        }
    }
}

impl ObservableMessage for Request {
    fn message(&self) -> MessageLabel {
        match self.payload {
            RequestBody::Query(_) => MessageLabel::Query,
        }
    }
    fn priority(&self) -> Priority {
        self.ext_qos.get_priority()
    }
    fn payload_size(&self) -> usize {
        self.payload_size()
    }
    #[cfg(feature = "shared-memory")]
    fn tx_shm(&self) -> bool {
        match &self.payload {
            RequestBody::Query(query) => {
                query.ext_body.as_ref().is_some_and(|b| b.ext_shm.is_some())
            }
        }
    }
    #[cfg(feature = "shared-memory")]
    fn rx_shm(&self) -> bool {
        match &self.payload {
            RequestBody::Query(query) => query
                .ext_body
                .as_ref()
                .is_some_and(|b| is_shm_payload(&b.payload)),
        }
    }
}

impl ObservableMessage for Response {
    fn message(&self) -> MessageLabel {
        match self.payload {
            ResponseBody::Reply(_) => MessageLabel::Reply,
            ResponseBody::Err(_) => MessageLabel::ReplyErr,
        }
    }
    fn priority(&self) -> Priority {
        self.ext_qos.get_priority()
    }
    fn payload_size(&self) -> usize {
        self.payload_size()
    }
    #[cfg(feature = "shared-memory")]
    fn tx_shm(&self) -> bool {
        match &self.payload {
            ResponseBody::Reply(reply) => match &reply.payload {
                PushBody::Put(put) => put.ext_shm.is_some(),
                PushBody::Del(_) => false,
            },
            ResponseBody::Err(err) => err.ext_shm.is_some(),
        }
    }
    #[cfg(feature = "shared-memory")]
    fn rx_shm(&self) -> bool {
        match &self.payload {
            ResponseBody::Reply(reply) => match &reply.payload {
                PushBody::Put(put) => is_shm_payload(&put.payload),
                PushBody::Del(_) => false,
            },
            ResponseBody::Err(err) => is_shm_payload(&err.payload),
        }
    }
}

pub(super) struct PayloadObserver<Msg> {
    message: MessageLabel,
    priority: Priority,
    payload_size: usize,
    space: SpaceLabel,
    keys: StatsKeys,
    _phantom: PhantomData<Msg>,
}

impl<Msg: ObservableMessage> PayloadObserver<Msg> {
    pub(super) fn new(msg: &Msg, expr: Option<&RoutingExpr>, tables: &Tables) -> Self {
        let space = if expr.is_some_and(|e| e.is_admin()) {
            SpaceLabel::Admin
        } else {
            SpaceLabel::User
        };
        // SAFETY: the tree is always the table's one
        let keys = expr.map_or_else(Default::default, |expr| unsafe {
            tables.stats_keys.get_keys(
                || Some(&expr.resource()?.context.as_ref()?.stats_keys),
                || expr.key_expr(),
            )
        });
        Self {
            message: msg.message(),
            priority: msg.priority(),
            payload_size: msg.payload_size(),
            space,
            // SAFETY: the tree is always the table's one
            keys,
            _phantom: PhantomData,
        }
    }

    pub(super) fn observe_payload(&self, direction: StatsDirection, face: &FaceState, msg: &Msg) {
        if let Some(stats) = face.stats.as_ref() {
            let shm = match direction {
                StatsDirection::Tx => msg.tx_shm(),
                StatsDirection::Rx => msg.rx_shm(),
            };
            stats.observe_network_message_payload(
                direction,
                self.message,
                self.priority,
                self.payload_size,
                self.space,
                &self.keys,
                shm,
            )
        }
    }
}
