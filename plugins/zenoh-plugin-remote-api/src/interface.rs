use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ts_rs::TS;
use uuid::Uuid;
use zenoh::{
    key_expr::OwnedKeyExpr,
    qos::{CongestionControl, Priority},
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
    PublisherPut {
        id: Uuid,
        payload: Vec<u8>,
        attachment: Option<Vec<u8>>,
        encoding: Option<String>,
    },
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
        reply: QueryReplyWS,
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
        encoding: String,
        #[serde(
            deserialize_with = "deserialize_congestion_control",
            serialize_with = "serialize_congestion_control"
        )]
        #[ts(type = "number")]
        congestion_control: CongestionControl,
        #[serde(
            deserialize_with = "deserialize_priority",
            serialize_with = "serialize_priority"
        )]
        #[ts(type = "number")]
        priority: Priority,
        express: bool,
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

fn deserialize_congestion_control<'de, D>(d: D) -> Result<CongestionControl, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match u8::deserialize(d)? {
        0u8 => CongestionControl::Drop,
        1u8 => CongestionControl::Block,
        val => {
            return Err(serde::de::Error::custom(format!(
                "Value not valid for CongestionControl Enum {:?}",
                val
            )))
        }
    })
}

fn serialize_congestion_control<S>(
    congestion_control: &CongestionControl,
    s: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_u8(*congestion_control as u8)
}

fn deserialize_priority<'de, D>(d: D) -> Result<Priority, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match u8::deserialize(d)? {
        1u8 => Priority::RealTime,
        2u8 => Priority::InteractiveHigh,
        3u8 => Priority::InteractiveLow,
        4u8 => Priority::DataHigh,
        5u8 => Priority::Data,
        6u8 => Priority::DataLow,
        7u8 => Priority::Background,
        val => {
            return Err(serde::de::Error::custom(format!(
                "Value not valid for Priority Enum {:?}",
                val
            )))
        }
    })
}

fn serialize_priority<S>(priority: &Priority, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_u8(*priority as u8)
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
        let payload: Option<Vec<u8>> = q.payload().map(Vec::<u8>::from);
        let attachment: Option<Vec<u8>> = q.attachment().map(Vec::<u8>::from);
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
pub struct QueryReplyWS {
    pub query_uuid: Uuid,
    pub result: QueryReplyVariant,
}

#[derive(TS)]
#[ts(export)]
#[derive(Debug, Serialize, Deserialize)]
pub enum QueryReplyVariant {
    Reply {
        #[ts(as = "OwnedKeyExprWrapper")]
        key_expr: OwnedKeyExpr,
        payload: Vec<u8>,
    },
    ReplyErr {
        payload: Vec<u8>,
    },
    ReplyDelete {
        #[ts(as = "OwnedKeyExprWrapper")]
        key_expr: OwnedKeyExpr,
    },
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
    pub(crate) key_expr: OwnedKeyExpr,
    pub(crate) value: Vec<u8>,
    pub(crate) kind: SampleKindWS,
    pub(crate) encoding: String,
    pub(crate) timestamp: Option<String>,
    pub(crate) congestion_control: u8,
    pub(crate) priority: u8,
    pub(crate) express: bool,
    pub(crate) attachement: Option<Vec<u8>>,
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
            timestamp: s.timestamp().map(|x| x.to_string()),
            priority: s.priority() as u8,
            congestion_control: s.congestion_control() as u8,
            encoding: s.encoding().to_string(),
            express: s.express(),
            attachement: s.attachment().map(|x| Vec::<u8>::from(x)),
        }
    }
}

impl From<Sample> for SampleWS {
    fn from(s: Sample) -> Self {
        SampleWS::from(&s)
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
            encoding: "zenoh/bytes".into(),
            timestamp: None,
            priority: 1,
            congestion_control: 1,
            express: false,
            attachement: None,
        };

        // let json: String = serde_json::to_string(&QueryableMsg::Reply {
        //     reply: QueryReplyWS {
        //         query_uuid: uuid,
        //     },
        // })
        // .unwrap();
        // assert_eq!(json, r#"{"Reply":{}}"#);

        let sample_ws = SampleWS {
            key_expr,
            value: vec![1, 2, 3],
            kind: SampleKindWS::Put,
            encoding: "zenoh/bytes".into(),
            timestamp: None,
            priority: 1,
            congestion_control: 1,
            express: false,
            attachement: None,
        };
        let json: String = serde_json::to_string(&DataMsg::Sample(sample_ws, uuid)).unwrap();
        // assert_eq!(
        //     json,
        //     r#"{"Sample":[{"key_expr":"demo/test","value":[1,2,3],"kind":"Put"},"a2663bb1-128c-4dd3-a42b-d1d3337e2e51"]}"#
        // );
    }
}
