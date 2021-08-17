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
// use super::transport;
// use std::sync::{Arc, Weak};
// use transport::TransportUnicastInner;
// use zenoh_util::core::{ZError, ZErrorKind, ZResult};
// use zenoh_util::zerror2;

// pub(crate) struct TransportConfigMulticast;

// pub type TransportMulticast = ();
// #[derive(Clone)]
// pub(crate) struct TransportMulticast(Weak<TransportMulticast>);

// impl TransportMulticast {
//     pub(crate) fn upgrade(&self) -> Option<Arc<TransportMulticast>> {
//         self.0.upgrade()
//     }

//     pub(super) fn get_transport(&self) -> ZResult<Arc<TransportMulticast>> {
//         self.upgrade().ok_or_else(|| {
//             zerror2!(ZErrorKind::InvalidReference {
//                 descr: "Transport closed".to_string()
//             })
//         })
//     }
// }

// impl From<&Arc<TransportMulticast>> for TransportMulticast {
//     fn from(s: &Arc<TransportMulticast>) -> TransportMulticast {
//         TransportMulticast(Arc::downgrade(s))
//     }
// }

// impl Eq for TransportMulticast {}

// impl PartialEq for TransportMulticast {
//     fn eq(&self, other: &Self) -> bool {
//         Weak::ptr_eq(&self.0, &other.0)
//     }
// }
