//
// Copyright (c) 2022 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//

//! This crate provides the traits to be implemented by a zenoh backend library:
//!  - [`Volume`]
//!  - [`Storage`]
//!
//! Such library must also declare a `create_volume()` operation
//! with the `#[no_mangle]` attribute as an entrypoint to be called for the Backend creation.
//!
//! # Example
//! ```
//! use std::sync::Arc;
//! use async_trait::async_trait;
//! use zenoh::prelude::r#async::*;
//! use zenoh::properties::properties_to_json_value;
//! use zenoh::time::Timestamp;
//! use zenoh_backend_traits::*;
//! use zenoh_backend_traits::config::*;
//! use zenoh::Result as ZResult;
//!
//! #[no_mangle]
//! pub fn create_volume(config: VolumeConfig) -> ZResult<Box<dyn Volume>> {
//!     Ok(Box::new(MyVolumeType { config }))
//! }
//!
//! // Your Backend implementation
//! struct MyVolumeType {
//!     config: VolumeConfig,
//! }
//!
//! #[async_trait]
//! impl Volume for MyVolumeType {
//!     fn get_admin_status(&self) -> serde_json::Value {
//!         // This operation is called on GET operation on the admin space for the Volume
//!         // Here we reply with a static status (containing the configuration properties).
//!         // But we could add dynamic properties for Volume monitoring.
//!         self.config.to_json_value()
//!     }
//!
//!     async fn create_storage(&mut self, properties: StorageConfig) -> ZResult<Box<dyn Storage>> {
//!         // The properties are the ones passed via a PUT in the admin space for Storage creation.
//!         Ok(Box::new(MyStorage::new(properties).await?))
//!     }
//!
//!     fn incoming_data_interceptor(&self) -> Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>> {
//!         // No interception point for incoming data (on PUT operations)
//!         None
//!     }
//!
//!     fn outgoing_data_interceptor(&self) -> Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>> {
//!         // No interception point for outgoing data (on GET operations)
//!         None
//!     }
//! }
//!
//! // Your Storage implementation
//! struct MyStorage {
//!     config: StorageConfig,
//! }
//!
//! impl MyStorage {
//!     async fn new(config: StorageConfig) -> ZResult<MyStorage> {
//!         Ok(MyStorage { config })
//!     }
//! }
//!
//! #[async_trait]
//! impl Storage for MyStorage {
//!     fn get_admin_status(&self) -> serde_json::Value {
//!         // This operation is called on GET operation on the admin space for the Storage
//!         // Here we reply with a static status (containing the configuration properties).
//!         // But we could add dynamic properties for Storage monitoring.
//!         self.config.to_json_value()
//!     }
//!
//!     async fn put(&mut self, mut sample: Sample) -> ZResult<StorageInsertionResult> {
//!         // When receiving a PUT operation
//!         // extract Timestamp from sample
//!         sample.ensure_timestamp();
//!         let timestamp = sample.timestamp.take().unwrap();
//!         // Store the sample
//!         let _key = sample.key_expr;
//!         // @TODO:
//!         //  - check if timestamp is newer than the stored one for the same key
//!         //  - if yes: store (key, sample)
//!         return Ok(StorageInsertionResult::Inserted);
//!         //  - if not: drop the sample
//!         // return Ok(StorageInsertionResult::Outdated);
//!     }
//!
//!     async fn delete(&mut self, mut sample: Sample) -> ZResult<StorageInsertionResult> {
//!         // When receiving a DELETE operation
//!         // extract Timestamp from sample
//!         sample.ensure_timestamp();
//!         let timestamp = sample.timestamp.take().unwrap();
//!         let _key = sample.key_expr;
//!         // @TODO:
//!         //  - check if timestamp is newer than the stored one for the same key
//!         //  - if yes: mark key as deleted (possibly scheduling definitive removal for later)
//!         return Ok(StorageInsertionResult::Deleted);
//!         //  - if not: drop the sample
//!         // return Ok(StorageInsertionResult::Outdated);
//!     }
//!
//!     // When receiving a Query (i.e. on GET operations)
//!     async fn on_query(&mut self, query: Query) -> ZResult<()> {
//!         let _key_elector = query.key_expr();
//!         // @TODO:
//!         //  - test if key selector contains *
//!         //  - if not: just get the sample with key==key_selector and call: query.reply(sample.clone()).await;
//!         //  - if yes: get all the samples with key matching key_selector and call for each: query.reply(sample.clone()).await;
//!         //
//!         // NOTE: in case query.parameters() is not empty something smarter should be done with returned samples...
//!         Ok(())
//!     }
//!
//!     // To get all entries in the datastore
//!     async fn get_all_entries(&self) -> ZResult<Vec<(OwnedKeyExpr, Timestamp)>> {
//!         // @TODO: get the list of (key, timestamp) in the datastore
//!         Ok(Vec::new())
//!     }
//! }
//! ```

use async_trait::async_trait;
use std::sync::Arc;
use zenoh::prelude::{KeyExpr, OwnedKeyExpr, Sample, Selector};
use zenoh::queryable::ReplyBuilder;
use zenoh::time::Timestamp;
pub use zenoh::Result as ZResult;

pub mod config;
use config::{StorageConfig, VolumeConfig, Capability};

/// Signature of the `confirm_capability` operation to be implemented in the library
/// This function should confirm that the library provides user requested capability
pub const CONFIRM_CAPABILITY_FN_NAME: &[u8] = b"confirm_capability";
pub type ConfirmCapability = fn(Capability) -> bool;

/// Signature of the `create_volume` operation to be implemented in the library as an entrypoint.
pub const CREATE_VOLUME_FN_NAME: &[u8] = b"create_volume";
pub type CreateVolume = fn(VolumeConfig) -> ZResult<Box<dyn Volume>>;

///
pub enum StorageInsertionResult {
    Outdated,
    Inserted,
    Replaced,
    Deleted,
}

/// Trait to be implemented by a Backend.
///
#[async_trait]
pub trait Volume: Send + Sync {
    /// Returns the status that will be sent as a reply to a query
    /// on the administration space for this backend.
    fn get_admin_status(&self) -> serde_json::Value;

    /// Creates a storage configured with some properties.
    async fn create_storage(&mut self, props: StorageConfig) -> ZResult<Box<dyn Storage>>;

    /// Returns an interceptor that will be called before pushing any data
    /// into a storage created by this backend. `None` can be returned for no interception point.
    fn incoming_data_interceptor(&self) -> Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>>;

    /// Returns an interceptor that will be called before sending any reply
    /// to a query from a storage created by this backend. `None` can be returned for no interception point.
    fn outgoing_data_interceptor(&self) -> Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>>;
}

/// Trait to be implemented by a Storage.
#[async_trait]
pub trait Storage: Send + Sync {
    /// Returns the status that will be sent as a reply to a query
    /// on the administration space for this storage.
    fn get_admin_status(&self) -> serde_json::Value;

    /// Function called for each incoming data ([`Sample`]) to be stored in this storage.
    async fn put(&mut self, sample: Sample) -> ZResult<StorageInsertionResult>;

    /// Function called for each incoming data ([`Sample`]) to be delted from this storage.
    async fn delete(&mut self, sample: Sample) -> ZResult<StorageInsertionResult>;

    /// Function called for each incoming query matching this storage's keys exp.
    /// This storage should reply with data matching the query calling [`Query::reply()`].
    async fn on_query(&mut self, query: Query) -> ZResult<()>;

    /// Function called to get the list of all storage content (key, timestamp)
    /// The latest Timestamp corresponding to each key is either the timestamp of the delete or put whichever is the latest.
    async fn get_all_entries(&self) -> ZResult<Vec<(OwnedKeyExpr, Timestamp)>>;
}

/// A wrapper around the [`zenoh::queryable::Query`] allowing to call the
/// OutgoingDataInterceptor (if any) before to send the reply
pub struct Query {
    q: zenoh::queryable::Query,
    interceptor: Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>>,
}

impl Query {
    pub fn new(
        q: zenoh::queryable::Query,
        interceptor: Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>>,
    ) -> Query {
        Query { q, interceptor }
    }

    /// The full [`Selector`] of this Query.
    #[inline(always)]
    pub fn selector(&self) -> Selector<'_> {
        self.q.selector()
    }

    /// The key selector part of this Query.
    #[inline(always)]
    pub fn key_expr(&self) -> &KeyExpr<'static> {
        self.q.key_expr()
    }

    /// This Query's selector parameters.
    #[inline(always)]
    pub fn parameters(&self) -> &str {
        self.q.parameters()
    }

    /// Sends a Sample as a reply to this Query
    pub fn reply(&self, sample: Sample) -> ReplyBuilder<'_> {
        // Call outgoing intercerceptor
        let sample = if let Some(ref interceptor) = self.interceptor {
            interceptor(sample)
        } else {
            sample
        };
        // Send reply
        self.q.reply(Ok(sample))
    }
}
