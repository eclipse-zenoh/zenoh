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
use serde::de::{Unexpected, Visitor};
use serde::{de, Deserialize, Deserializer};
use std::fmt;

const DEFAULT_HTTP_INTERFACE: &str = "[::]";

#[derive(Deserialize, serde::Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(deserialize_with = "deserialize_http_port")]
    pub http_port: String,
    __path__: Option<String>,
    __required__: Option<bool>,
}

impl From<&Config> for serde_json::Value {
    fn from(c: &Config) -> Self {
        serde_json::to_value(c).unwrap()
    }
}

fn deserialize_http_port<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(HttpPortVisitor)
}

struct HttpPortVisitor;

impl<'de> Visitor<'de> for HttpPortVisitor {
    type Value = String;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(r#"either a port number as an integer or a string, either a string with format "<local_ip>:<port_number>""#)
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(format!("{DEFAULT_HTTP_INTERFACE}:{value}"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let parts: Vec<&str> = value.split(':').collect();
        if parts.len() > 2 {
            return Err(E::invalid_value(Unexpected::Str(value), &self));
        }
        let (interface, port) = if parts.len() == 1 {
            (DEFAULT_HTTP_INTERFACE, parts[0])
        } else {
            (parts[0], parts[1])
        };
        if port.parse::<u32>().is_err() {
            return Err(E::invalid_value(Unexpected::Str(port), &self));
        }
        Ok(format!("{interface}:{port}"))
    }
}
