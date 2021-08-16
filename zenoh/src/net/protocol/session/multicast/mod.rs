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
// pub(crate) mod authenticator;
pub(crate) mod establishment;
// pub(crate) mod link;
// pub(crate) mod manager;
// pub(crate) mod rx;
// pub(crate) mod transport;
// pub(crate) mod tx;

// use super::common;
// use super::core;
// use super::core::{PeerId, WhatAmI, ZInt};
// use super::defaults;
// use super::io;
// use super::proto;
// use super::session;
// use std::sync::{Arc, Weak};
// use transport::SessionTransportUnicast;
// use zenoh_util::core::{ZError, ZErrorKind, ZResult};
// use zenoh_util::zerror2;

// pub(crate) struct SessionConfigMulticast;

// pub type SessionTransportMulticast = ();
// #[derive(Clone)]
// pub(crate) struct SessionMulticast(Weak<SessionTransportMulticast>);

// impl SessionMulticast {
//     pub(crate) fn upgrade(&self) -> Option<Arc<SessionTransportMulticast>> {
//         self.0.upgrade()
//     }

//     pub(super) fn get_transport(&self) -> ZResult<Arc<SessionTransportMulticast>> {
//         self.upgrade().ok_or_else(|| {
//             zerror2!(ZErrorKind::InvalidReference {
//                 descr: "Session closed".to_string()
//             })
//         })
//     }
// }

// impl From<&Arc<SessionTransportMulticast>> for SessionMulticast {
//     fn from(s: &Arc<SessionTransportMulticast>) -> SessionMulticast {
//         SessionMulticast(Arc::downgrade(s))
//     }
// }

// impl Eq for SessionMulticast {}

// impl PartialEq for SessionMulticast {
//     fn eq(&self, other: &Self) -> bool {
//         Weak::ptr_eq(&self.0, &other.0)
//     }
// }
