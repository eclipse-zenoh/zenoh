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
pub(crate) mod authenticator;
pub(crate) mod establishment;
pub(crate) mod link;
pub(crate) mod manager;
pub(crate) mod rx;
pub(crate) mod transport;
pub(crate) mod tx;

use super::common;
use super::core;
use super::core::{PeerId, WhatAmI, ZInt};
use super::defaults;
use super::io;
use super::proto;
use super::session;
use std::sync::{Arc, Weak};
use transport::SessionTransportUnicast;
use zenoh_util::core::{ZErrorKind, ZResult};

pub(crate) struct SessionConfigUnicast {
    pub(crate) peer: PeerId,
    pub(crate) whatami: WhatAmI,
    pub(crate) sn_resolution: ZInt,
    pub(crate) initial_sn_tx: ZInt,
    pub(crate) initial_sn_rx: ZInt,
    pub(crate) is_shm: bool,
    pub(crate) is_qos: bool,
}

#[derive(Clone)]
pub(crate) struct SessionUnicast(Weak<SessionTransportUnicast>);

impl SessionUnicast {
    pub(crate) fn upgrade(&self) -> Option<Arc<SessionTransportUnicast>> {
        self.0.upgrade()
    }

    pub(super) fn get_transport(&self) -> ZResult<Arc<SessionTransportUnicast>> {
        self.upgrade().ok_or_else(|| {
            zerror2!(ZErrorKind::InvalidReference {
                descr: "Session closed".to_string()
            })
        })
    }
}

impl From<&Arc<SessionTransportUnicast>> for SessionUnicast {
    fn from(s: &Arc<SessionTransportUnicast>) -> SessionUnicast {
        SessionUnicast(Arc::downgrade(s))
    }
}

impl Eq for SessionUnicast {}

impl PartialEq for SessionUnicast {
    fn eq(&self, other: &Self) -> bool {
        Weak::ptr_eq(&self.0, &other.0)
    }
}
