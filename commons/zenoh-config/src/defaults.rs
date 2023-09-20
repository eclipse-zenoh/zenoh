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
use super::*;

pub const ENV: &str = "ZENOH_CONFIG";

macro_rules! mode_accessor {
    ($type:ty) => {
        #[inline]
        pub fn get(whatami: zenoh_protocol::core::WhatAmI) -> &'static $type {
            match whatami {
                zenoh_protocol::core::WhatAmI::Router => router,
                zenoh_protocol::core::WhatAmI::Peer => peer,
                zenoh_protocol::core::WhatAmI::Client => client,
            }
        }
    };
}

#[allow(non_upper_case_globals)]
#[allow(dead_code)]
pub const mode: WhatAmI = WhatAmI::Peer;

#[allow(non_upper_case_globals)]
#[allow(dead_code)]
pub mod scouting {
    pub const timeout: u64 = 3000;
    pub const delay: u64 = 200;
    pub mod multicast {
        pub const enabled: bool = true;
        pub const address: ([u8; 4], u16) = ([224, 0, 0, 224], 7446);
        pub const interface: &str = "auto";
        pub mod autoconnect {
            pub const router: &crate::WhatAmIMatcher = // ""
                &crate::WhatAmIMatcher::empty();
            pub const peer: &crate::WhatAmIMatcher = // "router|peer"
                &crate::WhatAmIMatcher::empty().router().peer();
            pub const client: &crate::WhatAmIMatcher = // "router|peer"
                &crate::WhatAmIMatcher::empty().router().peer();
            mode_accessor!(crate::WhatAmIMatcher);
        }
        pub mod listen {
            pub const router: &bool = &true;
            pub const peer: &bool = &true;
            pub const client: &bool = &false;
            mode_accessor!(bool);
        }
    }
    pub mod gossip {
        pub const enabled: bool = true;
        pub const multihop: bool = false;
        pub mod autoconnect {
            pub const router: &crate::WhatAmIMatcher = // ""
                &crate::WhatAmIMatcher::empty();
            pub const peer: &crate::WhatAmIMatcher = // "router|peer"
                &crate::WhatAmIMatcher::empty().router().peer();
            pub const client: &crate::WhatAmIMatcher = // "router|peer"
                &crate::WhatAmIMatcher::empty().router().peer();
            mode_accessor!(crate::WhatAmIMatcher);
        }
    }
}

#[allow(non_upper_case_globals)]
#[allow(dead_code)]
pub mod timestamping {
    pub mod enabled {
        pub const router: &bool = &true;
        pub const peer: &bool = &false;
        pub const client: &bool = &false;
        mode_accessor!(bool);
    }
    pub const drop_future_timestamp: bool = false;
}

#[allow(non_upper_case_globals)]
#[allow(dead_code)]
pub const queries_default_timeout: u64 = 10000;

#[allow(non_upper_case_globals)]
#[allow(dead_code)]
pub mod routing {
    pub mod router {
        pub const peers_failover_brokering: bool = true;
    }
    pub mod peer {
        pub const mode: &str = "peer_to_peer";
    }
}

impl Default for TransportUnicastConf {
    fn default() -> Self {
        Self {
            accept_timeout: 10_000,
            accept_pending: 100,
            max_sessions: 1_000,
            max_links: 1,
            lowlatency: false,
        }
    }
}

impl Default for TransportMulticastConf {
    fn default() -> Self {
        Self {
            join_interval: Some(2500),
            max_sessions: Some(1000),
        }
    }
}

impl Default for QoSConf {
    fn default() -> Self {
        Self { enabled: true }
    }
}

impl Default for LinkTxConf {
    #[allow(clippy::unnecessary_cast)]
    fn default() -> Self {
        let num = 1 + ((num_cpus::get() - 1) / 4);
        Self {
            sequence_number_resolution: Bits::from(TransportSn::MAX),
            lease: 10_000,
            keep_alive: 4,
            batch_size: BatchSize::MAX,
            queue: QueueConf::default(),
            threads: num,
        }
    }
}

impl Default for QueueConf {
    fn default() -> Self {
        Self {
            size: QueueSizeConf::default(),
            backoff: 100,
        }
    }
}

impl QueueSizeConf {
    pub const MIN: usize = 1;
    pub const MAX: usize = 16;
}

impl Default for QueueSizeConf {
    fn default() -> Self {
        Self {
            control: 1,
            real_time: 1,
            interactive_low: 1,
            interactive_high: 1,
            data_high: 2,
            data: 4,
            data_low: 2,
            background: 1,
        }
    }
}

impl Default for LinkRxConf {
    fn default() -> Self {
        Self {
            buffer_size: BatchSize::MAX as usize,
            max_message_size: 2_usize.pow(30),
        }
    }
}

// Make explicit the value and ignore clippy warning
#[allow(clippy::derivable_impls)]
impl Default for SharedMemoryConf {
    fn default() -> Self {
        Self { enabled: false }
    }
}
