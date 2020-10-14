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
use async_std::sync::{Arc, RwLock};
use async_trait::async_trait;
use zenoh::net::Sample;
use zenoh::{Properties, Value, ZResult};

pub const STORAGE_PATH_EXPR_PROPERTY: &str = "path_expr";

#[async_trait]
pub trait Backend: Send + Sync {
    /// Returns the status that will be sent as a reply to a query
    /// on the administration space for this backend.
    async fn get_admin_status(&self) -> Value;

    /// Creates a storage configured with some properties.
    async fn create_storage(&mut self, props: Properties) -> ZResult<Box<dyn Storage>>;

    /// Returns an [`IncomingDataInterceptor`] that will be called before pushing any data
    /// into a storage created by this backend. `None` can be returned for no interception point.
    fn incoming_data_interceptor(&self) -> Option<Box<dyn IncomingDataInterceptor>>;

    /// Returns an [`OutgoingDataInterceptor`] that will be called before sending any reply
    /// to a query from a storage created by this backend. `None` can be returned for no interception point.
    fn outgoing_data_interceptor(&self) -> Option<Box<dyn OutgoingDataInterceptor>>;
}

#[async_trait]
pub trait Storage: Send + Sync {
    /// Returns the status that will be sent as a reply to a query
    /// on the administration space for this storage.
    async fn get_admin_status(&self) -> Value;

    /// Function called for each incoming data ([`Sample`]) to be stored in this storage.
    async fn on_sample(&mut self, sample: Sample) -> ZResult<()>;

    /// Function called for each incoming query matching this storage's PathExpression.
    /// This storage should reply with data matching the query calling [`Query::reply()`].
    async fn on_query(&mut self, query: Query) -> ZResult<()>;
}

/// An interceptor allowing to modify the data pushed into a storage before it's actually stored.
#[async_trait]
pub trait IncomingDataInterceptor: Send + Sync {
    async fn on_sample(&self, sample: Sample) -> Sample;
}

/// An interceptor allowing to modify the data going out of a storage before it's sent as a reply to a query.
#[async_trait]
pub trait OutgoingDataInterceptor: Send + Sync {
    async fn on_reply(&self, sample: Sample) -> Sample;
}

// A wrapper around the [`zenoh::net::Query`] allowing to call the
// OutgoingDataInterceptor (if any) before to send the reply
pub struct Query {
    q: zenoh::net::Query,
    interceptor: Option<Arc<RwLock<Box<dyn OutgoingDataInterceptor>>>>,
}

impl Query {
    pub fn new(
        q: zenoh::net::Query,
        interceptor: Option<Arc<RwLock<Box<dyn OutgoingDataInterceptor>>>>,
    ) -> Query {
        Query { q, interceptor }
    }

    #[inline(always)]
    pub fn res_name(&self) -> &str {
        &self.q.res_name
    }

    #[inline(always)]
    pub fn predicate(&self) -> &str {
        &self.q.predicate
    }

    pub async fn reply(&self, sample: Sample) {
        // Call outgoing intercerceptor
        let sample = if let Some(ref interceptor) = self.interceptor {
            interceptor.read().await.on_reply(sample).await
        } else {
            sample
        };
        // Send reply
        self.q.reply(sample).await
    }
}
