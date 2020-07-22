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
use crate::net::{RBuf, WBuf, ZInt};
use crate::net::encoding::*;
use zenoh_util::core::{ZResult, ZError, ZErrorKind};
use zenoh_util::{zerror, zerror2};
use crate::Properties;


#[derive(Clone, Debug)]
pub enum Value {
    Encoded(ZInt, RBuf),
    Raw(RBuf),                                      // alias for Encoded(RAW, encoding_descr+data)
    Custom { encoding_descr: String, data: RBuf },  // alias for Encoded(APP_CUSTOM, encoding_descr+data)
    StringUTF8(String),                             // alias for Encoded(STRING, String)
    Properties(Properties),                         // alias for Encoded(APP_PROPERTIES, props.to_string())
    Json(String),                                   // alias for Encoded(APP_JSON, String)
    Integer(i64),                                   // alias for Encoded(APP_INTEGER, i64.to_string())
    Float(f64)                                      // alias for Encoded(APP_FLOAT, f64.to_string())
}

impl Value {

    pub(crate) fn encode(self) -> (ZInt, RBuf) {
        use Value::*;
        match self {
            Encoded(encoding, buf) => (encoding, buf),
            Raw(buf)               => (RAW, buf),
            Custom{encoding_descr, data} => {
                let mut buf = WBuf::new(64, false);
                buf.write_string(&encoding_descr);
                for slice in data.get_slices() {
                    buf.write_slice(slice.clone());
                }
                (APP_CUSTOM, buf.into())
            },
            StringUTF8(s)     => (STRING, RBuf::from(s.as_bytes())),
            Properties(props) => (APP_PROPERTIES, RBuf::from(props.to_string().as_bytes())),
            Json(s)           => (APP_JSON, RBuf::from(s.as_bytes())),
            Integer(i)        => (APP_INTEGER, RBuf::from(i.to_string().as_bytes())),
            Float(f)          => (APP_FLOAT, RBuf::from(f.to_string().as_bytes()))
        }
    }

    pub(crate) fn decode(encoding: ZInt, mut payload: RBuf) -> ZResult<Value> {
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
            _ => Ok(Encoded(encoding, payload)),
        }
    }
}
