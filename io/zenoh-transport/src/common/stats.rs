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
    (@new($parent:expr) ) => {AtomicUsize::new(0)};
    (@new($parent:expr) $field_type:ident) => {std::sync::Arc::new($field_type::new($parent))};
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
                if let Some(parent) = self.parent.as_ref() {
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
    (@openmetrics($stats:expr, $string:expr) $field_name:ident $field_type:ident) => {
        $string.push_str(&$stats.$field_name.sub_openmetrics_text(stringify!($field_name)));
    };
    (@openmetrics_val($stats:expr) $field_name:ident) => {
        $stats.$field_name.to_string().as_str()
    };
    (@openmetrics_val($stats:expr) $field_name:ident $field_type:ident) => {""};
    (
     $(#[$meta:meta])*
     $vis:vis struct $struct_name:ident {

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
                parent: Option<std::sync::Arc<$struct_name>>,
                $(
                $(#[$field_meta])*
                $field_vis $field_name: stats_struct!(@field_type $($field_type)?),
                )*
            }

            $(#[$meta])*
            $vis struct [<$struct_name Report>] {
                $(
                $(#[$field_meta])*
                $field_vis $field_name: stats_struct!(@report_field_type $($field_type)?),
                )*
            }

            impl $struct_name {
                $vis fn new(parent: Option<std::sync::Arc<$struct_name>>) -> Self {
                    $struct_name {
                        parent: parent.clone(),
                        $($field_name: stats_struct!(@new(parent.as_ref().map(|p|p.$field_name.clone())) $($field_type)?),)*
                    }
                }

                $vis fn report(&self) -> [<$struct_name Report>] {
                    [<$struct_name Report>] {
                        $($field_name: self.[<get_ $field_name>](),)*
                    }
                }

                $(
                    stats_struct!(@get $vis $field_name $($field_type)?);
                    stats_struct!(@increment $vis $field_name $($field_type)?);
                )*
            }

            impl Default for $struct_name {
                fn default() -> Self {
                    Self {
                        parent: None,
                        $($field_name: stats_struct!(@new(None) $($field_type)?),)*
                    }
                }
            }

            impl [<$struct_name Report>] {
                #[allow(dead_code)]
                fn sub_openmetrics_text(&self, prefix: &str) -> String {
                    let mut s = String::new();
                    $(
                        s.push_str(prefix);
                        s.push_str("{space=\"");
                        s.push_str(stringify!($field_name));
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
                    )*
                    s
                }
            }

            impl Default for [<$struct_name Report>] {
                fn default() -> Self {
                    Self {
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
    pub struct DiscriminatedStats {
        pub user,
        pub admin,
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
        pub tx_n_msgs,

        # HELP "Counter of dropped network messages."
        # TYPE "counter"
        pub tx_n_dropped,

        # HELP "Counter of sent zenoh put messages."
        # TYPE "counter"
        pub tx_z_put_msgs DiscriminatedStats,

        # HELP "Counter of sent bytes in zenoh put message payloads."
        # TYPE "counter"
        pub tx_z_put_pl_bytes DiscriminatedStats,

        # HELP "Counter of sent zenoh del messages."
        # TYPE "counter"
        pub tx_z_del_msgs DiscriminatedStats,

         # HELP "Counter of received bytes in zenoh del message attachments."
        # TYPE "counter"
        pub tx_z_del_pl_bytes DiscriminatedStats,

        # HELP "Counter of sent zenoh query messages."
        # TYPE "counter"
        pub tx_z_query_msgs DiscriminatedStats,

        # HELP "Counter of sent bytes in zenoh query message payloads."
        # TYPE "counter"
        pub tx_z_query_pl_bytes DiscriminatedStats,

        # HELP "Counter of sent zenoh reply messages."
        # TYPE "counter"
        pub tx_z_reply_msgs DiscriminatedStats,

        # HELP "Counter of sent bytes in zenoh reply message payloads."
        # TYPE "counter"
        pub tx_z_reply_pl_bytes DiscriminatedStats,

        # HELP "Counter of received bytes."
        # TYPE "counter"
        pub rx_bytes,

        # HELP "Counter of received transport messages."
        # TYPE "counter"
        pub rx_t_msgs,

        # HELP "Counter of received network messages."
        # TYPE "counter"
        pub rx_n_msgs,

        # HELP "Counter of dropped network messages."
        # TYPE "counter"
        pub rx_n_dropped,

        # HELP "Counter of received zenoh put messages."
        # TYPE "counter"
        pub rx_z_put_msgs DiscriminatedStats,

        # HELP "Counter of received bytes in zenoh put message payloads."
        # TYPE "counter"
        pub rx_z_put_pl_bytes DiscriminatedStats,

        # HELP "Counter of received zenoh del messages."
        # TYPE "counter"
        pub rx_z_del_msgs DiscriminatedStats,

        # HELP "Counter of received bytes in zenoh del message attachments."
        # TYPE "counter"
        pub rx_z_del_pl_bytes DiscriminatedStats,

        # HELP "Counter of received zenoh query messages."
        # TYPE "counter"
        pub rx_z_query_msgs DiscriminatedStats,

        # HELP "Counter of received bytes in zenoh query message payloads."
        # TYPE "counter"
        pub rx_z_query_pl_bytes DiscriminatedStats,

        # HELP "Counter of received zenoh reply messages."
        # TYPE "counter"
        pub rx_z_reply_msgs DiscriminatedStats,

        # HELP "Counter of received bytes in zenoh reply message payloads."
        # TYPE "counter"
        pub rx_z_reply_pl_bytes DiscriminatedStats,
    }
}
