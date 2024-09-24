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
//! This crate is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)
pub mod unicast;

use std::str::FromStr;

use async_trait::async_trait;
pub use unicast::*;
use zenoh_config::Config;
use zenoh_core::zconfigurable;
use zenoh_link_commons::{ConfigurationInspector, LocatorInspector};
use zenoh_protocol::core::{parameters, Locator, Metadata, Reliability};
use zenoh_result::ZResult;

pub const UNIXPIPE_LOCATOR_PREFIX: &str = "unixpipe";

const IS_RELIABLE: bool = true;

#[derive(Default, Clone, Copy)]
pub struct UnixPipeLocatorInspector;
#[async_trait]
impl LocatorInspector for UnixPipeLocatorInspector {
    fn protocol(&self) -> &str {
        UNIXPIPE_LOCATOR_PREFIX
    }

    async fn is_multicast(&self, _locator: &Locator) -> ZResult<bool> {
        Ok(false)
    }

    fn is_reliable(&self, locator: &Locator) -> ZResult<bool> {
        if let Some(reliability) = locator
            .metadata()
            .get(Metadata::RELIABILITY)
            .map(Reliability::from_str)
            .transpose()?
        {
            Ok(reliability == Reliability::Reliable)
        } else {
            Ok(IS_RELIABLE)
        }
    }
}

#[derive(Default, Clone, Copy, Debug)]
pub struct UnixPipeConfigurator;

impl ConfigurationInspector<Config> for UnixPipeConfigurator {
    fn inspect_config(&self, config: &Config) -> ZResult<String> {
        let mut properties: Vec<(&str, &str)> = vec![];

        let c = config.transport().link().unixpipe();
        let file_access_mask_;
        if let Some(file_access_mask) = c.file_access_mask() {
            file_access_mask_ = file_access_mask.to_string();
            properties.push((config::FILE_ACCESS_MASK, &file_access_mask_));
        }

        let s = parameters::from_iter(properties.drain(..));

        Ok(s)
    }
}

zconfigurable! {
    // Default access mask for pipe files
    static ref FILE_ACCESS_MASK: u32 = config::FILE_ACCESS_MASK_DEFAULT;
}

pub mod config {
    pub const FILE_ACCESS_MASK: &str = "file_mask";
    pub const FILE_ACCESS_MASK_DEFAULT: u32 = 0o777;
}
