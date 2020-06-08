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
use std::fmt;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use async_std::sync::Arc;
use spin::RwLock;
use log::trace;
use super::InnerSession;

pub use zenoh_protocol::core::{
    ZInt,
    ResourceId,
    ResKey,
    PeerId,
};
pub use zenoh_protocol::proto::{
    Reliability,
    SubMode,
    Period,
    SubInfo,
    Target,
    QueryTarget,
    QueryConsolidation,
    Reply,
    DataInfo
};
pub use zenoh_protocol::proto::Primitives;
pub use zenoh_util::core::{ZError, ZErrorKind, ZResult};


pub type Properties = HashMap<ZInt, Vec<u8>>;

pub struct QueryHandle {
    pub(crate) pid: PeerId,
    pub(crate) kind: ZInt,
    pub(crate) primitives: Arc<dyn Primitives + Send + Sync>,
    pub(crate) qid: ZInt,
    pub(crate) nb_qhandlers: Arc<AtomicUsize>,
    pub(crate) sent_final: Arc<AtomicBool>,
}

pub type DataHandler = dyn FnMut(/*res_name:*/ &str, /*payload:*/ Vec<u8>, /*data_info:*/ DataInfo) + Send + Sync + 'static;

pub type QueryHandler = dyn FnMut(/*res_name:*/ &str, /*predicate:*/ &str, /*replies_sender:*/ &RepliesSender, /*query_handle:*/ QueryHandle) + Send + Sync + 'static;

pub type RepliesSender = dyn Fn(/*query_handle:*/ QueryHandle, /*replies:*/ Vec<(String, Vec<u8>)>) + Send + Sync + 'static;

pub type RepliesHandler = dyn FnMut(&Reply) + Send + Sync + 'static;


pub(crate) type Id = usize;

#[derive(Clone)]
pub struct Publisher {
    pub(crate) id: Id,
    pub(crate) reskey: ResKey,
}

impl PartialEq for Publisher {
    fn eq(&self, other: &Publisher) -> bool {
        self.id == other.id
    }
}

impl fmt::Debug for Publisher {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Publisher{{ id:{} }}", self.id)
    }
}


#[derive(Clone)]
pub struct Subscriber {
    pub(crate) id: Id,
    pub(crate) reskey: ResKey,
    pub(crate) resname: String,
    pub(crate) dhandler: Arc<RwLock<DataHandler>>,
    pub(crate) session: Arc<RwLock<InnerSession>>
}

impl Subscriber {
    pub async fn pull(&self) -> ZResult<()> {
        // @TODO: implement
        trace!("pull({:?})", self);
        Ok(())
    }
}

impl PartialEq for Subscriber {
    fn eq(&self, other: &Subscriber) -> bool {
        self.id == other.id
    }
}

impl fmt::Debug for Subscriber {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Subscriber{{ id:{}, resname:{} }}", self.id, self.resname)
    }
}

#[derive(Clone)]
pub struct Queryable {
    pub(crate) id: Id,
    pub(crate) reskey: ResKey,
    pub(crate) kind: ZInt,
    pub(crate) qhandler: Arc<RwLock<QueryHandler>>,
}

impl PartialEq for Queryable {
    fn eq(&self, other: &Queryable) -> bool {
        self.id == other.id
    }
}

impl fmt::Debug for Queryable {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Queryable{{ id:{} }}", self.id)
    }
}
