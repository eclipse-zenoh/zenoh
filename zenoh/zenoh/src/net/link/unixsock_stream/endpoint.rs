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
use super::*;
use async_std::path::PathBuf;
use std::fmt;
use std::str::FromStr;
use zenoh_core::{zerror, Result as ZResult};
use zenoh_util::properties::Properties;

#[allow(unreachable_patterns)]
pub(super) fn get_unix_path(locator: &Locator) -> ZResult<PathBuf> {
    match &locator.address {
        LocatorAddress::UnixSocketStream(path) => Ok(path.path.clone()),
        _ => {
            let e = zerror!("Not a UnixSocketStream locator: {:?}", locator);
            log::debug!("{}", e);
            Err(e.into())
        }
    }
}

#[allow(unreachable_patterns)]
pub(super) fn get_unix_path_as_string(locator: &Locator) -> String {
    match &locator.address {
        LocatorAddress::UnixSocketStream(path) => match path.path.to_str() {
            Some(path_str) => path_str.to_string(),
            None => {
                let e = format!("Not a UnixSocketStream locator: {:?}", locator);
                log::debug!("{}", e);
                "None".to_string()
            }
        },
        _ => {
            let e = format!("Not a UnixSocketStream locator: {:?}", locator);
            log::debug!("{}", e);
            "None".to_string()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LocatorUnixSocketStream {
    pub(super) path: PathBuf,
}

impl LocatorUnixSocketStream {
    pub fn is_multicast(&self) -> bool {
        false
    }
}

impl FromStr for LocatorUnixSocketStream {
    type Err = zenoh_core::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let addr = match PathBuf::from(s).to_str() {
            Some(path) => PathBuf::from(path),
            None => {
                bail!("Invalid UnixSocketStream locator: {:?}", s);
            }
        };
        Ok(LocatorUnixSocketStream { path: addr })
    }
}

impl fmt::Display for LocatorUnixSocketStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = self.path.to_str().unwrap_or("None");
        write!(f, "{}", path)?;
        Ok(())
    }
}

/*************************************/
/*          LOCATOR CONFIG           */
/*************************************/
#[derive(Clone)]
pub struct LocatorConfigUnixSocketStream;

impl LocatorConfigUnixSocketStream {
    pub fn from_config(_config: &crate::config::Config) -> ZResult<Option<Properties>> {
        Ok(None)
    }
}
