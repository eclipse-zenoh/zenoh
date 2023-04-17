//
// Copyright (c) 2023 ZettaScale Technology
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

//! ⚠️ WARNING ⚠️
//!
//! This crate should be considered unstable, as in we might change the APIs anytime.
//!
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
//!     fn get_capability(&self) -> Capability {
//!         // This operation is used to confirm if the volume indeed supports  
//!         // the capabilities requested by the configuration
//!         Capability{
//!             persistence: Persistence::Volatile,
//!             history: History::Latest,
//!             read_cost: 0,
//!         }
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
//!     async fn put(&mut self, key: Option<OwnedKeyExpr>, value: Value, timestamp: Timestamp) -> ZResult<StorageInsertionResult> {
//!         // the key will be None if it exactly matched with the strip_prefix
//!         // create a storge specific special structure to store it
//!         // Store the data with timestamp
//!         // @TODO:
//!         // store (key, value, timestamp)
//!         return Ok(StorageInsertionResult::Inserted);
//!         //  - if any issue: drop
//!         // return Ok(StorageInsertionResult::Outdated);
//!     }
//!
//!     async fn delete(&mut self, key: Option<OwnedKeyExpr>, timestamp: Timestamp) -> ZResult<StorageInsertionResult> {
//!         // @TODO:
//!         // delete the actual entry from storage
//!         return Ok(StorageInsertionResult::Deleted);
//!     }
//!
//!     // When receiving a GET operation
//!     async fn get(&mut self, key_expr: Option<OwnedKeyExpr>, parameters: &str) -> ZResult<Vec<StoredData>> {
//!         // @TODO:
//!         // get the data associated with key_expr and return it
//!         // NOTE: in case parameters is not empty something smarter should be done with returned data...
//!         Ok(Vec::new())
//!     }
//!
//!     // To get all entries in the datastore
//!     async fn get_all_entries(&self) -> ZResult<Vec<(Option<OwnedKeyExpr>, Timestamp)>> {
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
use zenoh::value::Value;
pub use zenoh::Result as ZResult;

pub mod config;
use config::{StorageConfig, VolumeConfig};

/// Capability of a storage indicates the guarantees of the storage
/// It is used by the storage manager to take decisions on the trade-offs to ensure correct performance
pub struct Capability {
    pub persistence: Persistence,
    pub history: History,
    /// `read_cost` is a parameter that hels the storage manager take a decision on optimizing database roundtrips
    /// If the `read_cost` is higher than a given threshold, the storage manger will maintain a cache with the keys present in the database
    /// This is a placeholder, not actually utilised in the current implementation
    pub read_cost: u32,
}

/// Persistence is the guarantee expected from a storage in case of failures
/// If a storage is marked Persistent::Durable, if it restarts after a crash, it will still have all the values that were saved.
/// This will include also persisting the metadata that Zenoh stores for the updates.
/// If a storage is marked Persistent::Volatile, the storage will not have any guarantees on its content after a crash.
/// This option should be used only if the storage is considered to function as a cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Persistence {
    Volatile, //default
    Durable,
}

/// History is the number of values that the backend is expected to save per key
/// History::Latest saves only the latest value per key
/// History::All saves all the values including historical values
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum History {
    Latest, //default
    All,
}

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

#[derive(Debug, Clone)]
pub struct StoredData {
    pub value: Value,
    pub timestamp: Timestamp,
}

/// Trait to be implemented by a Backend.
///
#[async_trait]
pub trait Volume: Send + Sync {
    /// Returns the status that will be sent as a reply to a query
    /// on the administration space for this backend.
    fn get_admin_status(&self) -> serde_json::Value;

    /// Returns the capability of this backend
    fn get_capability(&self) -> Capability;

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
    /// A key can be `None` if it matches the `strip_prefix` exactly.
    /// In order to avoid data loss, the storage must store the `value` and `timestamp` associated with the `None` key
    /// in a manner suitable for the given backend technology
    async fn put(
        &mut self,
        key: Option<OwnedKeyExpr>,
        value: Value,
        timestamp: Timestamp,
    ) -> ZResult<StorageInsertionResult>;

    /// Function called for each incoming delete request to this storage.
    /// A key can be `None` if it matches the `strip_prefix` exactly.
    /// In order to avoid data loss, the storage must delete the entry corresponding to the `None` key
    /// in a manner suitable for the given backend technology
    async fn delete(
        &mut self,
        key: Option<OwnedKeyExpr>,
        timestamp: Timestamp,
    ) -> ZResult<StorageInsertionResult>;

    /// Function to retrieve the sample associated with a single key.
    /// A key can be `None` if it matches the `strip_prefix` exactly.
    /// In order to avoid data loss, the storage must retrieve the `value` and `timestamp` associated with the `None` key
    /// in a manner suitable for the given backend technology
    async fn get(
        &mut self,
        key: Option<OwnedKeyExpr>,
        parameters: &str,
    ) -> ZResult<Vec<StoredData>>;

    /// Function called to get the list of all storage content (key, timestamp)
    /// The latest Timestamp corresponding to each key is either the timestamp of the delete or put whichever is the latest.
    /// Remember to fetch the entry corresponding to the `None` key
    async fn get_all_entries(&self) -> ZResult<Vec<(Option<OwnedKeyExpr>, Timestamp)>>;
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
