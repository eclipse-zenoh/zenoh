use prometheus_client::metrics::counter::Counter;

use crate::{
    family::TransportMetric,
    histogram::Histogram,
    keys::HistogramPerKey,
    labels::{
        BytesLabels, LinkLabels, MessageLabel, NetworkMessageDroppedPayloadLabels,
        NetworkMessageLabels, NetworkMessagePayloadLabels, SpaceLabel, TransportLabels,
        TransportMessageLabels,
    },
    ReasonLabel, Rx, StatsDirection, Tx,
};

const STATS_KEY: &str = "stats";
const STATS_FILTERED_KEY: &str = "stats_filtered";

pub(crate) trait JsonExt {
    fn has_field(&self, field: &str) -> bool;
    fn try_get_field(&mut self, field: &str) -> Option<&mut Self>;
    fn get_field(&mut self, field: &str) -> &mut Self;
    fn get_item(&mut self, predicate: impl Fn(&mut Self) -> bool) -> Option<&mut Self>;
    fn incr_counter(&mut self, field: &str, by: u64);
}
impl JsonExt for serde_json::Value {
    fn has_field(&self, field: &str) -> bool {
        self.as_object()
            .expect("json should be an object")
            .contains_key(field)
    }

    fn try_get_field(&mut self, field: &str) -> Option<&mut Self> {
        self.as_object_mut()
            .expect("json should be an object")
            .get_mut(field)
    }

    fn get_field(&mut self, field: &str) -> &mut Self {
        self.try_get_field(field).expect("field should exist")
    }

    fn get_item(&mut self, predicate: impl Fn(&mut Self) -> bool) -> Option<&mut Self> {
        self.as_array_mut()
            .expect("json should be an array")
            .iter_mut()
            .find_map(|json| predicate(json).then_some(json))
    }

    fn incr_counter(&mut self, field: &str, by: u64) {
        let obj = self.as_object_mut().expect("json should be an object");
        let counter = obj.get_mut(field).expect("field should exist");
        let value = counter
            .as_number()
            .expect("counter should be a number")
            .as_u64()
            .expect("counter should be u64");
        *counter = (value + by).into()
    }
}

macro_rules! stats_default {
    ($($name:ident $($discriminant:ident)?),+ $(, ..$flatten:expr)* $(,)?) => {{
        let mut stats = serde_json::Map::new();
        $(stats_default!(@ $name $($discriminant)?, stats);)+
        $(stats.extend($flatten.as_object().unwrap().clone());)*
        serde_json::Value::Object(stats)
    }};
    (@ $name:ident, $stats:expr, $value:expr) => {
        $stats.insert(concat!("rx_", stringify!($name)).into(), $value);
        $stats.insert(concat!("tx_", stringify!($name)).into(), $value);
    };
    (@ $name:ident, $stats:expr) => {
        stats_default!(@ $name, $stats, serde_json::json!(0));
    };
    (@ $name:ident space, $stats:expr) => {
        stats_default!(@ $name, $stats, serde_json::json!({ "admin": 0, "user": 0 }));
    };
    (@ $name:ident medium, $stats:expr) => {
        stats_default!(@ $name, $stats, serde_json::json!({ "net": 0, "shm": 0 }));
    };
}

pub(crate) fn init_stats(json: &mut serde_json::Value, keys: &[String]) {
    let link_stats = stats_default!(bytes, t_msgs, n_msgs medium, n_dropped);
    let payload_stats = stats_default!(
        z_del_msgs space,
        z_del_pl_bytes space,
        z_put_msgs space,
        z_put_pl_bytes space,
        z_query_msgs space,
        z_query_pl_bytes space,
        z_reply_msgs space,
        z_reply_pl_bytes space,
    );
    let transport_stats = stats_default!(
        downsampler_dropped_msgs,
        low_pass_dropped_bytes,
        low_pass_dropped_msgs,
        ..payload_stats,
        ..link_stats,
    );
    json.as_object_mut()
        .expect("json should be an object")
        .insert(STATS_KEY.into(), transport_stats.clone());
    let stats_filtered: serde_json::Value = keys
        .iter()
        .map(|key| serde_json::json!({ "key": key, STATS_KEY: payload_stats.clone() }))
        .collect::<Vec<_>>()
        .into();
    if !keys.is_empty() {
        json.as_object_mut()
            .expect("json should be an object")
            .insert(STATS_FILTERED_KEY.into(), stats_filtered.clone());
    }
    for transport in json
        .get_field("sessions")
        .as_array_mut()
        .expect("sessions should be an array")
    {
        transport
            .as_object_mut()
            .expect("json should be an object")
            .insert(STATS_KEY.into(), transport_stats.clone());
        if !keys.is_empty() {
            transport
                .as_object_mut()
                .expect("json should be an object")
                .insert(STATS_FILTERED_KEY.into(), stats_filtered.clone());
        }
        for link in transport
            .get_field("links")
            .as_array_mut()
            .expect("links should be array")
        {
            link.as_object_mut()
                .expect("json should be an object")
                .insert(STATS_KEY.into(), link_stats.clone());
        }
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
        key: Option<&str>,
        json: &mut serde_json::Value,
        incr_stats_counter: impl Fn(&mut serde_json::Value),
    ) {
        let incr_counter = |json: &mut serde_json::Value| match key {
            Some(key) => {
                if let Some(entry) = (json.try_get_field(STATS_FILTERED_KEY))
                    .and_then(|f| f.get_item(|entry| entry.get_field("key") == key))
                {
                    incr_stats_counter(entry.get_field(STATS_KEY))
                }
            }
            None => incr_stats_counter(json.get_field(STATS_KEY)),
        };
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
                    incr_counter(json);
                    if let Some(link) = link {
                        if let Some(json) = json.get_field("links").get_item(|entry| {
                            entry.get_field("src") == link.src_locator.as_str()
                                && entry.get_field("dst") == link.dst_locator.as_str()
                        }) {
                            incr_counter(json);
                        }
                    }
                    if transport.remote_zid.is_some() {
                        break;
                    }
                }
            }
        }
        incr_counter(json);
    }
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
        Self::incr_counters(transport, link, None, json, |stats| {
            stats.incr_counter(counter, collected)
        });
    }
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
        Self::incr_counters(transport, link, None, json, |stats| {
            stats.incr_counter(counter, collected)
        });
    }
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
        Self::incr_counters(transport, link, None, json, |stats| {
            stats.get_field(counter).incr_counter(medium, collected)
        });
    }
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
        Self::incr_stats(None, direction, transport, link, labels, collected, json);
    }
}

impl StatsPath<HistogramPerKey> for NetworkMessagePayloadLabels {
    fn incr_stats(
        direction: StatsDirection,
        transport: Option<&TransportLabels>,
        link: Option<&LinkLabels>,
        labels: &Self,
        collected: <HistogramPerKey as TransportMetric>::Collected,
        json: &mut serde_json::Value,
    ) {
        for (key, collected) in collected {
            Self::incr_stats(
                Some(&key),
                direction,
                transport,
                link,
                labels,
                collected,
                json,
            );
        }
    }
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
            Self::incr_counters(transport, link, None, json, |stats| {
                stats.incr_counter(field, by)
            })
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

impl NetworkMessagePayloadLabels {
    fn incr_stats(
        key: Option<&str>,
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
        <Self as StatsPath<Histogram>>::incr_counters(transport, link, key, json, |stats| {
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
