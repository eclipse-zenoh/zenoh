//
// Copyright (c) 2024 ZettaScale Technology
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
use zenoh_core::zparse_default;
use zenoh_protocol::core::{EndPoint, WhatAmI};

use crate::{defaults, mode_dependent::*, Config};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ConnectionRetryModeDependentConf {
    // initial wait timeout until next try
    pub period_init_ms: Option<ModeDependentValue<i64>>,
    // maximum wait timeout until next try
    pub period_max_ms: Option<ModeDependentValue<i64>>,
    // increase factor for the next timeout until next try
    pub period_increase_factor: Option<ModeDependentValue<f64>>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct ConnectionRetryConf {
    pub exit_on_failure: bool,
    pub period_init_ms: i64,
    pub period_max_ms: i64,
    pub period_increase_factor: f64,
}

impl ConnectionRetryConf {
    pub fn new(
        whatami: WhatAmI,
        exit_on_failure: bool,
        retry: ConnectionRetryModeDependentConf,
        default_retry: ConnectionRetryModeDependentConf,
    ) -> ConnectionRetryConf {
        ConnectionRetryConf {
            exit_on_failure,
            period_init_ms: *retry
                .period_init_ms
                .get(whatami)
                .unwrap_or(default_retry.period_init_ms.get(whatami).unwrap()),
            period_max_ms: *retry
                .period_max_ms
                .get(whatami)
                .unwrap_or(default_retry.period_max_ms.get(whatami).unwrap()),
            period_increase_factor: *retry
                .period_increase_factor
                .get(whatami)
                .unwrap_or(default_retry.period_increase_factor.get(whatami).unwrap()),
        }
    }

    pub fn timeout(&self) -> std::time::Duration {
        ms_to_duration(self.period_init_ms)
    }

    pub fn period(&self) -> ConnectionRetryPeriod {
        ConnectionRetryPeriod::new(self)
    }
}

pub struct ConnectionRetryPeriod {
    conf: ConnectionRetryConf,
    delay: i64,
}

impl ConnectionRetryPeriod {
    pub fn new(conf: &ConnectionRetryConf) -> ConnectionRetryPeriod {
        ConnectionRetryPeriod {
            conf: conf.clone(),
            delay: conf.period_init_ms,
        }
    }

    pub fn duration(&self) -> std::time::Duration {
        if self.conf.period_init_ms < 0 {
            return std::time::Duration::MAX;
        }

        if self.conf.period_init_ms == 0 {
            return std::time::Duration::from_millis(0);
        }

        std::time::Duration::from_millis(self.delay as u64)
    }

    pub fn next_duration(&mut self) -> std::time::Duration {
        let res = self.duration();

        self.delay = (self.delay as f64 * self.conf.period_increase_factor) as i64;
        if self.conf.period_max_ms > 0 && self.delay > self.conf.period_max_ms {
            self.delay = self.conf.period_max_ms;
        }

        res
    }
}

fn ms_to_duration(ms: i64) -> std::time::Duration {
    if ms >= 0 {
        std::time::Duration::from_millis(ms as u64)
    } else {
        std::time::Duration::MAX
    }
}

pub fn get_global_listener_timeout(config: &Config) -> std::time::Duration {
    let whatami = config.mode().unwrap_or(defaults::mode);
    ms_to_duration(
        *config
            .listen()
            .timeout_ms()
            .get(whatami)
            .unwrap_or(defaults::listen::timeout_ms.get(whatami).unwrap()),
    )
}

pub fn get_global_connect_timeout(config: &Config) -> std::time::Duration {
    let whatami = config.mode().unwrap_or(defaults::mode);
    ms_to_duration(
        *config
            .connect()
            .timeout_ms()
            .get(whatami)
            .unwrap_or(defaults::connect::timeout_ms.get(whatami).unwrap()),
    )
}

pub fn get_retry_config(
    config: &Config,
    endpoint: Option<&EndPoint>,
    listen: bool,
) -> ConnectionRetryConf {
    let whatami = config.mode().unwrap_or(defaults::mode);

    let default_retry = ConnectionRetryModeDependentConf::default();
    let retry: ConnectionRetryModeDependentConf;
    let exit_on_failure: bool;
    if listen {
        retry = config
            .listen()
            .retry()
            .clone()
            .unwrap_or_else(|| default_retry.clone());

        exit_on_failure = *config
            .listen()
            .exit_on_failure()
            .get(whatami)
            .unwrap_or(defaults::listen::exit_on_failure.get(whatami).unwrap());
    } else {
        retry = config
            .connect()
            .retry()
            .clone()
            .unwrap_or_else(|| default_retry.clone());

        exit_on_failure = *config
            .connect()
            .exit_on_failure()
            .get(whatami)
            .unwrap_or(defaults::connect::exit_on_failure.get(whatami).unwrap());
    }

    let mut res = ConnectionRetryConf::new(whatami, exit_on_failure, retry, default_retry);

    if let Some(endpoint) = endpoint {
        let config = endpoint.config();
        if let Some(val) = config.get("exit_on_failure") {
            res.exit_on_failure = zparse_default!(val, res.exit_on_failure);
        }
        if let Some(val) = config.get("retry_period_init_ms") {
            res.period_init_ms = zparse_default!(val, res.period_init_ms);
        }
        if let Some(val) = config.get("retry_period_max_ms") {
            res.period_max_ms = zparse_default!(val, res.period_max_ms);
        }
        if let Some(val) = config.get("retry_period_increase_factor") {
            res.period_increase_factor = zparse_default!(val, res.period_increase_factor);
        }
    }
    res
}
