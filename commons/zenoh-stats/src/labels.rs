use std::{
    borrow::Cow,
    fmt,
    fmt::{Error, Write},
    ops::Deref,
};

use prometheus_client::encoding::{
    EncodeLabelSet, EncodeLabelValue, LabelSetEncoder, LabelValueEncoder,
};
use zenoh_protocol::{
    core::{Locator, Priority, WhatAmI, ZenohIdProto},
    network::NetworkBodyRef,
    zenoh::{PushBody, ResponseBody},
};

// Because of `prometheus_client` lack of blanket impl
pub(crate) struct LabelsSetRef<'a, S>(pub(crate) &'a S);
impl<S: EncodeLabelSet> EncodeLabelSet for LabelsSetRef<'_, S> {
    fn encode(&self, encoder: &mut LabelSetEncoder) -> Result<(), Error> {
        self.0.encode(encoder)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ResourceLabel {
    Subscriber,
    Queryable,
    Token,
}

impl EncodeLabelValue for ResourceLabel {
    fn encode(&self, encoder: &mut LabelValueEncoder) -> fmt::Result {
        encoder.write_str(match self {
            Self::Subscriber => "subscriber",
            Self::Queryable => "queryable",
            Self::Token => "token",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LocalityLabel {
    Local,
    Remote,
}

impl EncodeLabelValue for LocalityLabel {
    fn encode(&self, encoder: &mut LabelValueEncoder) -> fmt::Result {
        encoder.write_str(match self {
            Self::Local => "local",
            Self::Remote => "remote",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageLabel {
    Put,
    Del,
    Query,
    Reply,
    ReplyErr,
    ResponseFinal,
    Interest,
    Declare,
    Oam,
}

impl MessageLabel {
    pub(crate) const NUM: usize = 9;
}

impl EncodeLabelValue for MessageLabel {
    fn encode(&self, encoder: &mut LabelValueEncoder) -> fmt::Result {
        encoder.write_str(match self {
            Self::Put => "put",
            Self::Del => "delete",
            Self::Query => "query",
            Self::Reply => "reply",
            Self::ReplyErr => "reply-err",
            Self::ResponseFinal => "response-final",
            Self::Interest => "interest",
            Self::Declare => "declare",
            Self::Oam => "oam",
        })
    }
}

impl From<NetworkBodyRef<'_>> for MessageLabel {
    fn from(value: NetworkBodyRef<'_>) -> Self {
        match value {
            NetworkBodyRef::Push(push) => match push.payload {
                PushBody::Put(_) => MessageLabel::Put,
                PushBody::Del(_) => MessageLabel::Del,
            },
            NetworkBodyRef::Request(_) => MessageLabel::Query,
            NetworkBodyRef::Response(res) => match res.payload {
                ResponseBody::Reply(_) => MessageLabel::Reply,
                ResponseBody::Err(_) => MessageLabel::ReplyErr,
            },
            NetworkBodyRef::ResponseFinal(_) => MessageLabel::ResponseFinal,
            NetworkBodyRef::Interest(_) => MessageLabel::Interest,
            NetworkBodyRef::Declare(_) => MessageLabel::Declare,
            NetworkBodyRef::OAM(_) => MessageLabel::Oam,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpaceLabel {
    User,
    Admin,
}

impl SpaceLabel {
    pub(crate) const NUM: usize = 2;
}

impl EncodeLabelValue for SpaceLabel {
    fn encode(&self, encoder: &mut LabelValueEncoder) -> fmt::Result {
        encoder.write_str(match self {
            Self::User => "user",
            Self::Admin => "admin",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReasonLabel {
    AccessControl,
    Congestion,
    Downsampling,
    LowPass,
    NoLink,
}

impl EncodeLabelValue for ReasonLabel {
    fn encode(&self, encoder: &mut LabelValueEncoder) -> fmt::Result {
        encoder.write_str(match self {
            Self::AccessControl => "access-control",
            Self::Congestion => "congestion",
            Self::Downsampling => "downsampling",
            Self::LowPass => "low-pass",
            Self::NoLink => "no-link",
        })
    }
}

pub(crate) const SHM_NUM: usize = 2;

macro_rules! wrap_label {
    ($ty:ty, $label:ident $(, $derive:ty)*) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, $($derive),*)]
        pub(crate) struct $label(pub(crate) $ty);

        impl EncodeLabelValue for $label {
            fn encode(&self, encoder: &mut LabelValueEncoder) -> fmt::Result {
                write!(encoder, "{}", self.0)
            }
        }

        impl<T: Into<$ty>> From<T> for $label {
            fn from(value: T) -> Self {
                Self(value.into())
            }
        }

        impl Deref for $label {
            type Target = $ty;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }
    };
}

wrap_label!(ZenohIdProto, ZidLabel, Copy);
wrap_label!(WhatAmI, WhatAmILabel, Copy);
wrap_label!(Priority, PriorityLabel, Copy);
wrap_label!(Locator, LocatorLabel);

pub(crate) type ProtocolLabel = Cow<'static, str>;

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct TransportLabels {
    pub(crate) remote_zid: Option<ZidLabel>,
    pub(crate) remote_whatami: Option<WhatAmILabel>,
    pub(crate) remote_group: Option<String>,
    pub(crate) remote_cn: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct ProtocolLabels {
    pub(crate) protocol: ProtocolLabel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct DisconnectedLabels {
    pub(crate) disconnected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct LinkLabels {
    pub(crate) src_locator: LocatorLabel,
    pub(crate) dst_locator: LocatorLabel,
}

impl LinkLabels {
    pub(crate) fn protocol(&self) -> ProtocolLabel {
        static KNOWN_PROTOCOLS: &[&str] = &[
            "tcp",
            "udp",
            "tls",
            "quic",
            "unixsock-stream",
            "ws",
            "serial",
            "unixpipe",
            "vsock",
        ];
        let protocol = self.dst_locator.protocol().as_str();
        match KNOWN_PROTOCOLS.iter().find(|p| **p == protocol) {
            Some(p) => Cow::Borrowed(*p),
            None => Cow::Owned(self.dst_locator.to_string()),
        }
    }
}

impl From<(&Locator, &Locator)> for LinkLabels {
    fn from(value: (&Locator, &Locator)) -> Self {
        Self {
            src_locator: value.0.clone().into(),
            dst_locator: value.1.clone().into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct ResourceDeclaredLabels {
    pub(crate) resource: ResourceLabel,
    pub(crate) locality: LocalityLabel,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct BytesLabels {
    pub(crate) protocol: ProtocolLabel,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct TransportMessageLabels {
    pub(crate) protocol: ProtocolLabel,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct NetworkMessageLabels {
    pub(crate) priority: PriorityLabel,
    pub(crate) message: MessageLabel,
    pub(crate) shm: bool,
    pub(crate) protocol: ProtocolLabel,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct NetworkMessagePayloadLabels {
    pub(crate) space: SpaceLabel,
    pub(crate) priority: PriorityLabel,
    pub(crate) message: MessageLabel,
    pub(crate) shm: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct NetworkMessageDroppedPayloadLabels {
    pub(crate) priority: PriorityLabel,
    pub(crate) message: MessageLabel,
    pub(crate) protocol: Option<ProtocolLabel>,
    pub(crate) reason: ReasonLabel,
}
