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
use async_std::net::SocketAddr;
use std::cmp::PartialEq;
use std::fmt;
use std::hash::Hash;
use std::str::FromStr;

use zenoh_util::zerror;
use zenoh_util::core::{ZError, ZErrorKind};


/*************************************/
/*          LOCATOR                  */
/*************************************/
const SEPARATOR: char = '/';
// Protocol literals
const STR_TCP: &str = "tcp";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LocatorProtocol {
    Tcp
}

impl fmt::Display for LocatorProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LocatorProtocol::Tcp => write!(f, "{}", STR_TCP)?,
        };
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Locator {
    Tcp(SocketAddr)
}

impl FromStr for Locator {
    type Err = ZError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut iter = s.split(SEPARATOR);
        let proto = iter.next().unwrap();
        let addr = iter.next().unwrap();
        match proto {
            STR_TCP => {
                let addr: SocketAddr = match addr.parse() {
                    Ok(addr) => addr,
                    Err(e) => {
                        let e = format!("Invalid TCP locator: {}", e);
                        log::warn!("{}", e);
                        return zerror!(ZErrorKind::InvalidLocator {
                            descr: e
                        })
                    }
                };
                Ok(Locator::Tcp(addr))
            },
            _ => {
                let e = format!("Invalid protocol locator: {}", proto);
                log::warn!("{}", e);
                zerror!(ZErrorKind::InvalidLocator {
                    descr: e
                })
            }
        }
    }
}

impl Locator {
    pub fn get_proto(&self) -> LocatorProtocol {
        match self {
            Locator::Tcp(..) => LocatorProtocol::Tcp,
        }
    }
}

impl fmt::Display for Locator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Locator::Tcp(addr) => write!(f, "{}/{}", STR_TCP, addr)?,
        }
        Ok(())
    }
}

impl fmt::Debug for Locator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (proto, addr): (&str, String) = match self {
            Locator::Tcp(addr) => (STR_TCP, addr.to_string()),
        };

        f.debug_struct("Locator")
            .field("protocol", &proto)
            .field("address", &addr)
            .finish()
    }
}