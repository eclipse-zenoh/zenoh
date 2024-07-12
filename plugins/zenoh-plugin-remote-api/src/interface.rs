use std::convert::Infallible;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use zenoh::sample::{Sample, SampleKind};

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
    Sample(SampleWS, Uuid),
    PublisherPut(Vec<u8>, Uuid),
}

type KeyExpr = String;

#[derive(TS)]
#[ts(export)]
#[derive(Debug, Serialize, Deserialize)]
pub enum ControlMsg {
    // Session
    OpenSession,
    CloseSession,

    //
    Session(Uuid),

    // KeyExpr
    CreateKeyExpr(String), // TODO do i need this ?
    KeyExpr(String),       // TODO do i need this ?
    DeleteKeyExpr(String),

    // Subscriber
    DeclareSubscriber(KeyExpr, Uuid),
    Subscriber(Uuid),
    UndeclareSubscriber(Uuid),

    //Publisher
    DeclarePublisher(KeyExpr, Uuid),

    //
    Error(String),
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
enum ErrorMsg {}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SampleWS {
    // key_expr: KeyExpr<'static>,
    pub(crate) key_expr: String,
    pub(crate) value: Vec<u8>,
    pub(crate) kind: SampleKindWS,
    // timestamp: Option<String>,
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

impl TryFrom<Sample> for SampleWS {
    type Error = Infallible;

    fn try_from(s: Sample) -> Result<Self, Self::Error> {
        let z_bytes: Vec<u8> = s.payload().try_into()?;
        // let timestamp = s.timestamp().to;

        Ok(SampleWS {
            key_expr: s.key_expr().to_string(),
            value: z_bytes,
            kind: s.kind().into(),
        })
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

        let json: String = serde_json::to_string(&RemoteAPIMsg::Control(
            ControlMsg::CreateKeyExpr("demo/tech".into()),
        ))
        .unwrap();
        assert_eq!(json, r#"{"Control":{"CreateKeyExpr":"demo/tech"}}"#);

        let key_expr = KeyExpr::new("demo/test").unwrap();
        let json: String = serde_json::to_string(&RemoteAPIMsg::Control(ControlMsg::KeyExpr(
            key_expr.clone().to_string(),
        )))
        .unwrap();
        assert_eq!(json, r#"{"Control":{"KeyExpr":"demo/test"}}"#);

        let sampe_ws = SampleWS {
            key_expr: key_expr.to_string(),
            value: vec![1, 2, 3],
            kind: SampleKindWS::Put,
        };

        let json: String = serde_json::to_string(&DataMsg::Sample(sampe_ws, uuid)).unwrap();

        assert_eq!(
            json,
            r#"{"Sample":[{"key_expr":"demo/test","value":[1,2,3],"kind":"Put"},"a2663bb1-128c-4dd3-a42b-d1d3337e2e51"]}"#
        );
    }
}
