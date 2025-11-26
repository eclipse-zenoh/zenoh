use std::{
    borrow::Cow,
    fmt,
    fmt::{Error, Write},
    ops::Deref,
};

use prometheus_client::{
    encoding::{EncodeLabelSet, EncodeLabelValue, LabelSetEncoder, LabelValueEncoder},
    metrics::counter::Counter,
};
use zenoh_protocol::{
    core::{Locator, Priority, Protocol, WhatAmI, ZenohIdProto},
    network::NetworkBodyRef,
    zenoh::{PushBody, ResponseBody},
};

use crate::{
    family::TransportMetric, histogram::Histogram, stats::JsonExt, Rx, StatsDirection, Tx,
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
pub(crate) enum MessageLabel {
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
pub(crate) enum SpaceLabel {
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

pub(crate) fn match_protocol(protocol: Protocol) -> ProtocolLabel {
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
    match KNOWN_PROTOCOLS.iter().find(|p| **p == protocol.as_str()) {
        Some(p) => Cow::Borrowed(*p),
        None => Cow::Owned(protocol.as_str().into()),
    }
}

pub(crate) trait StatsPath<M: TransportMetric> {
    fn incr_stats(
        direction: StatsDirection,
        transport: Option<&TransportLabels>,
        link: Option<&LinkLabels>,
        labels: &Self,
        collected: M::Collected,
        json: &mut serde_json::Value,
    );

    fn incr_counters(
        transport: Option<&TransportLabels>,
        link: Option<&LinkLabels>,
        json: &mut serde_json::Value,
        incr_stats_counter: impl Fn(&mut serde_json::Value),
    ) {
        if let Some(transport) = transport {
            for json in json
                .get_field("sessions")
                .as_array_mut()
                .expect("sessions should be an array")
            {
                if transport
                    .remote_zid
                    .is_some_and(|peer| json.get_field("peer") == &peer.0.to_string())
                    || transport.remote_zid.is_none()
                {
                    incr_stats_counter(json.get_field("stats"));
                    if let Some(link) = link {
                        for json in json
                            .get_field("links")
                            .as_array_mut()
                            .expect("links should be an array")
                        {
                            if json.get_field("src") == link.src_locator.as_str()
                                && json.get_field("dst") == link.dst_locator.as_str()
                            {
                                incr_stats_counter(json.get_field("stats"));
                            }
                        }
                    }
                    if transport.remote_zid.is_some() {
                        break;
                    }
                }
            }
        }
        incr_stats_counter(json.get_field("stats"));
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct TransportLabels {
    pub(crate) remote_zid: Option<ZidLabel>,
    pub(crate) remote_whatami: Option<WhatAmILabel>,
    pub(crate) remote_group: Option<String>,
    pub(crate) remote_cn: Option<String>,
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
    pub(crate) protocol: Cow<'static, str>,
}

impl StatsPath<Counter> for BytesLabels {
    fn incr_stats(
        direction: StatsDirection,
        transport: Option<&TransportLabels>,
        link: Option<&LinkLabels>,
        _labels: &Self,
        collected: <Counter as TransportMetric>::Collected,
        json: &mut serde_json::Value,
    ) {
        let counter = match direction {
            Tx => "tx_bytes",
            Rx => "rx_bytes",
        };
        Self::incr_counters(transport, link, json, |stats| {
            stats.incr_counter(counter, collected)
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct TransportMessageLabels {
    pub(crate) protocol: Cow<'static, str>,
}

impl StatsPath<Counter> for TransportMessageLabels {
    fn incr_stats(
        direction: StatsDirection,
        transport: Option<&TransportLabels>,
        link: Option<&LinkLabels>,
        _labels: &Self,
        collected: <Counter as TransportMetric>::Collected,
        json: &mut serde_json::Value,
    ) {
        let counter = match direction {
            Tx => "tx_t_msgs",
            Rx => "rx_t_msgs",
        };
        Self::incr_counters(transport, link, json, |stats| {
            stats.incr_counter(counter, collected)
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct NetworkMessageLabels {
    pub(crate) priority: PriorityLabel,
    pub(crate) message: MessageLabel,
    pub(crate) shm: bool,
    pub(crate) protocol: Cow<'static, str>,
}

impl StatsPath<Counter> for NetworkMessageLabels {
    fn incr_stats(
        direction: StatsDirection,
        transport: Option<&TransportLabels>,
        link: Option<&LinkLabels>,
        labels: &Self,
        collected: <Counter as TransportMetric>::Collected,
        json: &mut serde_json::Value,
    ) {
        let counter = match direction {
            Tx => "tx_n_msgs",
            Rx => "rx_n_msgs",
        };
        let medium = if labels.shm { "shm" } else { "net" };
        Self::incr_counters(transport, link, json, |stats| {
            stats.get_field(counter).incr_counter(medium, collected)
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct NetworkMessagePayloadLabels {
    pub(crate) space: SpaceLabel,
    pub(crate) priority: PriorityLabel,
    pub(crate) message: MessageLabel,
    pub(crate) shm: bool,
    pub(crate) protocol: Cow<'static, str>,
}

impl StatsPath<Histogram> for NetworkMessagePayloadLabels {
    fn incr_stats(
        direction: StatsDirection,
        transport: Option<&TransportLabels>,
        link: Option<&LinkLabels>,
        labels: &Self,
        collected: <Histogram as TransportMetric>::Collected,
        json: &mut serde_json::Value,
    ) {
        let (sum, count, _) = collected;
        let msg = match &labels.message {
            MessageLabel::Put => "put",
            MessageLabel::Del => "del",
            MessageLabel::Query => "query",
            MessageLabel::Reply | MessageLabel::ReplyErr => "reply",
            _ => unreachable!(),
        };
        let msgs = format!("{direction}_z_{msg}_msgs");
        let pl_bytes = format!("{direction}_z_{msg}_pl_bytes");
        Self::incr_counters(transport, link, json, |stats| {
            let space = match labels.space {
                SpaceLabel::Admin => "admin",
                SpaceLabel::User => "user",
            };
            if stats.has_field(&msgs) {
                stats.get_field(&msgs).incr_counter(space, count);
                stats.get_field(&pl_bytes).incr_counter(space, sum as u64);
            }
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct NetworkMessageDroppedPayloadLabels {
    pub(crate) priority: PriorityLabel,
    pub(crate) message: MessageLabel,
    pub(crate) protocol: Option<String>,
    pub(crate) reason: ReasonLabel,
}

impl StatsPath<Histogram> for NetworkMessageDroppedPayloadLabels {
    fn incr_stats(
        direction: StatsDirection,
        transport: Option<&TransportLabels>,
        link: Option<&LinkLabels>,
        labels: &Self,
        collected: <Histogram as TransportMetric>::Collected,
        json: &mut serde_json::Value,
    ) {
        let (sum, count, _) = collected;
        let mut incr_counters = |field, by| {
            Self::incr_counters(transport, link, json, |stats| stats.incr_counter(field, by))
        };
        match (direction, &labels.reason) {
            (Tx, ReasonLabel::Congestion) => {
                incr_counters("tx_n_dropped", count);
            }
            (Tx, ReasonLabel::Downsampling) => {
                incr_counters("tx_downsampler_dropped_msgs", count);
            }
            (Rx, ReasonLabel::Downsampling) => {
                incr_counters("rx_downsampler_dropped_msgs", count);
            }
            (Tx, ReasonLabel::LowPass) => {
                incr_counters("tx_low_pass_dropped_msgs", count);
                incr_counters("tx_low_pass_dropped_bytes", sum as u64);
            }
            (Rx, ReasonLabel::LowPass) => {
                incr_counters("rx_low_pass_dropped_msgs", count);
                incr_counters("rx_low_pass_dropped_bytes", sum as u64);
            }
            _ => {}
        }
    }
}
