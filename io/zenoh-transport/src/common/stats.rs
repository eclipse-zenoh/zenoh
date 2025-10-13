//
// Copyright (c) 2023 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//
macro_rules! stats_struct {
    (@field_type ) => {AtomicUsize};
    (@field_type $field_type:ident) => {std::sync::Arc<$field_type>};
    (@report_field_type ) => {usize};
    (@report_field_type $field_type:ident) => {paste::paste! {[<$field_type Report>]}};
    (@new($parent:expr, $id:expr) ) => {AtomicUsize::new(0)};
    (@new($parent:expr, $id:expr) $field_type:ident) => {$field_type::new($parent, $id)};
    (@report_default ) => {0};
    (@report_default $field_type:ident) => {paste::paste! {[<$field_type Report>]::default()}};
    (@get $vis:vis $field_name:ident) => {
        paste::paste! {
            $vis fn [<get_ $field_name>](&self) -> usize {
                self.$field_name.load(Ordering::Relaxed)
            }
        }
    };
    (@get $vis:vis $field_name:ident $field_type:ident) => {
        paste::paste! {
            $vis fn [<get_ $field_name>](&self) -> [<$field_type Report>] {
                self.$field_name.report()
            }
        }
    };
    (@increment $vis:vis $field_name:ident) => {
        paste::paste! {
            $vis fn [<inc_ $field_name>](&self, nb: usize) {
                self.$field_name.fetch_add(nb, Ordering::Relaxed);
                if let Some(parent) = self.parent.as_ref().and_then(|p| p.upgrade()) {
                    parent.[<inc_ $field_name>](nb);
                }
            }
        }
    };
    (@increment $vis:vis $field_name:ident $field_type:ident) => {};
    (@openmetrics($stats:expr, $string:expr) $field_name:ident) => {
        $string.push_str(stringify!($field_name));
        $string.push_str(" ");
        $string.push_str($stats.$field_name.to_string().as_str());
        $string.push_str("\n");
    };
    (@openmetrics_labels($stats:expr, $string:expr, $labels:expr) $field_name:ident) => {
        if (!$stats.labels.is_empty()) {
            $string.push_str(stringify!($field_name));
            $string.push_str("{");
            for (k, v) in &$stats.labels {
                $string.push_str(k);
                $string.push_str("=\"");
                $string.push_str(v);
                $string.push_str("\",")
            }
            $string.pop();
            $string.push_str("} ");
            $string.push_str($stats.$field_name.to_string().as_str());
            $string.push_str("\n");
        }
    };

    (@openmetrics($stats:expr, $string:expr) $field_name:ident $field_type:ident) => {
        $string.push_str(&$stats.$field_name.discriminated_openmetrics_text(stringify!($field_name), $field_type::DISCRIMINANT));
    };
    (@openmetrics_labels($stats:expr, $string:expr, $labels:expr) $field_name:ident $field_type:ident) => {
        if (!$stats.labels.is_empty()) {
            $string.push_str(&$stats.$field_name.labelled_discriminated_openmetrics_text(stringify!($field_name), $field_type::DISCRIMINANT, &$stats.labels));
        }
    };

    (@openmetrics_val($stats:expr) $field_name:ident) => {
        $stats.$field_name.to_string().as_str()
    };
    (@openmetrics_val($stats:expr) $field_name:ident $field_type:ident) => {""};
    (
     $(#[$meta:meta])*
     $vis:vis struct $struct_name:ident {
        $(# DISCRIMINANT $discriminant:literal)?
        $(
            $(# HELP $help:literal)?
            $(# TYPE $type:literal)?
            $(#[$field_meta:meta])*
            $field_vis:vis $field_name:ident $($field_type:ident)?,
        )*
     }
    ) => {
        paste::paste! {
            $vis struct $struct_name {
                labels: std::collections::HashMap<String, String>,
                parent: Option<std::sync::Weak<$struct_name>>,
                children: std::sync::Arc<std::sync::Mutex<std::vec::Vec<std::sync::Arc<$struct_name>>>>,
                $(
                $(#[$field_meta])*
                $field_vis $field_name: stats_struct!(@field_type $($field_type)?),
                )*
            }

            $(#[$meta])*
            $vis struct [<$struct_name Report>] {
                #[serde(skip)]
                labels: std::collections::HashMap<String, String>,
                #[serde(skip)]
                children: std::vec::Vec<[<$struct_name Report>]>,
                $(
                $(#[$field_meta])*
                $field_vis $field_name: stats_struct!(@report_field_type $($field_type)?),
                )*
            }

            impl $struct_name {
                $(const DISCRIMINANT: &str = $discriminant;)?
                $vis fn new(parent: Option<std::sync::Weak<$struct_name>>, labels: std::collections::HashMap<String, String>) -> std::sync::Arc<Self> {
                    let s = $struct_name {
                        labels: labels.clone(),
                        parent: parent.clone(),
                        $($field_name: stats_struct!(@new(parent.as_ref().and_then(|p| p.upgrade()).map(|p| std::sync::Arc::downgrade(&p.$field_name)), labels.clone()) $($field_type)?),)*
                        ..Default::default()
                    };
                    let a = std::sync::Arc::new(s);
                    match parent.and_then(|p| p.upgrade()) {
                        Some(p) => p.children.lock().unwrap().push(a.clone()),
                        None => {}
                    };
                    a
                }

                $vis fn parent(&self) -> &Option<std::sync::Weak<$struct_name>> {
                    &self.parent
                }

                $vis fn report(&self) -> [<$struct_name Report>] {
                    let report = [<$struct_name Report>] {
                        labels: self.labels.clone(),
                        children: self.children.lock().unwrap().iter().map(|c| c.report()).collect(),
                        $($field_name: self.[<get_ $field_name>](),)*
                    };
                    // remove already dropped children
                    self.children.lock().unwrap().retain(|c| std::sync::Arc::strong_count(c) > 1);

                    report
                }

                $(
                    stats_struct!(@get $vis $field_name $($field_type)?);
                    stats_struct!(@increment $vis $field_name $($field_type)?);
                )*
            }

            impl Default for $struct_name {
                fn default() -> Self {
                    Self {
                        labels: std::collections::HashMap::default(),
                        parent: None,
                        children: std::sync::Arc::new(std::sync::Mutex::new(std::vec::Vec::new())),
                        $($field_name: stats_struct!(@new(None, std::collections::HashMap::default()) $($field_type)?),)*
                    }
                }
            }

            impl [<$struct_name Report>] {
                #[allow(dead_code)]
                fn discriminated_openmetrics_text(&self, prefix: &str, disc: &str) -> String {
                    let mut s = String::new();
                    $(
                        s.push_str(prefix);
                        s.push_str("{");
                        s.push_str(disc);
                        s.push_str("=\"");
                        s.push_str(stringify!($field_name));
                        s.push_str("\"} ");
                        s.push_str(
                            stats_struct!(@openmetrics_val(self) $field_name $($field_type)?)
                        );
                        s.push_str("\n");
                    )*
                    s
                }

                #[allow(dead_code)]
                fn labelled_discriminated_openmetrics_text(&self, prefix: &str, disc: &str, labels: &std::collections::HashMap<String, String>) -> String {
                    let mut s = String::new();
                    $(
                        s.push_str(prefix);
                        s.push_str("{");
                        s.push_str(disc);
                        s.push_str("=\"");
                        s.push_str(stringify!($field_name));
                        for (k, v) in labels {
                            s.push_str("\",");
                            s.push_str(k);
                            s.push_str("=\"");
                            s.push_str(v);
                        }
                        s.push_str("\"} ");
                        s.push_str(
                            stats_struct!(@openmetrics_val(self) $field_name $($field_type)?)
                        );
                        s.push_str("\n");
                    )*
                    s
                }

                $vis fn openmetrics_text(&self) -> String {
                    let mut s = String::new();
                    $(
                        $(
                            s.push_str("# HELP ");
                            s.push_str(stringify!($field_name));
                            s.push_str(" ");
                            s.push_str($help);
                            s.push_str("\n");
                        )?
                        $(
                            s.push_str("# TYPE ");
                            s.push_str(stringify!($field_name));
                            s.push_str(" ");
                            s.push_str($type);
                            s.push_str("\n");
                        )?
                        stats_struct!(@openmetrics(self, s) $field_name $($field_type)?);
                        for c in &self.children {
                            stats_struct!(@openmetrics_labels(c, s, c.labels) $field_name $($field_type)?)
                        }
                    )*
                    s
                }
            }

            impl Default for [<$struct_name Report>] {
                fn default() -> Self {
                    Self {
                        labels: std::collections::HashMap::default(),
                        children: std::vec::Vec::default(),
                        $($field_name: stats_struct!(@report_default $($field_type)?),)*
                    }
                }
            }
        }
    }
}

use std::sync::atomic::{AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};
stats_struct! {
    #[derive(Clone, Debug, Deserialize, Serialize)]
    pub struct AdminStats {
        # DISCRIMINANT "space"
        pub user,
        pub admin,
    }
}

stats_struct! {
    #[derive(Clone, Debug, Deserialize, Serialize)]
    pub struct SHMStats {
        # DISCRIMINANT "medium"
        pub net,
        pub shm,
    }
}

stats_struct! {
    #[derive(Clone, Debug, Deserialize, Serialize)]
    pub struct TransportStats {
        # HELP "Counter of sent bytes."
        # TYPE "counter"
        pub tx_bytes,

        # HELP "Counter of sent transport messages."
        # TYPE "counter"
        pub tx_t_msgs,

        # HELP "Counter of sent network messages."
        # TYPE "counter"
        pub tx_n_msgs SHMStats,

        # HELP "Counter of dropped network messages."
        # TYPE "counter"
        pub tx_n_dropped,

        # HELP "Counter of sent zenoh put messages."
        # TYPE "counter"
        pub tx_z_put_msgs AdminStats,

        # HELP "Counter of sent bytes in zenoh put message payloads."
        # TYPE "counter"
        pub tx_z_put_pl_bytes AdminStats,

        # HELP "Counter of sent zenoh del messages."
        # TYPE "counter"
        pub tx_z_del_msgs AdminStats,

         # HELP "Counter of received bytes in zenoh del message attachments."
        # TYPE "counter"
        pub tx_z_del_pl_bytes AdminStats,

        # HELP "Counter of sent zenoh query messages."
        # TYPE "counter"
        pub tx_z_query_msgs AdminStats,

        # HELP "Counter of sent bytes in zenoh query message payloads."
        # TYPE "counter"
        pub tx_z_query_pl_bytes AdminStats,

        # HELP "Counter of sent zenoh reply messages."
        # TYPE "counter"
        pub tx_z_reply_msgs AdminStats,

        # HELP "Counter of sent bytes in zenoh reply message payloads."
        # TYPE "counter"
        pub tx_z_reply_pl_bytes AdminStats,

        # HELP "Counter of received bytes."
        # TYPE "counter"
        pub rx_bytes,

        # HELP "Counter of received transport messages."
        # TYPE "counter"
        pub rx_t_msgs,

        # HELP "Counter of received network messages."
        # TYPE "counter"
        pub rx_n_msgs SHMStats,

        # HELP "Counter of received zenoh put messages."
        # TYPE "counter"
        pub rx_z_put_msgs AdminStats,

        # HELP "Counter of received bytes in zenoh put message payloads."
        # TYPE "counter"
        pub rx_z_put_pl_bytes AdminStats,

        # HELP "Counter of received zenoh del messages."
        # TYPE "counter"
        pub rx_z_del_msgs AdminStats,

        # HELP "Counter of received bytes in zenoh del message attachments."
        # TYPE "counter"
        pub rx_z_del_pl_bytes AdminStats,

        # HELP "Counter of received zenoh query messages."
        # TYPE "counter"
        pub rx_z_query_msgs AdminStats,

        # HELP "Counter of received bytes in zenoh query message payloads."
        # TYPE "counter"
        pub rx_z_query_pl_bytes AdminStats,

        # HELP "Counter of received zenoh reply messages."
        # TYPE "counter"
        pub rx_z_reply_msgs AdminStats,

        # HELP "Counter of received bytes in zenoh reply message payloads."
        # TYPE "counter"
        pub rx_z_reply_pl_bytes AdminStats,

        # HELP "Counter of messages dropped by ingress downsampling."
        # TYPE "counter"
        pub rx_downsampler_dropped_msgs,

        # HELP "Counter of messages dropped by egress downsampling."
        # TYPE "counter"
        pub tx_downsampler_dropped_msgs,

        # HELP "Counter of bytes dropped by ingress low-pass filter."
        # TYPE "counter"
        pub rx_low_pass_dropped_bytes,

        # HELP "Counter of bytes dropped by egress low-pass filter."
        # TYPE "counter"
        pub tx_low_pass_dropped_bytes,

        # HELP "Counter of messages dropped by ingress low-pass filter."
        # TYPE "counter"
        pub rx_low_pass_dropped_msgs,

        # HELP "Counter of messages dropped by egress low-pass filter."
        # TYPE "counter"
        pub tx_low_pass_dropped_msgs,
    }
}
