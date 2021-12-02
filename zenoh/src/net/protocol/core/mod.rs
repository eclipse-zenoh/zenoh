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
pub mod key_expr;

use http_types::Mime;
use std::borrow::Cow;
use std::convert::{From, TryFrom, TryInto};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::num::NonZeroU64;
use std::str::FromStr;
use std::sync::atomic::AtomicU64;
pub use uhlc::{Timestamp, NTP64};
use uuid::Uuid;
use zenoh_util::bail;
use zenoh_util::core::Result as ZResult;

/// The unique Id of the [`HLC`](uhlc::HLC) that generated the concerned [`Timestamp`].
pub type TimestampId = uhlc::ID;

/// A zenoh integer.
pub type ZInt = u64;
pub type ZiInt = i64;
pub type AtomicZInt = AtomicU64;
pub type NonZeroZInt = NonZeroU64;
pub const ZINT_MAX_BYTES: usize = 10;

// WhatAmI values
pub type WhatAmI = whatami::WhatAmI;

/// Constants and helpers for zenoh `whatami` flags.
pub mod whatami {
    use super::{NonZeroZInt, ZInt};

    #[repr(u8)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum WhatAmI {
        Router = 1,
        Peer = 1 << 1,
        Client = 1 << 2,
    }

    impl std::str::FromStr for WhatAmI {
        type Err = ();

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            match s {
                "router" => Ok(WhatAmI::Router),
                "peer" => Ok(WhatAmI::Peer),
                "client" => Ok(WhatAmI::Client),
                _ => Err(()),
            }
        }
    }

    impl WhatAmI {
        pub fn to_str(self) -> &'static str {
            match self {
                WhatAmI::Router => "router",
                WhatAmI::Peer => "peer",
                WhatAmI::Client => "client",
            }
        }

        pub fn try_from(value: ZInt) -> Option<Self> {
            const CLIENT: ZInt = WhatAmI::Client as ZInt;
            const ROUTER: ZInt = WhatAmI::Router as ZInt;
            const PEER: ZInt = WhatAmI::Peer as ZInt;
            match value {
                CLIENT => Some(WhatAmI::Client),
                ROUTER => Some(WhatAmI::Router),
                PEER => Some(WhatAmI::Peer),
                _ => None,
            }
        }
    }

    impl std::fmt::Display for WhatAmI {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.to_str())
        }
    }

    impl serde::Serialize for WhatAmI {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            serializer.serialize_str(self.to_str())
        }
    }

    pub struct WhatAmIVisitor;

    impl<'de> serde::de::Visitor<'de> for WhatAmIVisitor {
        type Value = WhatAmI;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("either 'router', 'client' or 'peer'")
        }
        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            v.parse()
                .map_err(|_| serde::de::Error::unknown_variant(v, &["router", "client", "peer"]))
        }
        fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            self.visit_str(v)
        }
        fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            self.visit_str(&v)
        }
    }

    impl<'de> serde::Deserialize<'de> for WhatAmI {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            deserializer.deserialize_str(WhatAmIVisitor)
        }
    }

    impl From<WhatAmI> for ZInt {
        fn from(w: WhatAmI) -> Self {
            w as ZInt
        }
    }

    use std::ops::BitOr;
    #[repr(transparent)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct WhatAmIMatcher(pub NonZeroZInt);

    impl WhatAmIMatcher {
        pub fn try_from<T: std::convert::TryInto<ZInt>>(i: T) -> Option<Self> {
            let i = i.try_into().ok()?;
            if 0 < i && i < 8 {
                Some(WhatAmIMatcher(unsafe { NonZeroZInt::new_unchecked(i) }))
            } else {
                None
            }
        }

        pub fn matches(self, w: WhatAmI) -> bool {
            (self.0.get() & w as ZInt) != 0
        }

        pub fn to_str(self) -> &'static str {
            match self.0.get() {
                2 => "peer",
                4 => "client",
                1 => "router",
                3 => "router|peer",
                6 => "client|peer",
                5 => "client|router",
                7 => "client|router|peer",
                _ => "invalid_matcher",
            }
        }
    }

    impl std::str::FromStr for WhatAmIMatcher {
        type Err = ();

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            let mut inner = 0;
            for s in s.split('|') {
                match s.trim() {
                    "router" => inner |= WhatAmI::Router as ZInt,
                    "client" => inner |= WhatAmI::Client as ZInt,
                    "peer" => inner |= WhatAmI::Peer as ZInt,
                    _ => return Err(()),
                }
            }
            Self::try_from(inner).ok_or(())
        }
    }

    impl serde::Serialize for WhatAmIMatcher {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            serializer.serialize_str(self.to_str())
        }
    }
    struct WhatAmIMatcherVisitor;
    impl<'de> serde::de::Visitor<'de> for WhatAmIMatcherVisitor {
        type Value = WhatAmIMatcher;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter
                .write_str("a | separated list of whatami variants ('peer', 'client' or 'router')")
        }
        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            v.parse().map_err(|_| {
                serde::de::Error::invalid_value(
                    serde::de::Unexpected::Str(v),
                    &"a | separated list of whatami variants ('peer', 'client' or 'router')",
                )
            })
        }
        fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            self.visit_str(v)
        }
        fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            self.visit_str(&v)
        }
    }

    impl<'de> serde::Deserialize<'de> for WhatAmIMatcher {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            deserializer.deserialize_str(WhatAmIMatcherVisitor)
        }
    }

    impl std::fmt::Display for WhatAmIMatcher {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.to_str())
        }
    }

    impl From<WhatAmIMatcher> for ZInt {
        fn from(w: WhatAmIMatcher) -> ZInt {
            w.0.get() as ZInt
        }
    }

    impl<T> BitOr<T> for WhatAmIMatcher
    where
        NonZeroZInt: BitOr<T, Output = NonZeroZInt>,
    {
        type Output = Self;
        fn bitor(self, rhs: T) -> Self::Output {
            WhatAmIMatcher(self.0 | rhs)
        }
    }

    impl BitOr<WhatAmI> for WhatAmIMatcher {
        type Output = Self;
        fn bitor(self, rhs: WhatAmI) -> Self::Output {
            self | rhs as ZInt
        }
    }

    impl BitOr for WhatAmIMatcher {
        type Output = Self;
        fn bitor(self, rhs: Self) -> Self::Output {
            self | rhs.0
        }
    }

    impl BitOr for WhatAmI {
        type Output = WhatAmIMatcher;
        fn bitor(self, rhs: Self) -> Self::Output {
            WhatAmIMatcher(unsafe { NonZeroZInt::new_unchecked(self as ZInt | rhs as ZInt) })
        }
    }

    impl From<WhatAmI> for WhatAmIMatcher {
        fn from(w: WhatAmI) -> Self {
            WhatAmIMatcher(unsafe { NonZeroZInt::new_unchecked(w as ZInt) })
        }
    }
}

/// A numerical Id mapped to a key expression with [`declare_expr`](crate::Session::declare_expr).
pub type ExprId = ZInt;

pub const EMPTY_EXPR_ID: ExprId = 0;

/// A zenoh **resource** is represented by a pair composed by a **key** and a
/// **value**, such as, ```(/car/telemetry/speed, 320)```.  A **resource key**
/// is an arbitrary array of characters, with the exclusion of the symbols
/// ```*```, ```**```, ```?```, ```[```, ```]```, and ```#```,
/// which have special meaning in the context of zenoh.
///
/// A key including any number of the wildcard symbols, ```*``` and ```**```,
/// such as, ```/car/telemetry/*```, is called a **key expression** as it
/// denotes a set of keys. The wildcard character ```*``` expands to an
/// arbitrary string not including zenoh's reserved characters and the ```/```
/// character, while the ```**``` expands to  strings that may also include the
/// ```/``` character.  
///
/// Finally, it is worth mentioning that for time and space efficiency matters,
/// zenoh will automatically map key expressions to small integers. The mapping is automatic,
/// but it can be triggered excplicily by with [`declare_expr`](crate::Session::declare_expr).
///
//
//  7 6 5 4 3 2 1 0
// +-+-+-+-+-+-+-+-+
// ~      id       — if Expr : id=0
// +-+-+-+-+-+-+-+-+
// ~    suffix     ~ if flag K==1 in Message's header
// +---------------+
//
#[derive(PartialEq, Eq, Hash, Clone)]
pub struct KeyExpr<'a> {
    pub(crate) scope: ExprId, // 0 marks global scope
    pub(crate) suffix: Cow<'a, str>,
}

impl<'a> KeyExpr<'a> {
    pub fn as_str(&'a self) -> &'a str {
        if self.scope == 0 {
            self.suffix.as_ref()
        } else {
            "<encoded_expr>"
        }
    }

    pub fn try_as_str(&'a self) -> ZResult<&'a str> {
        if self.scope == 0 {
            Ok(self.suffix.as_ref())
        } else {
            bail!("Scoped key expression")
        }
    }

    pub fn as_id(&'a self) -> ExprId {
        self.scope
    }

    pub fn try_as_id(&'a self) -> ZResult<ExprId> {
        if self.has_suffix() {
            bail!("Suffixed key expression")
        } else {
            Ok(self.scope)
        }
    }

    pub fn as_id_and_suffix(&'a self) -> (ExprId, &'a str) {
        (self.scope, self.suffix.as_ref())
    }

    pub(crate) fn has_suffix(&self) -> bool {
        !self.suffix.as_ref().is_empty()
    }

    pub fn to_owned(&self) -> KeyExpr<'static> {
        KeyExpr {
            scope: self.scope,
            suffix: self.suffix.to_string().into(),
        }
    }

    pub fn with_suffix(mut self, suffix: &'a str) -> Self {
        if self.suffix.is_empty() {
            self.suffix = suffix.into();
        } else {
            self.suffix += suffix;
        }
        self
    }
}

impl TryInto<String> for KeyExpr<'_> {
    type Error = zenoh_util::core::Error;
    fn try_into(self) -> Result<String, Self::Error> {
        if self.scope == 0 {
            Ok(self.suffix.into_owned())
        } else {
            bail!("Scoped key expression")
        }
    }
}

impl TryInto<ExprId> for KeyExpr<'_> {
    type Error = zenoh_util::core::Error;
    fn try_into(self) -> Result<ExprId, Self::Error> {
        self.try_as_id()
    }
}

impl fmt::Debug for KeyExpr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.scope == 0 {
            write!(f, "{}", self.suffix)
        } else {
            write!(f, "{}:{}", self.scope, self.suffix)
        }
    }
}

impl fmt::Display for KeyExpr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.scope == 0 {
            write!(f, "{}", self.suffix)
        } else {
            write!(f, "{}:{}", self.scope, self.suffix)
        }
    }
}

impl<'a> From<&KeyExpr<'a>> for KeyExpr<'a> {
    #[inline]
    fn from(key: &KeyExpr<'a>) -> KeyExpr<'a> {
        key.clone()
    }
}

impl From<ExprId> for KeyExpr<'_> {
    #[inline]
    fn from(rid: ExprId) -> KeyExpr<'static> {
        KeyExpr {
            scope: rid,
            suffix: "".into(),
        }
    }
}

impl From<&ExprId> for KeyExpr<'_> {
    #[inline]
    fn from(rid: &ExprId) -> KeyExpr<'static> {
        KeyExpr {
            scope: *rid,
            suffix: "".into(),
        }
    }
}

impl<'a> From<&'a str> for KeyExpr<'a> {
    #[inline]
    fn from(name: &'a str) -> KeyExpr<'a> {
        KeyExpr {
            scope: 0,
            suffix: name.into(),
        }
    }
}

impl From<String> for KeyExpr<'_> {
    #[inline]
    fn from(name: String) -> KeyExpr<'static> {
        KeyExpr {
            scope: 0,
            suffix: name.into(),
        }
    }
}

impl<'a> From<&'a String> for KeyExpr<'a> {
    #[inline]
    fn from(name: &'a String) -> KeyExpr<'a> {
        KeyExpr {
            scope: 0,
            suffix: name.into(),
        }
    }
}

/// The encoding of a zenoh [`Value`](crate::Value).
///
/// A zenoh encoding is a [`Mime`](http_types::Mime) type represented, for wire efficiency,
/// as an integer prefix (that maps to a string) and a string suffix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Encoding {
    pub prefix: ZInt,
    pub suffix: Cow<'static, str>,
}

mod encoding {
    lazy_static! {
        pub(super) static ref MIMES: [&'static str; 21] = [
            /*  0 */ "",
            /*  1 */ "application/octet-stream",
            /*  2 */ "application/custom", // non iana standard
            /*  3 */ "text/plain",
            /*  4 */ "application/properties", // non iana standard
            /*  5 */ "application/json", // if not readable from casual users
            /*  6 */ "application/sql",
            /*  7 */ "application/integer", // non iana standard
            /*  8 */ "application/float", // non iana standard
            /*  9 */ "application/xml", // if not readable from casual users (RFC 3023, section 3)
            /* 10 */ "application/xhtml+xml",
            /* 11 */ "application/x-www-form-urlencoded",
            /* 12 */ "text/json", // non iana standard - if readable from casual users
            /* 13 */ "text/html",
            /* 14 */ "text/xml", // if readable from casual users (RFC 3023, section 3)
            /* 15 */ "text/css",
            /* 16 */ "text/csv",
            /* 17 */ "text/javascript",
            /* 18 */ "image/jpeg",
            /* 19 */ "image/png",
            /* 20 */ "image/gif",
        ];
    }
}

impl Encoding {
    pub const EMPTY: Encoding = Encoding {
        prefix: 0,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const APP_OCTET_STREAM: Encoding = Encoding {
        prefix: 1,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const APP_CUSTOM: Encoding = Encoding {
        prefix: 2,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const TEXT_PLAIN: Encoding = Encoding {
        prefix: 3,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const STRING: Encoding = Encoding::TEXT_PLAIN;
    pub const APP_PROPERTIES: Encoding = Encoding {
        prefix: 4,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const APP_JSON: Encoding = Encoding {
        prefix: 5,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const APP_SQL: Encoding = Encoding {
        prefix: 6,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const APP_INTEGER: Encoding = Encoding {
        prefix: 7,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const APP_FLOAT: Encoding = Encoding {
        prefix: 8,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const APP_XML: Encoding = Encoding {
        prefix: 9,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const APP_XHTML_XML: Encoding = Encoding {
        prefix: 10,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const APP_X_WWW_FORM_URLENCODED: Encoding = Encoding {
        prefix: 11,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const TEXT_JSON: Encoding = Encoding {
        prefix: 12,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const TEXT_HTML: Encoding = Encoding {
        prefix: 13,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const TEXT_XML: Encoding = Encoding {
        prefix: 14,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const TEXT_CSS: Encoding = Encoding {
        prefix: 15,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const TEXT_CSV: Encoding = Encoding {
        prefix: 16,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const TEXT_JAVASCRIPT: Encoding = Encoding {
        prefix: 17,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const IMG_JPG: Encoding = Encoding {
        prefix: 18,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const IMG_PNG: Encoding = Encoding {
        prefix: 19,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const IMG_GIF: Encoding = Encoding {
        prefix: 20,
        suffix: std::borrow::Cow::Borrowed(""),
    };

    /// Converts the given encoding to [`Mime`](http_types::Mime).
    pub fn to_mime(&self) -> ZResult<Mime> {
        if self.prefix == 0 {
            Mime::from_str(self.suffix.as_ref()).map_err(|e| zerror!("{}", &e).into())
        } else if self.prefix <= encoding::MIMES.len() as ZInt {
            Mime::from_str(&format!(
                "{}{}",
                &encoding::MIMES[self.prefix as usize],
                self.suffix
            ))
            .map_err(|e| zerror!("{}", &e).into())
        } else {
            bail!("Unknown encoding prefix {}", self.prefix)
        }
    }

    /// Sets the suffix of this encoding.
    pub fn with_suffix<IntoCowStr>(mut self, suffix: IntoCowStr) -> Self
    where
        IntoCowStr: Into<Cow<'static, str>>,
    {
        self.suffix = suffix.into();
        self
    }

    /// Returns `true`if the string representation of this encoding starts with
    /// the string representation of ther given encoding.
    pub fn starts_with(&self, encoding: &Encoding) -> bool {
        (self.prefix == encoding.prefix && self.suffix.starts_with(encoding.suffix.as_ref()))
            || self.to_string().starts_with(&encoding.to_string())
    }
}

impl fmt::Display for Encoding {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.prefix > 0 && self.prefix < encoding::MIMES.len() as ZInt {
            write!(
                f,
                "{}{}",
                &encoding::MIMES[self.prefix as usize],
                self.suffix
            )
        } else {
            write!(f, "{}", self.suffix)
        }
    }
}

impl From<&'static str> for Encoding {
    fn from(s: &'static str) -> Self {
        for (i, v) in encoding::MIMES.iter().enumerate() {
            if i != 0 && s.starts_with(v) {
                return Encoding {
                    prefix: i as ZInt,
                    suffix: s.split_at(v.len()).1.into(),
                };
            }
        }
        Encoding {
            prefix: 0,
            suffix: s.into(),
        }
    }
}

impl<'a> From<String> for Encoding {
    fn from(s: String) -> Self {
        for (i, v) in encoding::MIMES.iter().enumerate() {
            if i != 0 && s.starts_with(v) {
                return Encoding {
                    prefix: i as ZInt,
                    suffix: s.split_at(v.len()).1.to_string().into(),
                };
            }
        }
        Encoding {
            prefix: 0,
            suffix: s.into(),
        }
    }
}

impl<'a> From<Mime> for Encoding {
    fn from(m: Mime) -> Self {
        Encoding::from(&m)
    }
}

impl<'a> From<&Mime> for Encoding {
    fn from(m: &Mime) -> Self {
        Encoding::from(m.essence().to_string())
    }
}

impl From<ZInt> for Encoding {
    fn from(i: ZInt) -> Self {
        Encoding {
            prefix: i,
            suffix: "".into(),
        }
    }
}

impl Default for Encoding {
    fn default() -> Self {
        Encoding::EMPTY
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    pub key: ZInt,
    pub value: Vec<u8>,
}

/// The global unique id of a zenoh peer.
#[derive(Clone, Copy, Eq)]
pub struct PeerId {
    size: usize,
    id: [u8; PeerId::MAX_SIZE],
}

impl PeerId {
    pub const MAX_SIZE: usize = 16;

    pub fn new(size: usize, id: [u8; PeerId::MAX_SIZE]) -> PeerId {
        PeerId { size, id }
    }

    #[inline]
    pub fn size(&self) -> usize {
        self.size
    }

    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        &self.id[..self.size]
    }

    pub fn rand() -> PeerId {
        PeerId::from(Uuid::new_v4())
    }
}

impl From<uuid::Uuid> for PeerId {
    #[inline]
    fn from(uuid: uuid::Uuid) -> Self {
        PeerId {
            size: 16,
            id: *uuid.as_bytes(),
        }
    }
}

impl FromStr for PeerId {
    type Err = zenoh_util::core::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let id = s.parse::<Uuid>().map_err(|e| zerror!(e))?;
        let pid = PeerId {
            size: 16,
            id: *id.as_bytes(),
        };
        Ok(pid)
    }
}

impl PartialEq for PeerId {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.size == other.size && self.as_slice() == other.as_slice()
    }
}

impl Hash for PeerId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state);
    }
}

impl fmt::Debug for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", hex::encode_upper(self.as_slice()))
    }
}

impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

// A PeerID can be converted into a Timestamp's ID
impl From<&PeerId> for uhlc::ID {
    fn from(pid: &PeerId) -> Self {
        uhlc::ID::new(pid.size, pid.id)
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(u8)]
pub enum Priority {
    Control = 0,
    RealTime = 1,
    InteractiveHigh = 2,
    InteractiveLow = 3,
    DataHigh = 4,
    Data = 5,
    DataLow = 6,
    Background = 7,
}

impl Priority {
    pub const NUM: usize = 8;
}

impl Default for Priority {
    fn default() -> Priority {
        Priority::Data
    }
}

impl TryFrom<u8> for Priority {
    type Error = zenoh_util::core::Error;

    fn try_from(conduit: u8) -> Result<Self, Self::Error> {
        match conduit {
            0 => Ok(Priority::Control),
            1 => Ok(Priority::RealTime),
            2 => Ok(Priority::InteractiveHigh),
            3 => Ok(Priority::InteractiveLow),
            4 => Ok(Priority::DataHigh),
            5 => Ok(Priority::Data),
            6 => Ok(Priority::DataLow),
            7 => Ok(Priority::Background),
            unknown => bail!(
                "{} is not a valid conduit value. Admitted values are [0-7].",
                unknown
            ),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(u8)]
pub enum Reliability {
    BestEffort,
    Reliable,
}

impl Default for Reliability {
    fn default() -> Reliability {
        Reliability::BestEffort
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Channel {
    pub priority: Priority,
    pub reliability: Reliability,
}

impl Default for Channel {
    fn default() -> Channel {
        Channel {
            priority: Priority::default(),
            reliability: Reliability::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConduitSnList {
    Plain(ConduitSn),
    QoS(Box<[ConduitSn; Priority::NUM]>),
}

impl fmt::Display for ConduitSnList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[ ")?;
        match self {
            ConduitSnList::Plain(sn) => {
                write!(
                    f,
                    "{:?} {{ reliable: {}, best effort: {} }}",
                    Priority::default(),
                    sn.reliable,
                    sn.best_effort
                )?;
            }
            ConduitSnList::QoS(ref sns) => {
                for (prio, sn) in sns.iter().enumerate() {
                    let p: Priority = (prio as u8).try_into().unwrap();
                    write!(
                        f,
                        "{:?} {{ reliable: {}, best effort: {} }}",
                        p, sn.reliable, sn.best_effort
                    )?;
                    if p != Priority::Background {
                        write!(f, ", ")?;
                    }
                }
            }
        }
        write!(f, " ]")
    }
}

/// The kind of reliability.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct ConduitSn {
    pub reliable: ZInt,
    pub best_effort: ZInt,
}

impl Default for ConduitSn {
    fn default() -> ConduitSn {
        ConduitSn {
            reliable: 0,
            best_effort: 0,
        }
    }
}

/// The kind of congestion control.
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(u8)]
pub enum CongestionControl {
    Block,
    Drop,
}

impl Default for CongestionControl {
    fn default() -> CongestionControl {
        CongestionControl::Drop
    }
}

/// The subscription mode.
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(u8)]
pub enum SubMode {
    Push,
    Pull,
}

impl Default for SubMode {
    #[inline]
    fn default() -> Self {
        SubMode::Push
    }
}

/// A time period.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Period {
    pub origin: ZInt,
    pub period: ZInt,
    pub duration: ZInt,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubInfo {
    pub reliability: Reliability,
    pub mode: SubMode,
    pub period: Option<Period>,
}

impl Default for SubInfo {
    fn default() -> SubInfo {
        SubInfo {
            reliability: Reliability::default(),
            mode: SubMode::default(),
            period: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueryableInfo {
    pub complete: ZInt,
    pub distance: ZInt,
}

impl Default for QueryableInfo {
    fn default() -> QueryableInfo {
        QueryableInfo {
            complete: 1,
            distance: 0,
        }
    }
}

pub mod queryable {
    pub const ALL_KINDS: super::ZInt = 0x01;
    pub const STORAGE: super::ZInt = 0x02;
    pub const EVAL: super::ZInt = 0x04;
}

/// The kind of consolidation.
#[derive(Debug, Clone, PartialEq, Copy)]
#[repr(u8)]
pub enum ConsolidationMode {
    None,
    Lazy,
    Full,
}

/// The kind of consolidation that should be applied on replies to a [`get`](crate::Session::get)
/// at different stages of the reply process.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryConsolidation {
    pub first_routers: ConsolidationMode,
    pub last_router: ConsolidationMode,
    pub reception: ConsolidationMode,
}

impl QueryConsolidation {
    pub fn none() -> Self {
        Self {
            first_routers: ConsolidationMode::None,
            last_router: ConsolidationMode::None,
            reception: ConsolidationMode::None,
        }
    }
}

impl Default for QueryConsolidation {
    fn default() -> Self {
        Self {
            first_routers: ConsolidationMode::Lazy,
            last_router: ConsolidationMode::Lazy,
            reception: ConsolidationMode::Full,
        }
    }
}

/// The [`Queryable`](crate::queryable::Queryable)s that should be target of a [`get`](crate::Session::get).
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    BestMatching,
    All,
    AllComplete,
    None,
    #[cfg(feature = "complete_n")]
    Complete(ZInt),
}

impl Default for Target {
    fn default() -> Self {
        Target::BestMatching
    }
}

/// The [`Queryable`](crate::queryable::Queryable)s that should be target of a [`get`](crate::Session::get).
#[derive(Debug, Clone, PartialEq)]
pub struct QueryTarget {
    pub kind: ZInt,
    pub target: Target,
}

impl Default for QueryTarget {
    fn default() -> Self {
        QueryTarget {
            kind: queryable::ALL_KINDS,
            target: Target::default(),
        }
    }
}
