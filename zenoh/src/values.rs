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
use crate::net::encoding::*;
use crate::net::{RBuf, Sample, WBuf, ZInt};
use crate::workspace::ChangeKind;
use crate::Properties;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::{zerror, zerror2};

/// A user value that is associated with a [Path](super::Path) in zenoh.
#[derive(Clone, Debug)]
pub enum Value {
    /// A value as a bytes buffer (_RBuf_) and an encoding flag.  
    /// See [zenoh::net::enocding](crate::net::encoding) for available flags.
    Raw(ZInt, RBuf),
    /// A value as a bytes buffer and an encoding description (free String).  
    /// Note: this is equivalent to `Raw(APP_CUSTOM, buf)` where buf contains the encoding description and the data.
    Custom { encoding_descr: String, data: RBuf },
    /// A String value.  
    /// Note: this is equivalent to `Raw(STRING, buf)` where buf contains the String
    StringUtf8(String),
    /// A Properties value.  
    /// Note: this is equivalent to `Raw(APP_PROPERTIES, buf)` where buf contains the Properties encoded as a String
    Properties(Properties),
    /// A Json value (string format).  
    /// Note: this is equivalent to `Raw(APP_JSON, buf)` where buf contains the Json string
    Json(String),
    /// An Integer value.  
    /// Note: this is equivalent to `Raw(APP_INTEGER, buf)` where buf contains the integer encoded as a String
    Integer(i64),
    /// An Float value.  
    /// Note: this is equivalent to `Raw(APP_FLOAT, buf)` where buf contains the float encoded as a String
    Float(f64),
}

impl Value {
    /// Returns the encoding flag of the Value.
    pub fn encoding(&self) -> ZInt {
        use Value::*;
        match self {
            Raw(encoding, _) => *encoding,
            Custom {
                encoding_descr: _,
                data: _,
            } => APP_CUSTOM,
            StringUtf8(_) => STRING,
            Properties(_) => APP_PROPERTIES,
            Json(_) => APP_JSON,
            Integer(_) => APP_INTEGER,
            Float(_) => APP_FLOAT,
        }
    }

    /// Returns the encoding description of the Value.
    pub fn encoding_descr(&self) -> String {
        use Value::*;
        match self {
            Custom {
                encoding_descr,
                data: _,
            } => encoding_descr.clone(),
            _ => to_string(self.encoding()),
        }
    }

    /// Encodes the Value and return the resulting buffer and its encoding flag.
    pub fn encode(self) -> (ZInt, RBuf) {
        use Value::*;
        match self {
            Raw(encoding, buf) => (encoding, buf),
            Custom {
                encoding_descr,
                data,
            } => {
                let mut buf = WBuf::new(64, false);
                buf.write_string(&encoding_descr);
                buf.write_rbuf_slices(&data);
                (APP_CUSTOM, buf.into())
            }
            StringUtf8(s) => (STRING, RBuf::from(s.as_bytes())),
            Properties(props) => (APP_PROPERTIES, RBuf::from(props.to_string().as_bytes())),
            Json(s) => (APP_JSON, RBuf::from(s.as_bytes())),
            Integer(i) => (APP_INTEGER, RBuf::from(i.to_string().as_bytes())),
            Float(f) => (APP_FLOAT, RBuf::from(f.to_string().as_bytes())),
        }
    }

    /// Decodes the payload according to the encoding flag.
    pub fn decode(encoding: ZInt, mut payload: RBuf) -> ZResult<Value> {
        use Value::*;
        match encoding {
            APP_CUSTOM => {
                if let Some(encoding_descr) = payload.read_string() {
                    let mut data = RBuf::empty();
                    payload.drain_into_rbuf(&mut data);
                    Ok(Custom {
                        encoding_descr,
                        data,
                    })
                } else {
                    zerror!(ZErrorKind::ValueDecodingFailed {
                        descr:
                            "Failed to read 'encoding_decscr' from a payload with Custom encoding"
                                .to_string()
                    })
                }
            }
            STRING => String::from_utf8(payload.read_vec())
                .map(StringUtf8)
                .map_err(|e| {
                    zerror2!(
                        ZErrorKind::ValueDecodingFailed {
                            descr: "Failed to decode StringUtf8 Value".to_string()
                        },
                        e
                    )
                }),
            APP_PROPERTIES => String::from_utf8(payload.read_vec())
                .map(|s| Properties(crate::Properties::from(s)))
                .map_err(|e| {
                    zerror2!(
                        ZErrorKind::ValueDecodingFailed {
                            descr: "Failed to decode UTF-8 string for a Properties Value"
                                .to_string()
                        },
                        e
                    )
                }),
            APP_JSON | TEXT_JSON => String::from_utf8(payload.read_vec())
                .map(Json)
                .map_err(|e| {
                    zerror2!(
                        ZErrorKind::ValueDecodingFailed {
                            descr: "Failed to decode UTF-8 string for a JSON Value".to_string()
                        },
                        e
                    )
                }),
            APP_INTEGER => String::from_utf8(payload.read_vec())
                .map_err(|e| {
                    zerror2!(
                        ZErrorKind::ValueDecodingFailed {
                            descr: "Failed to decode an Integer Value".to_string()
                        },
                        e
                    )
                })
                .and_then(|s| {
                    s.parse::<i64>().map_err(|e| {
                        zerror2!(
                            ZErrorKind::ValueDecodingFailed {
                                descr: "Failed to decode an Integer Value".to_string()
                            },
                            e
                        )
                    })
                })
                .map(Integer),
            APP_FLOAT => String::from_utf8(payload.read_vec())
                .map_err(|e| {
                    zerror2!(
                        ZErrorKind::ValueDecodingFailed {
                            descr: "Failed to decode an Float Value".to_string()
                        },
                        e
                    )
                })
                .and_then(|s| {
                    s.parse::<f64>().map_err(|e| {
                        zerror2!(
                            ZErrorKind::ValueDecodingFailed {
                                descr: "Failed to decode an Float Value".to_string()
                            },
                            e
                        )
                    })
                })
                .map(Float),
            _ => Ok(Raw(encoding, payload)),
        }
    }

    /// Encodes the Value as an UTF-8 String, possibly converting it to base64 its content is not
    /// UTF-8 compatible. Returns a tuple containing the encoding flag, a boolean indicating if the
    /// content has been encoded to base64 and the resulting UTF-8 String.
    ///
    /// Note: for Custom Value, the resulting String will have the format:
    /// `encoding_descr + ':' + data_as_a_string` (Therefore, the `encoding_descr` must not contain the ':' character)
    pub fn encode_to_string(self) -> (ZInt, bool, String) {
        use Value::*;
        match self {
            Raw(encoding, buf) => {
                // try to directly convert buf to a String, encode into base64 if fails
                match String::from_utf8(buf.to_vec()) {
                    Ok(s) => (encoding, false, s),
                    Err(err) => (encoding, true, base64::encode(err.into_bytes())),
                }
            }
            Custom {
                encoding_descr,
                data,
            } => {
                // try to directly convert data to a String, encode into base64 if fails
                match String::from_utf8(data.to_vec()) {
                    Ok(s) => (APP_CUSTOM, false, format!("{}:{}", encoding_descr, s)),
                    Err(err) => (
                        APP_CUSTOM,
                        true,
                        format!("{}:{}", encoding_descr, base64::encode(err.into_bytes())),
                    ),
                }
            }
            StringUtf8(s) => (STRING, false, s),
            Properties(props) => (APP_PROPERTIES, false, props.to_string()),
            Json(s) => (APP_JSON, false, s),
            Integer(i) => (APP_INTEGER, false, i.to_string()),
            Float(f) => (APP_FLOAT, false, f.to_string()),
        }
    }

    /// Decodes the payload from a string according to the encoding flag,
    /// converting the string from base64 if the boolean is true (for UTF-8 compatible Values).
    pub fn decode_from_string(encoding: ZInt, base64: bool, s: String) -> ZResult<Value> {
        use Value::*;
        match encoding {
            APP_CUSTOM => {
                if let Some(i) = s.find(':') {
                    let (encoding_descr, rem) = s.split_at(i);
                    if base64 {
                        match base64::decode(&rem[1..]) {
                            Ok(bytes) => Ok(Custom {
                                encoding_descr: encoding_descr.into(),
                                data: bytes.into(),
                            }),
                            Err(e) => zerror!(ZErrorKind::ValueDecodingFailed {
                                descr: format!("Failed to decode base64 Custom Value: {}", e)
                            }),
                        }
                    } else {
                        Ok(Custom {
                            encoding_descr: encoding_descr.into(),
                            data: rem[1..].as_bytes().into(),
                        })
                    }
                } else {
                    zerror!(ZErrorKind::ValueDecodingFailed {
                        descr: format!(
                            "Failed to read 'encoding_decscr' decoding Custom Value from String: {}"
                                , s)
                    })
                }
            }
            STRING => Ok(StringUtf8(s)),
            APP_PROPERTIES => Ok(Properties(crate::Properties::from(s))),
            APP_JSON | TEXT_JSON => Ok(Json(s)),
            APP_INTEGER => s.parse::<i64>().map(Integer).map_err(|e| {
                zerror2!(
                    ZErrorKind::ValueDecodingFailed {
                        descr: "Failed to decode an Integer Value".to_string()
                    },
                    e
                )
            }),
            APP_FLOAT => s.parse::<f64>().map(Float).map_err(|e| {
                zerror2!(
                    ZErrorKind::ValueDecodingFailed {
                        descr: "Failed to decode an Float Value".to_string()
                    },
                    e
                )
            }),
            _ => {
                if base64 {
                    match base64::decode(s) {
                        Ok(bytes) => Ok(Raw(encoding, bytes.into())),
                        Err(e) => zerror!(ZErrorKind::ValueDecodingFailed {
                            descr: format!(
                                "Failed to decode base64 Value with encoding {} : {}",
                                encoding, e
                            )
                        }),
                    }
                } else {
                    Ok(Raw(encoding, s.as_bytes().into()))
                }
            }
        }
    }

    /// Convert the payload from a [`Sample`] into a [`Value`].
    /// If the Sample's kind is DELETE, `Ok(None)` is returned.
    /// Otherwise, if decode_value is `true` the payload is decoded as a typed [`Value`].
    /// If decode_value is `false`, the payload is converted into a [`Value::Raw`].
    pub fn from_sample(sample: &Sample, decode_value: bool) -> ZResult<Option<Value>> {
        let (kind, encoding) = if let Some(info) = &sample.data_info {
            (
                info.kind.map_or(ChangeKind::Put, ChangeKind::from),
                info.encoding.unwrap_or(APP_OCTET_STREAM),
            )
        } else {
            (ChangeKind::Put, APP_OCTET_STREAM)
        };
        if kind == ChangeKind::Delete {
            Ok(None)
        } else if decode_value {
            Ok(Some(Value::decode(encoding, sample.payload.clone())?))
        } else {
            Ok(Some(Value::Raw(encoding, sample.payload.clone())))
        }
    }
}

impl From<RBuf> for Value {
    fn from(buf: RBuf) -> Self {
        Value::Raw(APP_OCTET_STREAM, buf)
    }
}

impl From<Vec<u8>> for Value {
    fn from(buf: Vec<u8>) -> Self {
        Value::from(RBuf::from(buf))
    }
}

impl From<&[u8]> for Value {
    fn from(buf: &[u8]) -> Self {
        Value::from(RBuf::from(buf))
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::StringUtf8(s)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::from(s.to_string())
    }
}

impl From<Properties> for Value {
    fn from(p: Properties) -> Self {
        Value::Properties(p)
    }
}

impl From<&serde_json::Value> for Value {
    fn from(json: &serde_json::Value) -> Self {
        Value::Json(json.to_string())
    }
}

impl From<serde_json::Value> for Value {
    fn from(json: serde_json::Value) -> Self {
        Value::from(&json)
    }
}

impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Value::Integer(i)
    }
}

impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Value::Float(f)
    }
}
