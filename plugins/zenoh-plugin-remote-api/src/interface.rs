use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use zenoh::{
    key_expr::OwnedKeyExpr,
    query::{Query, Reply, ReplyError},
    sample::{Sample, SampleKind},
};

// ██████  ███████ ███    ███  ██████  ████████ ███████      █████  ██████  ██     ███    ███ ███████ ███████ ███████  █████   ██████  ███████
// ██   ██ ██      ████  ████ ██    ██    ██    ██          ██   ██ ██   ██ ██     ████  ████ ██      ██      ██      ██   ██ ██       ██
// ██████  █████   ██ ████ ██ ██    ██    ██    █████       ███████ ██████  ██     ██ ████ ██ █████   ███████ ███████ ███████ ██   ███ █████
// ██   ██ ██      ██  ██  ██ ██    ██    ██    ██          ██   ██ ██      ██     ██  ██  ██ ██           ██      ██ ██   ██ ██    ██ ██
// ██   ██ ███████ ██      ██  ██████     ██    ███████     ██   ██ ██      ██     ██      ██ ███████ ███████ ███████ ██   ██  ██████  ███████

#[derive(TS)]
#[ts(export)]
#[derive(Debug, Serialize, Deserialize)]
pub enum RemoteAPIMsg {
    Data(DataMsg),
    Control(ControlMsg),
}

#[derive(TS)]
#[ts(export)]
#[derive(Debug, Serialize, Deserialize)]
pub enum DataMsg {
    // Client -> SVR
    PublisherPut(Vec<u8>, Uuid),
    // SVR -> Client
    // Subscriber
    Sample(SampleWS, Uuid),
    // GetReply
    GetReply(ReplyWS),
    // Bidirectional
    Queryable(QueryableMsg),
}

#[derive(TS)]
#[ts(export)]
#[derive(Debug, Serialize, Deserialize)]
pub enum QueryableMsg {
    // SVR -> Client
    // UUID of original queryable
    Query {
        queryable_uuid: Uuid,
        query: QueryWS,
    },
    // Client -> SVR
    Reply {
        reply: ReplyWS,
    },
}

//  ██████  ██████  ███    ██ ████████ ██████   ██████  ██          ███    ███ ███████ ███████ ███████  █████   ██████  ███████
// ██      ██    ██ ████   ██    ██    ██   ██ ██    ██ ██          ████  ████ ██      ██      ██      ██   ██ ██       ██
// ██      ██    ██ ██ ██  ██    ██    ██████  ██    ██ ██          ██ ████ ██ █████   ███████ ███████ ███████ ██   ███ █████
// ██      ██    ██ ██  ██ ██    ██    ██   ██ ██    ██ ██          ██  ██  ██ ██           ██      ██ ██   ██ ██    ██ ██
//  ██████  ██████  ██   ████    ██    ██   ██  ██████  ███████     ██      ██ ███████ ███████ ███████ ██   ██  ██████  ███████

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum ControlMsg {
    // Session
    OpenSession,
    CloseSession,
    Session(Uuid),

    // Session Action Messages
    // TODO Replace parameters String with Parameters
    Get {
        #[ts(as = "OwnedKeyExprWrapper")]
        key_expr: OwnedKeyExpr,
        parameters: Option<String>,
        id: Uuid,
    },
    GetFinished {
        id: Uuid,
    },
    Put {
        #[ts(as = "OwnedKeyExprWrapper")]
        key_expr: OwnedKeyExpr,
        payload: Vec<u8>,
    },
    Delete {
        #[ts(as = "OwnedKeyExprWrapper")]
        key_expr: OwnedKeyExpr,
    },
    //

    // Subscriber
    DeclareSubscriber {
        #[ts(as = "OwnedKeyExprWrapper")]
        key_expr: OwnedKeyExpr,
        id: Uuid,
    },
    Subscriber(Uuid),
    UndeclareSubscriber(Uuid),

    // Publisher
    DeclarePublisher {
        #[ts(as = "OwnedKeyExprWrapper")]
        key_expr: OwnedKeyExpr,
        id: Uuid,
    },
    UndeclarePublisher(Uuid),

    // Queryable
    DeclareQueryable {
        #[ts(as = "OwnedKeyExprWrapper")]
        key_expr: OwnedKeyExpr,
        complete: bool,
        id: Uuid,
    },
    UndeclareQueryable(Uuid),
}

// ██     ██ ██████   █████  ██████  ██████  ███████ ██████  ███████
// ██     ██ ██   ██ ██   ██ ██   ██ ██   ██ ██      ██   ██ ██
// ██  █  ██ ██████  ███████ ██████  ██████  █████   ██████  ███████
// ██ ███ ██ ██   ██ ██   ██ ██      ██      ██      ██   ██      ██
//  ███ ███  ██   ██ ██   ██ ██      ██      ███████ ██   ██ ███████

// Wrapper to get OwnerKeyExpr to play with TS
#[derive(Debug, Deserialize, TS)]
#[serde(from = "String")]
pub struct OwnedKeyExprWrapper(Arc<str>);

impl From<String> for OwnedKeyExprWrapper {
    fn from(s: String) -> Self {
        OwnedKeyExprWrapper(s.into())
    }
}

#[derive(TS)]
#[ts(export)]
#[derive(Debug, Serialize, Deserialize)]
pub struct QueryWS {
    query_uuid: Uuid,
    #[ts(as = "OwnedKeyExprWrapper")]
    key_expr: OwnedKeyExpr,
    parameters: String,
    encoding: Option<String>,
    attachment: Option<Vec<u8>>,
    payload: Option<Vec<u8>>,
}

impl From<(&Query, Uuid)> for QueryWS {
    fn from((q, uuid): (&Query, Uuid)) -> Self {
        let payload: Option<Vec<u8>> = match q.payload().map(|x| Vec::<u8>::from(x)) {
            Some(x) => Some(x),
            None => None,
        };
        let attachment: Option<Vec<u8>> = match q.attachment().map(|x| Vec::<u8>::from(x)) {
            Some(x) => Some(x),
            None => None,
        };
        QueryWS {
            query_uuid: uuid,
            key_expr: q.key_expr().to_owned().into(),
            parameters: q.parameters().to_string(),
            encoding: q.encoding().map(|x| x.to_string()),
            attachment,
            payload,
        }
    }
}

#[derive(TS)]
#[ts(export)]
#[derive(Debug, Serialize, Deserialize)]
pub struct ReplyWS {
    pub query_uuid: Uuid,
    pub result: Result<SampleWS, ReplyErrorWS>,
}

impl From<(Reply, Uuid)> for ReplyWS {
    fn from((reply, uuid): (Reply, Uuid)) -> Self {
        match reply.result() {
            Ok(sample) => {
                let sample_ws = SampleWS::from(sample);
                ReplyWS {
                    query_uuid: uuid,
                    result: Ok(sample_ws),
                }
            }
            Err(err) => {
                let error_ws = ReplyErrorWS::from(err);

                ReplyWS {
                    query_uuid: uuid,
                    result: Err(error_ws),
                }
            }
        }
    }
}

#[derive(TS)]
#[ts(export)]
#[derive(Debug, Serialize, Deserialize)]
pub struct ReplyErrorWS {
    pub(crate) payload: Vec<u8>,
    pub(crate) encoding: String,
}

impl From<ReplyError> for ReplyErrorWS {
    fn from(r_e: ReplyError) -> Self {
        let z_bytes: Vec<u8> = Vec::<u8>::from(r_e.payload());

        ReplyErrorWS {
            payload: z_bytes,
            encoding: r_e.encoding().to_string(),
        }
    }
}

impl From<&ReplyError> for ReplyErrorWS {
    fn from(r_e: &ReplyError) -> Self {
        let z_bytes: Vec<u8> = Vec::<u8>::from(r_e.payload());

        ReplyErrorWS {
            payload: z_bytes,
            encoding: r_e.encoding().to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SampleWS {
    #[ts(as = "OwnedKeyExprWrapper")]
    key_expr: OwnedKeyExpr,
    pub(crate) value: Vec<u8>,
    pub(crate) kind: SampleKindWS,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
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

impl From<&Sample> for SampleWS {
    fn from(s: &Sample) -> Self {
        let z_bytes: Vec<u8> = Vec::<u8>::from(s.payload());

        SampleWS {
            key_expr: s.key_expr().to_owned().into(),
            value: z_bytes,
            kind: s.kind().into(),
        }
    }
}

impl From<Sample> for SampleWS {
    fn from(s: Sample) -> Self {
        let z_bytes: Vec<u8> = Vec::<u8>::from(s.payload());

        SampleWS {
            key_expr: s.key_expr().to_owned().into(),
            value: z_bytes,
            kind: s.kind().into(),
        }
    }
}

#[cfg(test)]
mod tests {

    use std::str::FromStr;

    use uuid::Uuid;
    use zenoh::key_expr::KeyExpr;

    use super::*;

    #[test]
    fn serialize_messages() {
        let json: String =
            serde_json::to_string(&RemoteAPIMsg::Control(ControlMsg::OpenSession)).unwrap();
        assert_eq!(json, r#"{"Control":"OpenSession"}"#);

        let uuid = Uuid::from_str("a2663bb1-128c-4dd3-a42b-d1d3337e2e51").unwrap();

        let json: String =
            serde_json::to_string(&RemoteAPIMsg::Control(ControlMsg::Session(uuid))).unwrap();
        assert_eq!(
            json,
            r#"{"Control":{"Session":"a2663bb1-128c-4dd3-a42b-d1d3337e2e51"}}"#
        );

        let json: String =
            serde_json::to_string(&RemoteAPIMsg::Control(ControlMsg::CloseSession)).unwrap();
        assert_eq!(json, r#"{"Control":"CloseSession"}"#);

        let key_expr: OwnedKeyExpr = KeyExpr::new("demo/test").unwrap().to_owned().into();

        let sample_ws = SampleWS {
            key_expr: key_expr.clone(),
            value: vec![1, 2, 3],
            kind: SampleKindWS::Put,
        };

        let json: String = serde_json::to_string(&QueryableMsg::Reply {
            reply: ReplyWS {
                query_uuid: uuid,
                result: Ok(sample_ws),
            },
        })
        .unwrap();
        assert_eq!(json, r#"{"Reply":{}}"#);

        let sample_ws = SampleWS {
            key_expr,
            value: vec![1, 2, 3],
            kind: SampleKindWS::Put,
        };
        let json: String = serde_json::to_string(&DataMsg::Sample(sample_ws, uuid)).unwrap();
        assert_eq!(
            json,
            r#"{"Sample":[{"key_expr":"demo/test","value":[1,2,3],"kind":"Put"},"a2663bb1-128c-4dd3-a42b-d1d3337e2e51"]}"#
        );
    }
}
