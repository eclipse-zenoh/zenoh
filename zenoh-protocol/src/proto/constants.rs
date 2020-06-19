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
pub mod kind {
    use crate::core::ZInt;

    pub const PUT: ZInt = 0;
    pub const UPDATE: ZInt = 1;
    pub const REMOVE: ZInt = 2;

    pub const DEFAULT: ZInt = PUT;
}

pub mod encoding {
    use crate::core::ZInt;
    use zenoh_util::zerror;
    use zenoh_util::core::{ZResult, ZError, ZErrorKind};
    use http_types::Mime;
    use std::str::FromStr;

    lazy_static! {
    static ref MIMES: [Mime; 17] = [
        /*  0 */ Mime::from_str("application/octet-stream").unwrap(),
        /*  1 */ Mime::from_str("application/custom").unwrap(),
        /*  2 */ Mime::from_str("text/plain").unwrap(),
        /*  3 */ Mime::from_str("application/properties").unwrap(),
        /*  4 */ Mime::from_str("application/json").unwrap(), // if not readable from casual users
        /*  5 */ Mime::from_str("application/sql").unwrap(),
        /*  6 */ Mime::from_str("application/integer").unwrap(),
        /*  7 */ Mime::from_str("application/float").unwrap(),
        /*  8 */ Mime::from_str("application/xml").unwrap(), // if not readable from casual users (RFC 3023, section 3)
        /*  9 */ Mime::from_str("text/json").unwrap(), //  if readable from casual users
        /* 10 */ Mime::from_str("text/htlm").unwrap(),
        /* 11 */ Mime::from_str("text/xml").unwrap(), //  if readable from casual users (RFC 3023, section 3)
        /* 12 */ Mime::from_str("text/css").unwrap(),
        /* 13 */ Mime::from_str("text/javascript").unwrap(),
        /* 14 */ Mime::from_str("image/jpeg").unwrap(),
        /* 15 */ Mime::from_str("image/png").unwrap(),
        /* 16 */ Mime::from_str("image/gif").unwrap(),
    ];
    }

    pub fn to_mime(i: ZInt) -> ZResult<Mime> {
        if i < MIMES.len() as u64 {
            Ok(MIMES[i as usize].clone())
        } else {
            zerror!(ZErrorKind::Other { descr: format!("Unknown encoding id {}", i)})
        }
    }

    pub fn to_str(i: ZInt) -> ZResult<String> {
        Ok(to_mime(i)?.essence().to_string())
    }

    pub fn from_str(string: &str) -> ZResult<ZInt> {
        match string {
            "application/octet-stream" => Ok(0),
            "application/custom" => Ok(1),
            "text/plain" => Ok(2),
            "application/properties" => Ok(3),
            "application/json" => Ok(4),
            "application/sql" => Ok(5),
            "application/integer" => Ok(6),
            "application/float" => Ok(7),
            "application/xml" => Ok(8),
            "text/json" => Ok(9),
            "text/htlm" => Ok(10),
            "text/xml" => Ok(11),
            "text/css" => Ok(12),
            "text/javascript" => Ok(13),
            "image/jpeg" => Ok(14),
            "image/png" => Ok(15),
            "image/gif" => Ok(16),
            s => zerror!(ZErrorKind::Other { descr: format!("Unknown encoding '{}'",  s)}),
        }
    }

    pub const APP_OCTET_STREAM: ZInt = 0;
    pub const RAW: ZInt = APP_OCTET_STREAM;
    pub const APP_CUSTOM: ZInt = 1;
    pub const TEXT_PLAIN: ZInt = 2;
    pub const STRING: ZInt = TEXT_PLAIN;
    pub const APP_PROPERTIES: ZInt = 3;
    pub const APP_JSON: ZInt = 4;
    pub const APP_SQL: ZInt = 5;
    pub const APP_INTEGER: ZInt = 6;
    pub const APP_FLOAT: ZInt = 7;
    pub const APP_XML: ZInt = 8;
    pub const TEXT_JSON: ZInt = 9;
    pub const TEXT_HTML: ZInt = 10;
    pub const TEXT_XML: ZInt = 11;
    pub const TEXT_CSS: ZInt = 12;
    pub const TEXT_JAVASCRIPT: ZInt = 13;
    pub const IMG_JPG: ZInt = 14;
    pub const IMG_PNG: ZInt = 15;
    pub const IMG_GIF: ZInt = 16;
}