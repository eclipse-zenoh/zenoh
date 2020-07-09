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
use crate::net::{encoding, RBuf, ZInt};
use log::warn;

pub trait Value {
    fn as_rbuf(&self) -> RBuf;

    fn encoding(&self) -> ZInt;
}

impl From<&dyn Value> for RBuf {
    fn from(value: &dyn Value) -> Self {
        value.as_rbuf()
    }
}

// ------- RawValue 
pub struct RawValue(RBuf);

impl Value for RawValue {
    fn as_rbuf(&self) -> RBuf {
        self.0.clone()
    }
    fn encoding(&self) -> ZInt {
        encoding::RAW
    }
}

impl From<RBuf> for RawValue {
    fn from(rbuf: RBuf) -> Self {
        RawValue(rbuf)
    }
}

impl From<&RBuf> for RawValue {
    fn from(rbuf: &RBuf) -> Self {
        RawValue(rbuf.clone())
    }
}

impl From<Vec<u8>> for RawValue {
    fn from(buf: Vec<u8>) -> Self {
        RawValue(buf.into())
    }
}

impl From<&[u8]> for RawValue {
    fn from(buf: &[u8]) -> Self {
        RawValue(buf.into())
    }
}


// ------- StringValue 
pub struct StringValue(String);

impl Value for StringValue {
    fn as_rbuf(&self) -> RBuf {
        self.0.as_bytes().into()
    }
    fn encoding(&self) -> ZInt {
        encoding::STRING
    }
}

impl From<&RBuf> for StringValue {
    fn from(rbuf: &RBuf) -> Self {
        StringValue(String::from_utf8_lossy(&rbuf.to_vec()).into())
    }
}

impl From<String> for StringValue {
    fn from(s: String) -> Self {
        StringValue(s)
    }
}

impl From<&str> for StringValue {
    fn from(s: &str) -> Self {
        StringValue(s.into())
    }
}

// ------- IntValue
pub struct IntValue(i64);

impl Value for IntValue {
    fn as_rbuf(&self) -> RBuf {
        self.0.to_string().as_bytes().into()
    }
    fn encoding(&self) -> ZInt {
        encoding::APP_INTEGER
    }
}

impl From<&RBuf> for IntValue {
    fn from(rbuf: &RBuf) -> Self {
        IntValue(
            String::from_utf8_lossy(&rbuf.to_vec())
            .parse().unwrap_or_else(|err| {
                warn!("Error decoding IntValue '{}': {}", String::from_utf8_lossy(&rbuf.to_vec()), err);
                i64::default()
            }))
    }
}

impl From<i64> for IntValue {
    fn from(i: i64) -> Self {
        IntValue(i)
    }
}
// ------- FloatValue
pub struct FloatValue(f64);

impl Value for FloatValue {
    fn as_rbuf(&self) -> RBuf {
        self.0.to_string().as_bytes().into()
    }
    fn encoding(&self) -> ZInt {
        encoding::APP_FLOAT
    }
}

impl From<&RBuf> for FloatValue {
    fn from(rbuf: &RBuf) -> Self {
        FloatValue(
            String::from_utf8_lossy(&rbuf.to_vec())
            .parse().unwrap_or_else(|err| {
                warn!("Error decoding FloatValue '{}': {}", String::from_utf8_lossy(&rbuf.to_vec()), err);
                f64::default()
            }))
    }
}

impl From<f64> for FloatValue {
    fn from(f: f64) -> Self {
        FloatValue(f)
    }
}