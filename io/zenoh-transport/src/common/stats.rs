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
    (
     $(#[$meta:meta])*
     $vis:vis struct $struct_name:ident {
        $(
            $(# HELP $field_help:literal)?
            $(# TYPE $field_type:literal)?
            $(#[$field_meta:meta])*
            $field_vis:vis $field_name:ident,
        )*
     }
    ) => {
        paste::paste! {
            $vis struct $struct_name {
                parent: Option<std::sync::Arc<$struct_name>>,
                $(
                $(#[$field_meta])*
                $field_vis $field_name: AtomicUsize,
                )*
            }

            $(#[$meta])*
            $vis struct [<$struct_name Report>] {
                $(
                $(#[$field_meta])*
                $field_vis $field_name: usize,
                )*
            }

            impl $struct_name {
                $vis fn new(parent: Option<std::sync::Arc<$struct_name>>) -> Self {
                    $struct_name {
                        parent,
                        $($field_name: AtomicUsize::new(0),)*
                    }
                }

                $vis fn report(&self) -> [<$struct_name Report>] {
                    [<$struct_name Report>] {
                        $($field_name: self.[<get_ $field_name>](),)*
                    }
                }

                $(
                    $vis fn [<get_ $field_name>](&self) -> usize {
                    self.$field_name.load(Ordering::Relaxed)
                }

                $vis fn [<inc_ $field_name>](&self, nb: usize) {
                    self.$field_name.fetch_add(nb, Ordering::Relaxed);
                    if let Some(parent) = self.parent.as_ref() {
                        parent.[<inc_ $field_name>](nb);
                    }
                }
                )*
            }

            impl Default for $struct_name {
                fn default() -> Self {
                    Self {
                        parent: None,
                        $($field_name: AtomicUsize::new(0),)*
                    }
                }
            }

            impl [<$struct_name Report>] {
                $vis fn openmetrics_text(&self) -> String {
                    let mut s = String::new();
                    $(
                        $(
                            s.push_str("# HELP ");
                            s.push_str(stringify!($field_name));
                            s.push_str(" ");
                            s.push_str($field_help);
                            s.push_str("\n");
                        )?
                        $(
                            s.push_str("# TYPE ");
                            s.push_str(stringify!($field_name));
                            s.push_str(" ");
                            s.push_str($field_type);
                            s.push_str("\n");
                        )?
                        s.push_str(stringify!($field_name));
                        s.push_str(" ");
                        s.push_str(self.$field_name.to_string().as_str());
                        s.push_str("\n");
                    )*
                    s
                }
            }

            impl Default for [<$struct_name Report>] {
                fn default() -> Self {
                    Self {
                        $($field_name: 0,)*
                    }
                }
            }
        }
    }
}

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
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
        pub tx_z_put_user_msgs,

        # HELP "Counter of sent bytes in zenoh put message payloads."
        # TYPE "counter"
        pub tx_z_put_user_pl_bytes,

        # HELP "Counter of sent zenoh put messages."
        # TYPE "counter"
        pub tx_z_put_admin_msgs,

        # HELP "Counter of sent bytes in zenoh put message payloads."
        # TYPE "counter"
        pub tx_z_put_admin_pl_bytes,

        # HELP "Counter of sent zenoh del messages."
        # TYPE "counter"
        pub tx_z_del_user_msgs,

        # HELP "Counter of sent zenoh del messages."
        # TYPE "counter"
        pub tx_z_del_admin_msgs,

        # HELP "Counter of sent zenoh query messages."
        # TYPE "counter"
        pub tx_z_query_user_msgs,

        # HELP "Counter of sent bytes in zenoh query message payloads."
        # TYPE "counter"
        pub tx_z_query_user_pl_bytes,

        # HELP "Counter of sent zenoh query messages."
        # TYPE "counter"
        pub tx_z_query_admin_msgs,

        # HELP "Counter of sent bytes in zenoh query message payloads."
        # TYPE "counter"
        pub tx_z_query_admin_pl_bytes,

        # HELP "Counter of sent zenoh reply messages."
        # TYPE "counter"
        pub tx_z_reply_user_msgs,

        # HELP "Counter of sent bytes in zenoh reply message payloads."
        # TYPE "counter"
        pub tx_z_reply_user_pl_bytes,

        # HELP "Counter of sent zenoh reply messages."
        # TYPE "counter"
        pub tx_z_reply_admin_msgs,

        # HELP "Counter of sent bytes in zenoh reply message payloads."
        # TYPE "counter"
        pub tx_z_reply_admin_pl_bytes,


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
        pub rx_z_put_user_msgs,

        # HELP "Counter of received bytes in zenoh put message payloads."
        # TYPE "counter"
        pub rx_z_put_user_pl_bytes,

        # HELP "Counter of received zenoh put messages."
        # TYPE "counter"
        pub rx_z_put_admin_msgs,

        # HELP "Counter of received bytes in zenoh put message payloads."
        # TYPE "counter"
        pub rx_z_put_admin_pl_bytes,

        # HELP "Counter of received zenoh del messages."
        # TYPE "counter"
        pub rx_z_del_user_msgs,

        # HELP "Counter of received zenoh del messages."
        # TYPE "counter"
        pub rx_z_del_admin_msgs,

        # HELP "Counter of received zenoh query messages."
        # TYPE "counter"
        pub rx_z_query_user_msgs,

        # HELP "Counter of received bytes in zenoh query message payloads."
        # TYPE "counter"
        pub rx_z_query_user_pl_bytes,

        # HELP "Counter of received zenoh query messages."
        # TYPE "counter"
        pub rx_z_query_admin_msgs,

        # HELP "Counter of received bytes in zenoh query message payloads."
        # TYPE "counter"
        pub rx_z_query_admin_pl_bytes,

        # HELP "Counter of received zenoh reply messages."
        # TYPE "counter"
        pub rx_z_reply_user_msgs,

        # HELP "Counter of received bytes in zenoh reply message payloads."
        # TYPE "counter"
        pub rx_z_reply_user_pl_bytes,

        # HELP "Counter of received zenoh reply messages."
        # TYPE "counter"
        pub rx_z_reply_admin_msgs,

        # HELP "Counter of received bytes in zenoh reply message payloads."
        # TYPE "counter"
        pub rx_z_reply_admin_pl_bytes,
    }
}
