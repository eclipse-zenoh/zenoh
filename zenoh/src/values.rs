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
use std::fmt;
use crate::net::{RBuf, WBuf, ZInt};
use zenoh_util::core::{ZResult, ZError, ZErrorKind};
use zenoh_util::{zerror, zerror2};
use crate::Properties;


#[derive(Debug)]
pub enum Value {
    Raw(RBuf),
    Custom { encoding_descr: String, data: RBuf },
    StringUTF8(String),
    Properties(Properties),
    Json(String),
    Integer(i64),
    Float(f64)
}

impl Value {
    pub(crate) fn encoding(&self) -> ZInt {
        use crate::net::encoding::*;
        use Value::*;
        match self {
            Raw(_)        => RAW,
            Custom{encoding_descr:_, data:_}  => APP_CUSTOM,
            StringUTF8(_) => STRING,
            Properties(_) => APP_PROPERTIES,
            Json(_) => APP_JSON,
            Integer(_)    => APP_INTEGER,
            Float(_)      => APP_FLOAT
        }
    }

    pub(crate) fn decode(encoding: ZInt, mut payload: RBuf) -> ZResult<Value> {
        use crate::net::encoding::*;
        use Value::*;
        match encoding {
            RAW => Ok(Raw(payload)),
            APP_CUSTOM => {
                if let Ok(encoding_descr) = payload.read_string() {
                    let mut data = RBuf::empty();
                    payload.drain_into_rbuf(&mut data);
                    Ok(Custom{encoding_descr, data})
                } else {
                    zerror!(ZErrorKind::ValueDecodingFailed{ descr: 
                        "Failed to read 'encoding_decscr' from a payload with Custom encoding".to_string() })
                }
            }
            STRING => String::from_utf8(payload.read_vec())
                .map(StringUTF8)
                .map_err(|e| zerror2!(ZErrorKind::ValueDecodingFailed{ descr: "Failed to decode StringUTF8 Value".to_string() }, e)),
            APP_PROPERTIES => String::from_utf8(payload.read_vec())
                .map(|s| Properties(crate::Properties::from(s)))
                .map_err(|e| zerror2!(ZErrorKind::ValueDecodingFailed{ descr: "Failed to decode UTF-8 string for a Properties Value".to_string() }, e)),
            APP_JSON | TEXT_JSON => String::from_utf8(payload.read_vec())
                .map(Json)
                .map_err(|e| zerror2!(ZErrorKind::ValueDecodingFailed{ descr: "Failed to decode UTF-8 string for a JSON Value".to_string() }, e)),
            APP_INTEGER => String::from_utf8(payload.read_vec())
                .map_err(|e| zerror2!(ZErrorKind::ValueDecodingFailed{ descr: "Failed to decode an Integer Value".to_string() }, e))
                .and_then(|s| s.parse::<i64>()
                    .map_err(|e| zerror2!(ZErrorKind::ValueDecodingFailed{ descr: "Failed to decode an Integer Value".to_string() }, e)))
                .map(Integer),
            APP_FLOAT => String::from_utf8(payload.read_vec())
                .map_err(|e| zerror2!(ZErrorKind::ValueDecodingFailed{ descr: "Failed to decode an Float Value".to_string() }, e))
                .and_then(|s| s.parse::<f64>()
                    .map_err(|e| zerror2!(ZErrorKind::ValueDecodingFailed{ descr: "Failed to decode an Float Value".to_string() }, e)))
                .map(Float),
            _ => {
                zerror!(ZErrorKind::ValueDecodingFailed{ descr: 
                    format!("Unkown encoding flag '{}'", encoding) })
            }
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use Value::*;
        match self {
            Raw(buf)        => write!(f, "{}", buf),
            Custom{encoding_descr, data}  => write!(f, "{} {}", encoding_descr, data),
            StringUTF8(s) => write!(f, "{}", s),
            Properties(p) => write!(f, "{}", p),
            Json(s)       => write!(f, "{}", s),
            Integer(i)    => write!(f, "{}", i),
            Float(fl)     => write!(f, "{}", fl)
        }
    }
}

impl From<Value> for RBuf {
    fn from(v: Value) -> Self {
        Self::from(&v)
    }
}

impl From<&Value> for RBuf {
    fn from(v: &Value) -> Self {
        use Value::*;
        match v {
            Raw(buf) => buf.clone(),
            Custom{encoding_descr, data} => {
                let mut buf = WBuf::new(64, false);
                buf.write_string(&encoding_descr);
                for slice in data.get_slices() {
                    buf.write_slice(slice.clone());
                }
                buf.into()
            },
            StringUTF8(s) => RBuf::from(s.as_bytes()),
            Properties(props) => RBuf::from(props.to_string().as_bytes()),
            Json(s) => RBuf::from(s.as_bytes()),
            Integer(i) =>  RBuf::from(i.to_string().as_bytes()),
            Float(f) =>  RBuf::from(f.to_string().as_bytes()),
        }
    }
}
