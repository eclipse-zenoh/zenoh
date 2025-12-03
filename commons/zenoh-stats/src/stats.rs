pub(crate) trait JsonExt {
    fn has_field(&self, field: &str) -> bool;
    fn get_field(&mut self, field: &str) -> &mut Self;
    fn incr_counter(&mut self, field: &str, by: u64);
}
impl JsonExt for serde_json::Value {
    fn has_field(&self, field: &str) -> bool {
        self.as_object()
            .expect("json should be an object")
            .contains_key(field)
    }

    fn get_field(&mut self, field: &str) -> &mut Self {
        self.as_object_mut()
            .expect("json should be an object")
            .get_mut(field)
            .expect("field should exist")
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

pub(crate) fn init_stats(json: &mut serde_json::Value) {
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
        .insert("stats".into(), transport_stats.clone());
    for transport in json
        .get_field("sessions")
        .as_array_mut()
        .expect("sessions should be an array")
    {
        transport
            .as_object_mut()
            .expect("json should be an object")
            .insert("stats".into(), transport_stats.clone());
        for link in transport
            .get_field("links")
            .as_array_mut()
            .expect("links should be array")
        {
            link.as_object_mut()
                .expect("json should be an object")
                .insert("stats".into(), link_stats.clone());
        }
    }
}
