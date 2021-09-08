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
use super::core::ZInt;

pub mod data_kind {
    use super::ZInt;

    pub const PUT: ZInt = 0;
    pub const PATCH: ZInt = 1;
    pub const DELETE: ZInt = 2;

    pub const DEFAULT: ZInt = PUT;

    pub fn to_string(i: ZInt) -> String {
        match i {
            0 => "PUT".to_string(),
            1 => "PATCH".to_string(),
            2 => "DELETE".to_string(),
            i => i.to_string(),
        }
    }
}
