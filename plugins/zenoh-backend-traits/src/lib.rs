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
//! use zenoh::{key_expr::OwnedKeyExpr, time::Timestamp, bytes::{ZBytes, Encoding}};
//! use zenoh_backend_traits::*;
//! use zenoh_backend_traits::config::*;
//! use zenoh_plugin_trait::{plugin_long_version, plugin_version, Plugin};
//! use zenoh_result::ZResult;
//! use zenoh_util::ffi::JsonValue;
//!
//!
//! // Your Backend volume implementation
//! struct MyVolumeType {
//!     config: VolumeConfig,
//! }
//!
//! // Create the entry point for your backend
//! zenoh_plugin_trait::declare_plugin!(MyVolumeType);
//!
//! impl Plugin for MyVolumeType {
//!     type StartArgs = VolumeConfig;
//!     type Instance = VolumeInstance;
//!     fn start(_name: &str, _args: &Self::StartArgs) -> ZResult<Self::Instance> {
//!         let volume = MyVolumeType {config: _args.clone()};
//!         Ok(Box::new(volume))
//!     }
//!
//!     const DEFAULT_NAME: &'static str = "my_backend";
//!     const PLUGIN_VERSION: &'static str = plugin_version!();
//!     const PLUGIN_LONG_VERSION: &'static str = plugin_long_version!();
//! }
//!
//!
//!
//! #[async_trait]
//! impl Volume for MyVolumeType {
//!     fn get_admin_status(&self) -> JsonValue {
//!         // This operation is called on GET operation on the admin space for the Volume
//!         // Here we reply with a static status (containing the configuration properties).
//!         // But we could add dynamic properties for Volume monitoring.
//!         self.config.to_json_value().into()
//!     }
//!
//!     fn get_capability(&self) -> Capability {
//!         // This operation is used to confirm if the volume indeed supports
//!         // the capabilities requested by the configuration
//!         Capability{
//!             persistence: Persistence::Volatile,
//!             history: History::Latest,
//!         }
//!     }
//!
//!     async fn create_storage(&self, properties: StorageConfig) -> zenoh::Result<Box<dyn Storage>> {
//!         // The properties are the ones passed via a PUT in the admin space for Storage creation.
//!         Ok(Box::new(MyStorage::new(properties).await?))
//!     }
//! }
//!
//! // Your Storage implementation
//! struct MyStorage {
//!     config: StorageConfig,
//! }
//!
//! impl MyStorage {
//!     async fn new(config: StorageConfig) -> zenoh::Result<MyStorage> {
//!         Ok(MyStorage { config })
//!     }
//! }
//!
//! #[async_trait]
//! impl Storage for MyStorage {
//!     fn get_admin_status(&self) -> JsonValue {
//!         // This operation is called on GET operation on the admin space for the Storage
//!         // Here we reply with a static status (containing the configuration properties).
//!         // But we could add dynamic properties for Storage monitoring.
//!         self.config.to_json_value().into()
//!     }
//!
//!     async fn put(&mut self, key: Option<OwnedKeyExpr>, payload: ZBytes, encoding: Encoding, timestamp: Timestamp) -> zenoh::Result<StorageInsertionResult> {
//!         // the key will be None if it exactly matched with the strip_prefix
//!         // create a storage specific special structure to store it
//!         // Store the data with timestamp
//!         // @TODO:
//!         // store (key, value, timestamp)
//!         return Ok(StorageInsertionResult::Inserted);
//!         //  - if any issue: drop
//!         // return Ok(StorageInsertionResult::Outdated);
//!     }
//!
//!     async fn delete(&mut self, key: Option<OwnedKeyExpr>, timestamp: Timestamp) -> zenoh::Result<StorageInsertionResult> {
//!         // @TODO:
//!         // delete the actual entry from storage
//!         return Ok(StorageInsertionResult::Deleted);
//!     }
//!
//!     // When receiving a GET operation
//!     async fn get(&mut self, key_expr: Option<OwnedKeyExpr>, parameters: &str) -> zenoh::Result<Vec<StoredData>> {
//!         // @TODO:
//!         // get the data associated with key_expr and return it
//!         // NOTE: in case parameters is not empty something smarter should be done with returned data...
//!         Ok(Vec::new())
//!     }
//!
//!     // To get all entries in the datastore
//!     async fn get_all_entries(&self) -> zenoh::Result<Vec<(Option<OwnedKeyExpr>, Timestamp)>> {
//!         // @TODO: get the list of (key, timestamp) in the datastore
//!         Ok(Vec::new())
//!     }
//! }
//! ```

use async_trait::async_trait;
use zenoh::{
    bytes::{Encoding, ZBytes},
    key_expr::{keyexpr, OwnedKeyExpr},
    time::Timestamp,
    Result as ZResult,
};
use zenoh_plugin_trait::{PluginControl, PluginInstance, PluginStatusRec};
use zenoh_util::{concat_enabled_features, ffi::JsonValue};

pub mod config;
use config::StorageConfig;

// No features are actually used in this crate, but this dummy list allows to demonstrate how to combine feature lists
// from multiple crates. See impl `PluginStructVersion` for `VolumeConfig` below.
const FEATURES: &str =
    concat_enabled_features!(prefix = "zenoh-backend-traits", features = ["default"]);

/// Capability of a storage indicates the guarantees of the storage
/// It is used by the storage manager to take decisions on the trade-offs to ensure correct performance
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capability {
    pub persistence: Persistence,
    pub history: History,
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

pub enum StorageInsertionResult {
    Outdated,
    Inserted,
    Replaced,
    Deleted,
}

#[derive(Debug, Clone)]
pub struct StoredData {
    pub payload: ZBytes,
    pub encoding: Encoding,
    pub timestamp: Timestamp,
}

/// Trait to be implemented by a Backend.
#[async_trait]
pub trait Volume: Send + Sync {
    /// Returns the status in the json format that will be sent as a reply to a query
    /// on the administration space for this backend.
    fn get_admin_status(&self) -> JsonValue;

    /// Returns the capability of this backend
    fn get_capability(&self) -> Capability;

    /// Creates a storage configured with some properties.
    async fn create_storage(&self, props: StorageConfig) -> ZResult<Box<dyn Storage>>;
}

pub type VolumeInstance = Box<dyn Volume + 'static>;

impl PluginControl for VolumeInstance {
    fn plugins_status(&self, _names: &keyexpr) -> Vec<PluginStatusRec<'_>> {
        Vec::new()
    }
}

impl PluginInstance for VolumeInstance {}

/// Trait to be implemented by a Storage.
#[async_trait]
pub trait Storage: Send + Sync {
    /// Returns the status that will be sent as a reply to a query
    /// on the administration space for this storage.
    fn get_admin_status(&self) -> JsonValue;

    /// Function called for each incoming data ([`Sample`](zenoh::sample::Sample)) to be stored in this storage.
    /// A key can be `None` if it matches the `strip_prefix` exactly.
    /// In order to avoid data loss, the storage must store the `value` and `timestamp` associated with the `None` key
    /// in a manner suitable for the given backend technology
    async fn put(
        &mut self,
        key: Option<OwnedKeyExpr>,
        payload: ZBytes,
        encoding: Encoding,
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
