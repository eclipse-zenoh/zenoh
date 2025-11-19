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

pub(crate) fn init_stats(json: &mut serde_json::Value) {
    let transport_stats = serde_json::json!({
        "rx_bytes": 0,
        "rx_downsampler_dropped_msgs": 0,
        "rx_low_pass_dropped_bytes": 0,
        "rx_low_pass_dropped_msgs": 0,
        "rx_n_msgs": {
            "net": 0,
            "shm": 0
        },
        "rx_t_msgs": 0,
        "rx_z_del_msgs": {
            "admin": 0,
            "user": 0
        },
        "rx_z_del_pl_bytes": {
            "admin": 0,
            "user": 0
        },
        "rx_z_put_msgs": {
            "admin": 0,
            "user": 0
        },
        "rx_z_put_pl_bytes": {
            "admin": 0,
            "user": 0
        },
        "rx_z_query_msgs": {
            "admin": 0,
            "user": 0
        },
        "rx_z_query_pl_bytes": {
            "admin": 0,
            "user": 0
        },
        "rx_z_reply_msgs": {
            "admin": 0,
            "user": 0
        },
        "rx_z_reply_pl_bytes": {
            "admin": 0,
            "user": 0
        },
        "tx_bytes": 0,
        "tx_downsampler_dropped_msgs": 0,
        "tx_low_pass_dropped_bytes": 0,
        "tx_low_pass_dropped_msgs": 0,
        "tx_n_dropped": 0,
        "tx_n_msgs": {
            "net": 0,
            "shm": 0
        },
        "tx_t_msgs": 0,
        "tx_z_del_msgs": {
            "admin": 0,
            "user": 0
        },
        "tx_z_del_pl_bytes": {
            "admin": 0,
            "user": 0
        },
        "tx_z_put_msgs": {
            "admin": 0,
            "user": 0
        },
        "tx_z_put_pl_bytes": {
            "admin": 0,
            "user": 0
        },
        "tx_z_query_msgs": {
            "admin": 0,
            "user": 0
        },
        "tx_z_query_pl_bytes": {
            "admin": 0,
            "user": 0
        },
        "tx_z_reply_msgs": {
            "admin": 0,
            "user": 0
        },
        "tx_z_reply_pl_bytes": {
            "admin": 0,
            "user": 0
        }
    });
    let link_stats = serde_json::json!({
        "rx_bytes": 0,
        "rx_n_msgs": {
            "net": 0,
            "shm": 0
        },
        "rx_t_msgs": 0,
        "tx_bytes": 0,
        "tx_n_dropped": 0,
        "tx_n_msgs": {
            "net": 0,
            "shm": 0
        },
        "tx_t_msgs": 0,
    });
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
