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
use super::core::ZInt;

pub mod data_kind {
    use super::ZInt;

    pub const PUT: ZInt = 0;
    pub const PATCH: ZInt = 1;
    pub const DELETE: ZInt = 2;

    pub const DEFAULT: ZInt = PUT;

    pub fn to_string(i: ZInt) -> String {
        match i {
            0 => "PUT".to_string(),
            1 => "PATCH".to_string(),
            2 => "DELETE".to_string(),
            i => i.to_string(),
        }
    }
}

pub mod encoding {
    use super::ZInt;
    use http_types::Mime;
    use std::str::FromStr;
    use zenoh_util::core::{ZError, ZErrorKind, ZResult};
    use zenoh_util::zerror;

    lazy_static! {
    static ref MIMES: [Mime; 20] = [
        /*  0 */ Mime::from_str("application/octet-stream").unwrap(),
        /*  1 */ Mime::from_str("application/custom").unwrap(), // non iana standard
        /*  2 */ Mime::from_str("text/plain").unwrap(),
        /*  3 */ Mime::from_str("application/properties").unwrap(), // non iana standard
        /*  4 */ Mime::from_str("application/json").unwrap(), // if not readable from casual users
        /*  5 */ Mime::from_str("application/sql").unwrap(),
        /*  6 */ Mime::from_str("application/integer").unwrap(), // non iana standard
        /*  7 */ Mime::from_str("application/float").unwrap(), // non iana standard
        /*  8 */ Mime::from_str("application/xml").unwrap(), // if not readable from casual users (RFC 3023, section 3)
        /*  9 */ Mime::from_str("application/xhtml+xml").unwrap(),
        /* 10 */ Mime::from_str("application/x-www-form-urlencoded").unwrap(),
        /* 11 */ Mime::from_str("text/json").unwrap(), // non iana standard - if readable from casual users
        /* 12 */ Mime::from_str("text/html").unwrap(),
        /* 13 */ Mime::from_str("text/xml").unwrap(), // if readable from casual users (RFC 3023, section 3)
        /* 14 */ Mime::from_str("text/css").unwrap(),
        /* 15 */ Mime::from_str("text/csv").unwrap(),
        /* 16 */ Mime::from_str("text/javascript").unwrap(),
        /* 17 */ Mime::from_str("image/jpeg").unwrap(),
        /* 18 */ Mime::from_str("image/png").unwrap(),
        /* 19 */ Mime::from_str("image/gif").unwrap(),
    ];
    }

    pub fn to_mime(i: ZInt) -> ZResult<Mime> {
        if i < MIMES.len() as ZInt {
            Ok(MIMES[i as usize].clone())
        } else {
            zerror!(ZErrorKind::Other {
                descr: format!("Unknown encoding id {}", i)
            })
        }
    }

    pub fn to_string(i: ZInt) -> String {
        match to_mime(i) {
            Ok(mime) => mime.essence().to_string(),
            _ => i.to_string(),
        }
    }

    pub fn from_str(string: &str) -> ZResult<ZInt> {
        let string = string.split(';').next().unwrap();
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
            "application/xhtml+xml" => Ok(9),
            "application/x-www-form-urlencoded" => Ok(10),
            "text/json" => Ok(11),
            "text/html" => Ok(12),
            "text/xml" => Ok(13),
            "text/css" => Ok(14),
            "text/csv" => Ok(15),
            "text/javascript" => Ok(16),
            "image/jpeg" => Ok(17),
            "image/png" => Ok(18),
            "image/gif" => Ok(19),
            s => zerror!(ZErrorKind::Other {
                descr: format!("Unknown encoding '{}'", s)
            }),
        }
    }

    pub const APP_OCTET_STREAM: ZInt = 0;
    pub const NONE: ZInt = APP_OCTET_STREAM;
    pub const APP_CUSTOM: ZInt = 1;
    pub const TEXT_PLAIN: ZInt = 2;
    pub const STRING: ZInt = TEXT_PLAIN;
    pub const APP_PROPERTIES: ZInt = 3;
    pub const APP_JSON: ZInt = 4;
    pub const APP_SQL: ZInt = 5;
    pub const APP_INTEGER: ZInt = 6;
    pub const APP_FLOAT: ZInt = 7;
    pub const APP_XML: ZInt = 8;
    pub const APP_XHTML_XML: ZInt = 9;
    pub const APP_X_WWW_FORM_URLENCODED: ZInt = 10;
    pub const TEXT_JSON: ZInt = 11;
    pub const TEXT_HTML: ZInt = 12;
    pub const TEXT_XML: ZInt = 13;
    pub const TEXT_CSS: ZInt = 14;
    pub const TEXT_CSV: ZInt = 15;
    pub const TEXT_JAVASCRIPT: ZInt = 16;
    pub const IMG_JPG: ZInt = 17;
    pub const IMG_PNG: ZInt = 18;
    pub const IMG_GIF: ZInt = 19;

    pub const DEFAULT: ZInt = APP_OCTET_STREAM;
}
