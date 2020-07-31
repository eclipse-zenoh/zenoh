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
use async_trait::async_trait;
use zenoh::net::{Query, Sample};
use zenoh::{Properties, Value, ZResult};

pub const STORAGE_PATH_EXPR_PROPERTY: &str = "path_expr";

#[async_trait]
pub trait Backend: Drop + Send + Sync {
    async fn get_admin_status(&self) -> Value;

    async fn create_storage(&mut self, props: Properties) -> ZResult<Box<dyn Storage>>;
}

#[async_trait]
pub trait Storage: Drop + Send + Sync {
    async fn get_admin_status(&self) -> Value;

    async fn on_sample(&mut self, sample: Sample) -> ZResult<()>;

    async fn on_query(&mut self, query: Query) -> ZResult<()>;
}
