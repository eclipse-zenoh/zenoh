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
        $(#[$field_meta:meta])*
        $field_vis:vis $field_name:ident,
        )*
     }
    ) => {
        paste::paste! {
            $vis struct $struct_name {
                $(
                $(#[$field_meta:meta])*
                $field_vis $field_name: AtomicUsize,
                )*
            }

            $(#[$meta])*
            $vis struct [<$struct_name Report>] {
                $(
                $(#[$field_meta:meta])*
                $field_vis $field_name: usize,
                )*
            }

            impl $struct_name {
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
                }
                )*
            }

            impl Default for $struct_name {
                fn default() -> $struct_name {
                    $struct_name {
                        $($field_name: AtomicUsize::new(0),)*
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
        pub tx_t_msgs,
        pub tx_n_msgs,
        pub tx_n_dropped,
        pub tx_z_put_user_msgs,
        pub tx_z_put_user_pl_bytes,
        // pub tx_z_put_admin_msgs,
        // pub tx_z_put_admin_pl_bytes,
        pub tx_z_del_user_msgs,
        // pub tx_z_del_admin_msgs,
        pub tx_z_query_user_msgs,
        pub tx_z_query_user_pl_bytes,
        // pub tx_z_query_admin_msgs,
        // pub tx_z_query_admin_pl_bytes,
        pub tx_z_reply_user_msgs,
        pub tx_z_reply_user_pl_bytes,
        // pub tx_z_reply_admin_msgs,
        // pub tx_z_reply_admin_pl_bytes,
        pub tx_bytes,

        pub rx_t_msgs,
        pub rx_n_msgs,
        pub rx_z_put_user_msgs,
        pub rx_z_put_user_pl_bytes,
        // pub rx_z_put_admin_msgs,
        // pub rx_z_put_admin_pl_bytes,
        pub rx_z_del_user_msgs,
        // pub rx_z_del_admin_msgs,
        pub rx_z_query_user_msgs,
        pub rx_z_query_user_pl_bytes,
        // pub rx_z_query_admin_msgs,
        // pub rx_z_query_admin_pl_bytes,
        pub rx_z_reply_user_msgs,
        pub rx_z_reply_user_pl_bytes,
        // pub rx_z_reply_admin_msgs,
        // pub rx_z_reply_admin_pl_bytes,
        pub rx_bytes,
    }
}
