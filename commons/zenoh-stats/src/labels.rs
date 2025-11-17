use std::{fmt, fmt::Write};

use prometheus_client::{
    encoding::{EncodeLabelSet, EncodeLabelValue, LabelValueEncoder},
    metrics::counter::Counter,
};
use zenoh_link_commons::LinkId;
use zenoh_protocol::{
    core::{Priority, WhatAmI, ZenohIdProto},
    network::NetworkBodyRef,
    zenoh::{PushBody, ResponseBody},
};

use crate::{
    histogram::Histogram, per_remote::PerRemoteMetric, stats::JsonExt, Rx, StatsDirection, Tx,
};

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
    ($label:ident, $ty:ty) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    };
}

wrap_label!(ZidLabel, ZenohIdProto);
wrap_label!(WhatAmILabel, WhatAmI);
wrap_label!(PriorityLabel, Priority);

pub(crate) trait StatsPath<M: PerRemoteMetric> {
    fn incr_stats(
        direction: StatsDirection,
        remote: Option<&RemoteLabels>,
        labels: &Self,
        link_id: Option<LinkId>,
        collected: M::Collected,
        json: &mut serde_json::Value,
    );

    fn incr_counters(
        remote: Option<&RemoteLabels>,
        link_id: Option<LinkId>,
        json: &mut serde_json::Value,
        incr_stats_counter: impl Fn(&mut serde_json::Value),
    ) {
        if let Some(remote) = remote {
            let incr_session_counters = |json: &mut serde_json::Value| {
                incr_stats_counter(json.get_field("stats"));
                if let Some(link) = link_id {
                    if let Some(json) = json.get_field("links").get_item("id", &link.to_string()) {
                        incr_stats_counter(json.get_field("stats"));
                    }
                }
            };
            let sessions = json.get_field("sessions");
            match remote.remote_zid {
                Some(peer) => {
                    if let Some(item) = sessions.get_item("peer", &peer.0.to_string()) {
                        incr_session_counters(item)
                    }
                }
                None => {
                    for item in sessions.as_array_mut().expect("json should be an array") {
                        incr_session_counters(item)
                    }
                }
            }
        }
        incr_stats_counter(json.get_field("stats"));
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct RemoteLabels {
    pub(crate) remote_zid: Option<ZidLabel>,
    pub(crate) remote_whatami: Option<WhatAmILabel>,
    pub(crate) remote_group: Option<String>,
    pub(crate) remote_cn: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct ResourceDeclaredLabels {
    pub(crate) resource: ResourceLabel,
    pub(crate) locality: LocalityLabel,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct BytesLabels {
    pub(crate) protocol: String,
}

impl StatsPath<Counter> for BytesLabels {
    fn incr_stats(
        direction: StatsDirection,
        remote: Option<&RemoteLabels>,
        _labels: &Self,
        link_id: Option<LinkId>,
        collected: <Counter as PerRemoteMetric>::Collected,
        json: &mut serde_json::Value,
    ) {
        let counter = match direction {
            Tx => "tx_bytes",
            Rx => "rx_bytes",
        };
        Self::incr_counters(remote, link_id, json, |stats| {
            stats.incr_counter(counter, collected)
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct TransportMessageLabels {
    pub(crate) protocol: String,
}

impl StatsPath<Counter> for TransportMessageLabels {
    fn incr_stats(
        direction: StatsDirection,
        remote: Option<&RemoteLabels>,
        _labels: &Self,
        link_id: Option<LinkId>,
        collected: <Counter as PerRemoteMetric>::Collected,
        json: &mut serde_json::Value,
    ) {
        let counter = match direction {
            Tx => "tx_t_msgs",
            Rx => "rx_t_msgs",
        };
        Self::incr_counters(remote, link_id, json, |stats| {
            stats.incr_counter(counter, collected)
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct NetworkMessageLabels {
    pub(crate) priority: PriorityLabel,
    pub(crate) message: MessageLabel,
    pub(crate) shm: bool,
    pub(crate) protocol: String,
}

impl StatsPath<Counter> for NetworkMessageLabels {
    fn incr_stats(
        direction: StatsDirection,
        remote: Option<&RemoteLabels>,
        labels: &Self,
        link_id: Option<LinkId>,
        collected: <Counter as PerRemoteMetric>::Collected,
        json: &mut serde_json::Value,
    ) {
        let counter = match direction {
            Tx => "tx_n_msgs",
            Rx => "rx_n_msgs",
        };
        let medium = if labels.shm { "shm" } else { "net" };
        Self::incr_counters(remote, link_id, json, |stats| {
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
    pub(crate) protocol: String,
}

impl StatsPath<Histogram> for NetworkMessagePayloadLabels {
    fn incr_stats(
        direction: StatsDirection,
        remote: Option<&RemoteLabels>,
        labels: &Self,
        link_id: Option<LinkId>,
        collected: <Histogram as PerRemoteMetric>::Collected,
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
        Self::incr_counters(remote, link_id, json, |stats| {
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
        remote: Option<&RemoteLabels>,
        labels: &Self,
        link_id: Option<LinkId>,
        collected: <Histogram as PerRemoteMetric>::Collected,
        json: &mut serde_json::Value,
    ) {
        let (sum, count, _) = collected;
        let mut incr_counters = |field, by| {
            Self::incr_counters(remote, link_id, json, |stats| stats.incr_counter(field, by))
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
