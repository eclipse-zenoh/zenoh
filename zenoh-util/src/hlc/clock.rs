//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use super::ntp64::NTP64;
use std::time::{SystemTime, UNIX_EPOCH};

// A function giving the current time
pub type Clock = dyn Fn() -> NTP64 + Send + Sync + 'static;

// A clock relying on std::time::SystemTime::now(), and returning
// a NTP64 relative to std::time::UNIX_EPOCH (1st Jan 1970).
pub fn system_time_clock() -> NTP64 {
    NTP64::from(SystemTime::now().duration_since(UNIX_EPOCH).unwrap())
}
