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
pub mod authenticator;
pub(crate) mod establishment;
pub(crate) mod link;
pub(crate) mod manager;
pub(crate) mod rx;
pub(crate) mod transport;
pub(crate) mod tx;

use super::common;
use super::defaults;
use super::protocol;
use super::protocol::core::{PeerId, WhatAmI, ZInt};
use std::sync::{Arc, Weak};
use transport::TransportUnicastInner;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::zerror2;

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
pub struct TransportUnicast(Weak<TransportUnicastInner>);

impl TransportUnicast {
    pub(crate) fn upgrade(&self) -> Option<Arc<TransportUnicastInner>> {
        self.0.upgrade()
    }

    pub(super) fn get_transport(&self) -> ZResult<Arc<TransportUnicastInner>> {
        self.upgrade().ok_or_else(|| {
            zerror2!(ZErrorKind::InvalidReference {
                descr: "Session closed".to_string()
            })
        })
    }
}

impl From<&Arc<TransportUnicastInner>> for TransportUnicast {
    fn from(s: &Arc<TransportUnicastInner>) -> TransportUnicast {
        TransportUnicast(Arc::downgrade(s))
    }
}

impl Eq for TransportUnicast {}

impl PartialEq for TransportUnicast {
    fn eq(&self, other: &Self) -> bool {
        Weak::ptr_eq(&self.0, &other.0)
    }
}
