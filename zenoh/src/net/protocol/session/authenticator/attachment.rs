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
use super::ZInt;

// These are the properties IDs to be included in the
// attachment of the four messages used for opening a
// session: InitSyn, InitAck, OpenSyn, and OpenAck.

#[allow(dead_code)]
pub mod authorization {
    use super::ZInt;

    pub const RESERVED: ZInt = 0;
    pub const USRPWD: ZInt = 1;
    pub const SHM: ZInt = 2;
}
