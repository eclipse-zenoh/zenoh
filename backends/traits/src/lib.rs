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
use std::convert::TryFrom;
use zenoh::net::Sample;
use zenoh::{Properties, Selector, Timestamp, Value, ZError, ZResult};

pub mod utils;

pub const STORAGE_PATH_EXPR_PROPERTY: &str = "path_expr";

/// Trait to be implemented by a Backend.  
/// A library implementing the Backend and Storage traits must also declare a [`create_backend()`] operation
/// with the `#[no_mangle]` attribute as an entrypoint to be called for the Backend creation.
///
/// # Example
/// ```
/// use async_trait::async_trait;
/// use zenoh::net::Sample;
/// use zenoh::{utils, ChangeKind, Properties, Value, ZResult};
/// use zenoh_backend_traits::*;
///
/// #[no_mangle]
/// pub fn create_backend(_unused: &Properties) -> ZResult<Box<dyn Backend>> {
///     Ok(Box::new(MyBackend {}))
/// }
///
/// // Your Backend implementation
/// pub struct MyBackend {}
///
/// #[async_trait]
/// impl Backend for MyBackend {
///     async fn get_admin_status(&self) -> Value {
///         // TODO: possibly add more properties in returned Value for more information about this storage
///         Value::Json(r#"{"kind"="some kind"}"#.to_string())
///     }
///
///     async fn create_storage(&mut self, properties: Properties) -> ZResult<Box<dyn Storage>> {
///         Ok(Box::new(MyStorage::new(properties).await?))
///     }
///
///     fn incoming_data_interceptor(&self) -> Option<Box<dyn IncomingDataInterceptor>> {
///         None
///     }
///
///     fn outgoing_data_interceptor(&self) -> Option<Box<dyn OutgoingDataInterceptor>> {
///         None
///     }
/// }
///
/// // Your Storage implementation
/// struct MyStorage {
///     admin_status: Value,
/// }
///
/// impl MyStorage {
///     async fn new(properties: Properties) -> ZResult<MyStorage> {
///         let admin_status = utils::properties_to_json_value(&properties);
///         Ok(MyStorage { admin_status })
///     }
/// }
///
/// #[async_trait]
/// impl Storage for MyStorage {
///     async fn get_admin_status(&self) -> Value {
///         // TODO: possibly add more properties in returned Value for more information about this storage
///         self.admin_status.clone()
///     }
///
///     // When receiving a Sample (i.e. on PUT or DELETE operations)
///     async fn on_sample(&mut self, sample: Sample) -> ZResult<()> {
///         // extract ChangeKind and Timestamp from sample.data_info
///         let (kind, _timestamp) = if let Some(ref info) = sample.data_info {
///             (
///                 info.kind.map_or(ChangeKind::PUT, ChangeKind::from),
///                 match &info.timestamp {
///                     Some(ts) => ts.clone(),
///                     None => new_reception_timestamp(),
///                 },
///             )
///         } else {
///             (ChangeKind::PUT, new_reception_timestamp())
///         };
///         // Store or delete the sample depending the ChangeKind
///         match kind {
///             ChangeKind::PUT => {
///                 let _key = sample.res_name;
///                 // TODO:
///                 //  - check if timestamp is newer than the stored one for the same key
///                 //  - if yes: store (key, sample)
///                 //  - if not: drop the sample
///             }
///             ChangeKind::DELETE => {
///                 let _key = sample.res_name;
///                 // TODO:
///                 //  - check if timestamp is newer than the stored one for the same key
///                 //  - if yes: mark (key, sample) as deleted (possibly scheduling definitive removal for later)
///                 //  - if not: drop the sample
///             }
///             ChangeKind::PATCH => {
///                 println!("Received PATCH for {}: not yet supported", sample.res_name);
///             }
///         }
///         Ok(())
///     }
///
///     // When receiving a Query (i.e. on GET operations)
///     async fn on_query(&mut self, query: Query) -> ZResult<()> {
///         let _path_expr = query.res_name();
///         // TODO:
///         //  - test if path expression contains *
///         //  - if not: just get the sample with key==path_expr and call: query.reply(sample.clone()).await;
///         //  - if yes: get all the samples with key matching path_expr and call for each: query.reply(sample.clone()).await;
///         //
///         // NOTE: in case query.predicate() is not empty something smarter should be done with returned samples...
///         Ok(())
///     }
/// }
/// ```
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

    /// Returns the resource name of this Query
    #[inline(always)]
    pub fn res_name(&self) -> &str {
        &self.q.res_name
    }

    /// Returns the predicate of this Query
    #[inline(always)]
    pub fn predicate(&self) -> &str {
        &self.q.predicate
    }

    /// Sends a Sample as a reply to this Query
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

impl TryFrom<&Query> for Selector {
    type Error = ZError;
    fn try_from(q: &Query) -> Result<Self, Self::Error> {
        Selector::try_from(&q.q)
    }
}

/// Generates a reception timestamp with id=0x00
pub fn new_reception_timestamp() -> Timestamp {
    use std::time::{SystemTime, UNIX_EPOCH};
    use zenoh::TimestampID;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    Timestamp::new(
        now.into(),
        TimestampID::new(1, [0u8; TimestampID::MAX_SIZE]),
    )
}
