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
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use zenoh_util::stats_struct;
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
