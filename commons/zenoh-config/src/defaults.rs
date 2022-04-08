//
// Copyright (c) 2022 ZettaScale Technology
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
use num_cpus;

impl Default for TransportUnicastConf {
    fn default() -> Self {
        Self {
            accept_timeout: Some(10000),
            accept_pending: Some(100),
            max_sessions: Some(1000),
            max_links: Some(1),
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
            sequence_number_resolution: Some((2 as ZInt).pow(28)),
            lease: Some(10000),
            keep_alive: Some(2500),
            batch_size: Some(u16::MAX),
            queue: QueueConf::default(),
            threads: Some(num),
        }
    }
}

impl Default for QueueConf {
    fn default() -> Self {
        Self {
            size: QueueSizeConf::default(),
            backoff: Some(100),
        }
    }
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
            data_low: 4,
            background: 4,
        }
    }
}

impl Default for LinkRxConf {
    fn default() -> Self {
        Self {
            buffer_size: Some(u16::MAX as usize),
            max_message_size: Some(2_usize.pow(30)),
        }
    }
}

impl Default for SharedMemoryConf {
    fn default() -> Self {
        Self { enabled: true }
    }
}
