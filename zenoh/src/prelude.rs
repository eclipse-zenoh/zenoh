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

//! A "prelude" for crates using the `zenoh` crate.
//!
//! This prelude is similar to the standard library's prelude in that you'll
//! almost always want to import its entire contents, but unlike the standard
//! library's prelude you'll have to do so manually. An example of using this is:
//!
//! ```
//! use zenoh::prelude::*;
//! ```

#[cfg(feature = "shared-memory")]
use crate::buf::SharedMemoryBuf;
use crate::buf::ZBuf;
use crate::data_kind;
use crate::net::protocol::message::DataInfo;
use crate::queryable::Query;
use crate::time::{new_reception_timestamp, Timestamp};
#[cfg(feature = "shared-memory")]
use async_std::sync::Arc;
use regex::Regex;
use std::convert::TryFrom;
use std::fmt;

pub(crate) type Id = usize;

pub use crate::config;
pub use crate::properties::Properties;
pub use crate::sync::channel::Receiver;
pub use crate::sync::ZFuture;

/// A [`Locator`] contains a choice of protocol, an address and port, as well as optional additional properties to work with.
pub use crate::net::link::Locator;

/// The encoding of a zenoh [`Value`].
pub use super::net::protocol::core::Encoding;

/// The global unique id of a zenoh peer.
pub use super::net::protocol::core::PeerId;

/// A numerical Id mapped to a key expression with [`declare_expr`](Session::declare_expr).
pub use super::net::protocol::core::ExprId;

/// A key expression.
pub use super::net::protocol::core::KeyExpr;

/// A zenoh integer.
pub use super::net::protocol::core::ZInt;

/// A zenoh Value.
#[derive(Clone)]
pub struct Value {
    /// The payload of this Value.
    pub payload: ZBuf,

    /// An encoding description indicating how the associated payload is encoded.
    pub encoding: Encoding,
}

impl Value {
    /// Creates a new zenoh Value.
    pub fn new(payload: ZBuf) -> Self {
        Value {
            payload,
            encoding: Encoding::APP_OCTET_STREAM,
        }
    }

    /// Creates an empty Value.
    pub fn empty() -> Self {
        Value {
            payload: ZBuf::new(),
            encoding: Encoding::APP_OCTET_STREAM,
        }
    }

    /// Sets the encoding of this zenoh Value.
    #[inline(always)]
    pub fn encoding(mut self, encoding: Encoding) -> Self {
        self.encoding = encoding;
        self
    }

    pub fn as_json(&self) -> Option<serde_json::Value> {
        if [Encoding::APP_JSON.prefix, Encoding::TEXT_JSON.prefix].contains(&self.encoding.prefix) {
            serde::Deserialize::deserialize(&mut serde_json::Deserializer::from_slice(
                self.payload.contiguous().as_slice(),
            ))
            .ok()
        } else {
            None
        }
    }

    pub fn as_integer(&self) -> Option<i64> {
        if self.encoding.prefix == Encoding::APP_INTEGER.prefix {
            std::str::from_utf8(self.payload.contiguous().as_slice())
                .ok()?
                .parse()
                .ok()
        } else {
            None
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        if self.encoding.prefix == Encoding::APP_FLOAT.prefix {
            std::str::from_utf8(self.payload.contiguous().as_slice())
                .ok()?
                .parse()
                .ok()
        } else {
            None
        }
    }

    pub fn as_properties(&self) -> Option<Properties> {
        if self.encoding.prefix == Encoding::APP_PROPERTIES.prefix {
            Some(Properties::from(
                std::str::from_utf8(self.payload.contiguous().as_slice()).ok()?,
            ))
        } else {
            None
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Value{{ payload: {}, encoding: {} }}",
            self.payload, self.encoding
        )
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            String::from_utf8(self.payload.to_vec())
                .unwrap_or_else(|_| base64::encode(self.payload.to_vec()))
        )
    }
}

impl From<ZBuf> for Value {
    fn from(buf: ZBuf) -> Self {
        Value {
            payload: buf,
            encoding: Encoding::APP_OCTET_STREAM,
        }
    }
}

#[cfg(feature = "shared-memory")]
impl From<Arc<SharedMemoryBuf>> for Value {
    fn from(smb: Arc<SharedMemoryBuf>) -> Self {
        Value {
            payload: smb.into(),
            encoding: Encoding::APP_OCTET_STREAM,
        }
    }
}

#[cfg(feature = "shared-memory")]
impl From<Box<SharedMemoryBuf>> for Value {
    fn from(smb: Box<SharedMemoryBuf>) -> Self {
        Value {
            payload: smb.into(),
            encoding: Encoding::APP_OCTET_STREAM,
        }
    }
}

#[cfg(feature = "shared-memory")]
impl From<SharedMemoryBuf> for Value {
    fn from(smb: SharedMemoryBuf) -> Self {
        Value {
            payload: smb.into(),
            encoding: Encoding::APP_OCTET_STREAM,
        }
    }
}

impl From<Vec<u8>> for Value {
    fn from(buf: Vec<u8>) -> Self {
        Value::from(ZBuf::from(buf))
    }
}

impl From<&[u8]> for Value {
    fn from(buf: &[u8]) -> Self {
        Value::from(ZBuf::from(buf.to_vec()))
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value {
            payload: ZBuf::from(s.into_bytes()),
            encoding: Encoding::STRING,
        }
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value {
            payload: ZBuf::from(s.as_bytes().to_vec()),
            encoding: Encoding::STRING,
        }
    }
}

impl From<Properties> for Value {
    fn from(p: Properties) -> Self {
        Value {
            payload: ZBuf::from(p.to_string().as_bytes().to_vec()),
            encoding: Encoding::APP_PROPERTIES,
        }
    }
}

impl From<&serde_json::Value> for Value {
    fn from(json: &serde_json::Value) -> Self {
        Value {
            payload: ZBuf::from(json.to_string().as_bytes().to_vec()),
            encoding: Encoding::APP_JSON,
        }
    }
}

impl From<serde_json::Value> for Value {
    fn from(json: serde_json::Value) -> Self {
        Value::from(&json)
    }
}

impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Value {
            payload: ZBuf::from(i.to_string().as_bytes().to_vec()),
            encoding: Encoding::APP_INTEGER,
        }
    }
}

impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Value {
            payload: ZBuf::from(f.to_string().as_bytes().to_vec()),
            encoding: Encoding::APP_FLOAT,
        }
    }
}

/// Informations on the source of a zenoh [`Sample`].
#[derive(Debug, Clone)]
pub struct SourceInfo {
    /// The [`PeerId`] of the zenoh instance that published the concerned [`Sample`].
    pub source_id: Option<PeerId>,
    /// The sequence number of the [`Sample`] from the source.
    pub source_sn: Option<ZInt>,
    /// The [`PeerId`] of the first zenoh router that routed this [`Sample`].
    pub first_router_id: Option<PeerId>,
    /// The sequence number of the [`Sample`] from the first zenoh router that routed it.
    pub first_router_sn: Option<ZInt>,
}

impl SourceInfo {
    pub(crate) fn empty() -> Self {
        SourceInfo {
            source_id: None,
            source_sn: None,
            first_router_id: None,
            first_router_sn: None,
        }
    }
}

impl From<DataInfo> for SourceInfo {
    fn from(data_info: DataInfo) -> Self {
        SourceInfo {
            source_id: data_info.source_id,
            source_sn: data_info.source_sn,
            first_router_id: data_info.first_router_id,
            first_router_sn: data_info.first_router_sn,
        }
    }
}

impl From<Option<DataInfo>> for SourceInfo {
    fn from(data_info: Option<DataInfo>) -> Self {
        match data_info {
            Some(data_info) => data_info.into(),
            None => SourceInfo::empty(),
        }
    }
}

/// The kind of a [`Sample`].
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum SampleKind {
    /// if the [`Sample`] was caused by a `put` operation.
    Put = data_kind::PUT as isize,
    /// if the [`Sample`] was caused by a `patch` operation.
    Patch = data_kind::PATCH as isize,
    /// if the [`Sample`] was caused by a `delete` operation.
    Delete = data_kind::DELETE as isize,
}

impl Default for SampleKind {
    fn default() -> Self {
        SampleKind::Put
    }
}

impl fmt::Display for SampleKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SampleKind::Put => write!(f, "PUT"),
            SampleKind::Patch => write!(f, "PATCH"),
            SampleKind::Delete => write!(f, "DELETE"),
        }
    }
}

impl From<ZInt> for SampleKind {
    fn from(kind: ZInt) -> Self {
        match kind {
            data_kind::PUT => SampleKind::Put,
            data_kind::PATCH => SampleKind::Patch,
            data_kind::DELETE => SampleKind::Delete,
            _ => {
                log::warn!(
                    "Received DataInfo with kind={} which doesn't correspond to a SampleKind. \
                       Assume a PUT with RAW encoding",
                    kind
                );
                SampleKind::Put
            }
        }
    }
}

/// A zenoh sample.
#[derive(Clone, Debug)]
pub struct Sample {
    // The key expression on which this Sample was published.
    pub key_expr: KeyExpr<'static>,
    /// The value of this Sample.
    pub value: Value,
    // The kind of this Sample.
    pub kind: SampleKind,
    // The [`Timestamp`] of this Sample.
    pub timestamp: Option<Timestamp>,
    // Infos on the source of this Sample.
    pub source_info: SourceInfo,
}

impl Sample {
    /// Creates a new Sample.
    #[inline]
    pub fn new<IntoKeyExpr, IntoValue>(key_expr: IntoKeyExpr, value: IntoValue) -> Self
    where
        IntoKeyExpr: Into<KeyExpr<'static>>,
        IntoValue: Into<Value>,
    {
        Sample {
            key_expr: key_expr.into(),
            value: value.into(),
            kind: SampleKind::default(),
            timestamp: None,
            source_info: SourceInfo::empty(),
        }
    }

    #[inline]
    pub(crate) fn with_info(
        key_expr: KeyExpr<'static>,
        payload: ZBuf,
        data_info: Option<DataInfo>,
    ) -> Self {
        let mut value: Value = payload.into();
        if let Some(data_info) = data_info {
            if let Some(encoding) = &data_info.encoding {
                value.encoding = encoding.clone();
            }
            Sample {
                key_expr,
                value,
                kind: data_info.kind.unwrap_or(data_kind::DEFAULT).into(),
                timestamp: data_info.timestamp,
                source_info: data_info.into(),
            }
        } else {
            Sample {
                key_expr,
                value,
                kind: SampleKind::default(),
                timestamp: None,
                source_info: SourceInfo::empty(),
            }
        }
    }

    #[inline]
    pub(crate) fn split(self) -> (KeyExpr<'static>, ZBuf, DataInfo) {
        let info = DataInfo {
            kind: None,
            encoding: Some(self.value.encoding),
            timestamp: self.timestamp,
            #[cfg(feature = "shared-memory")]
            sliced: false,
            source_id: self.source_info.source_id,
            source_sn: self.source_info.source_sn,
            first_router_id: self.source_info.first_router_id,
            first_router_sn: self.source_info.first_router_sn,
        };
        (self.key_expr, self.value.payload, info)
    }

    /// Gets the timestamp of this Sample.
    #[inline]
    pub fn get_timestamp(&self) -> Option<&Timestamp> {
        self.timestamp.as_ref()
    }

    /// Sets the timestamp of this Sample.
    #[inline]
    pub fn with_timestamp(mut self, timestamp: Timestamp) -> Self {
        self.timestamp = Some(timestamp);
        self
    }

    /// Sets the source info of this Sample.
    #[inline]
    pub fn with_source_info(mut self, source_info: SourceInfo) -> Self {
        self.source_info = source_info;
        self
    }

    #[inline]
    /// Ensure that an associated Timestamp is present in this Sample.
    /// If not, a new one is created with the current system time and 0x00 as id.
    pub fn ensure_timestamp(&mut self) {
        if self.timestamp.is_none() {
            self.timestamp = Some(new_reception_timestamp());
        }
    }
}

impl fmt::Display for Sample {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            SampleKind::Delete => write!(f, "{}({})", self.kind, self.key_expr),
            _ => write!(f, "{}({}: {})", self.kind, self.key_expr, self.value),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Priority {
    RealTime = 1,
    InteractiveHigh = 2,
    InteractiveLow = 3,
    DataHigh = 4,
    Data = 5,
    DataLow = 6,
    Background = 7,
}

impl Default for Priority {
    fn default() -> Priority {
        Priority::Data
    }
}

impl From<Priority> for super::net::protocol::core::Priority {
    fn from(prio: Priority) -> Self {
        match prio {
            Priority::RealTime => super::net::protocol::core::Priority::RealTime,
            Priority::InteractiveHigh => super::net::protocol::core::Priority::InteractiveHigh,
            Priority::InteractiveLow => super::net::protocol::core::Priority::InteractiveLow,
            Priority::DataHigh => super::net::protocol::core::Priority::DataHigh,
            Priority::Data => super::net::protocol::core::Priority::Data,
            Priority::DataLow => super::net::protocol::core::Priority::DataLow,
            Priority::Background => super::net::protocol::core::Priority::Background,
        }
    }
}

/// The "starttime" property key for time-range selection
pub const PROP_STARTTIME: &str = "starttime";
/// The "stoptime" property key for time-range selection
pub const PROP_STOPTIME: &str = "stoptime";

#[derive(Clone, Debug, PartialEq)]
/// An expression identifying a selection of resources.
///
/// A selector is the conjunction of an key expression identifying a set
/// of resource keys and a value selector filtering out the resource values.
///
/// Structure of a selector:
/// ```text
/// /s1/s2/..../sn?x>1&y<2&...&z=4(p1=v1;p2=v2;...;pn=vn)#a;b;x;y;...;z
/// |key_selector||---------------- value_selector -------------------|
///                |--- filter --| |---- properties ---|  |  fragment |
/// ```
/// where:
///  * __key_selector__: an expression identifying a set of Resources.
///  * __filter__: a list of `value_selectors` separated by `'&'` allowing to perform filtering on the values
///    associated with the matching keys. Each `value_selector` has the form "`field`-`operator`-`value`" value where:
///      * _field_ is the name of a field in the value (is applicable and is existing. otherwise the `value_selector` is false)
///      * _operator_ is one of a comparison operators: `<` , `>` , `<=` , `>=` , `=` , `!=`
///      * _value_ is the the value to compare the field’s value with
///  * __fragment__: a list of fields names allowing to return a sub-part of each value.
///    This feature only applies to structured values using a “self-describing” encoding, such as JSON or XML.
///    It allows to select only some fields within the structure. A new structure with only the selected fields
///    will be used in place of the original value.
///
/// _**NOTE**_: _the filters and fragments are not yet supported in current zenoh version._
pub struct Selector<'a> {
    /// The part of this selector identifying which keys should be part of the selection.
    /// I.e. all characters before `?`.
    pub key_selector: KeyExpr<'a>,
    /// the part of this selector identifying which values should be part of the selection.
    /// I.e. all characters starting from `?`.
    pub value_selector: &'a str,
}

impl<'a> Selector<'a> {
    /// Sets the `value_selector` part of this `Selector`.
    #[inline(always)]
    pub fn with_value_selector(mut self, value_selector: &'a str) -> Self {
        self.value_selector = value_selector;
        self
    }

    /// Parses the `value_selector` part of this `Selector`.
    pub fn parse_value_selector(&self) -> crate::Result<ValueSelector<'a>> {
        ValueSelector::try_from(self.value_selector)
    }

    /// Returns true if the `Selector` specifies a time-range in its properties
    /// (i.e. using `"starttime"` or `"stoptime"`)
    pub fn has_time_range(&self) -> bool {
        match ValueSelector::try_from(self.value_selector) {
            Ok(value_selector) => value_selector.has_time_range(),
            _ => false,
        }
    }
}

impl fmt::Display for Selector<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.key_selector, self.value_selector)
    }
}

impl<'a> From<&Selector<'a>> for Selector<'a> {
    fn from(s: &Selector<'a>) -> Self {
        s.clone()
    }
}

impl<'a> From<&'a str> for Selector<'a> {
    fn from(s: &'a str) -> Self {
        let (key_selector, value_selector) =
            s.find(|c| c == '?').map_or((s, ""), |i| s.split_at(i));
        Selector {
            key_selector: key_selector.into(),
            value_selector,
        }
    }
}

impl<'a> From<&'a String> for Selector<'a> {
    fn from(s: &'a String) -> Self {
        Self::from(s.as_str())
    }
}

impl<'a> From<&'a Query> for Selector<'a> {
    fn from(q: &'a Query) -> Self {
        Selector {
            key_selector: q.key_selector.clone(),
            value_selector: &q.value_selector,
        }
    }
}

impl<'a> From<&KeyExpr<'a>> for Selector<'a> {
    fn from(key_selector: &KeyExpr<'a>) -> Self {
        Self {
            key_selector: key_selector.clone(),
            value_selector: "",
        }
    }
}

impl<'a> From<KeyExpr<'a>> for Selector<'a> {
    fn from(key_selector: KeyExpr<'a>) -> Self {
        Self {
            key_selector,
            value_selector: "",
        }
    }
}

/// A struct that can be used to help decoding or encoding the `value_selector` part of a [`Query`](crate::queryable::Query).
///
/// # Examples
/// ```
/// use std::convert::TryInto;
/// use zenoh::prelude::*;
///
/// let value_selector: ValueSelector = "?x>1&y<2&z=4(p1=v1;p2=v2;pn=vn)[a;b;x;y;z]".try_into().unwrap();
/// assert_eq!(value_selector.filter, "x>1&y<2&z=4");
/// assert_eq!(value_selector.properties.get("p2").unwrap().as_str(), "v2");
/// assert_eq!(value_selector.fragment, "a;b;x;y;z");
/// ```
///
/// ```no_run
/// # async_std::task::block_on(async {
/// # use futures::prelude::*;
/// # use zenoh::prelude::*;
/// # let session = zenoh::open(config::peer()).await.unwrap();
///
/// use std::convert::TryInto;
///
/// let mut queryable = session.queryable("/key/expression").await.unwrap();
/// while let Some(query) = queryable.receiver().next().await {
///     let value_selector = query.selector().parse_value_selector().unwrap();
///     println!("filter: {}", value_selector.filter);
///     println!("properties: {}", value_selector.properties);
///     println!("fragment: {}", value_selector.fragment);
/// }
/// # })
/// ```
///
/// ```
/// # async_std::task::block_on(async {
/// # use futures::prelude::*;
/// # use zenoh::prelude::*;
/// # let session = zenoh::open(config::peer()).await.unwrap();
/// # let mut properties = Properties::default();
///
/// let value_selector = ValueSelector::empty()
///     .with_filter("x>1&y<2")
///     .with_properties(properties)
///     .with_fragment("x;y");
///
/// let mut replies = session.get(
///     &Selector::from("/key/expression").with_value_selector(&value_selector.to_string())
/// ).await.unwrap();
/// # })
/// ```
#[derive(Debug, Clone)]
pub struct ValueSelector<'a> {
    /// the filter part of this `ValueSelector`, if any (all characters after `?` and before `(` or `[`)
    pub filter: &'a str,
    /// the properties part of this `ValueSelector`) (all characters between ``( )`` and after `?`)
    pub properties: Properties,
    /// the fragment part of this `ValueSelector`, if any (all characters between ``[ ]`` and after `?`)
    pub fragment: &'a str,
}

impl<'a> ValueSelector<'a> {
    /// Creates a new `ValueSelector`.
    pub fn new(filter: &'a str, properties: Properties, fragment: &'a str) -> Self {
        ValueSelector {
            filter,
            properties,
            fragment,
        }
    }

    /// Creates an empty `ValueSelector`.
    pub fn empty() -> Self {
        ValueSelector::new("", Properties::default(), "")
    }

    /// Sets the filter part of this `ValueSelector`.
    pub fn with_filter(mut self, filter: &'a str) -> Self {
        self.filter = filter;
        self
    }

    /// Sets the properties part of this `ValueSelector`.
    pub fn with_properties(mut self, properties: Properties) -> Self {
        self.properties = properties;
        self
    }

    /// Sets the fragment part of this `ValueSelector`.
    pub fn with_fragment(mut self, fragment: &'a str) -> Self {
        self.fragment = fragment;
        self
    }

    /// Returns true if the `ValueSelector` specifies a time-range in its properties
    /// (i.e. using `"starttime"` or `"stoptime"`)
    pub fn has_time_range(&self) -> bool {
        self.properties.contains_key(PROP_STARTTIME) || self.properties.contains_key(PROP_STOPTIME)
    }
}

impl fmt::Display for ValueSelector<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut s = "?".to_string();
        s.push_str(self.filter);
        if !self.properties.is_empty() {
            s.push('(');
            s.push_str(self.properties.to_string().as_str());
            s.push(')');
        }
        if !self.fragment.is_empty() {
            s.push('[');
            s.push_str(self.fragment);
            s.push(']');
        }
        write!(f, "{}", s)
    }
}

impl<'a> TryFrom<&'a str> for ValueSelector<'a> {
    type Error = zenoh_util::core::Error;

    fn try_from(s: &'a str) -> crate::Result<Self> {
        const REGEX_PROJECTION: &str = r"[^\[\]\(\)\[\]]+";
        const REGEX_PROPERTIES: &str = ".*";
        const REGEX_FRAGMENT: &str = ".*";

        lazy_static! {
            static ref RE: Regex = Regex::new(&format!(
                "(?:\\?(?P<proj>{})?(?:\\((?P<prop>{})\\))?)?(?:\\[(?P<frag>{})\\])?",
                REGEX_PROJECTION, REGEX_PROPERTIES, REGEX_FRAGMENT
            ))
            .unwrap();
        }

        if let Some(caps) = RE.captures(s) {
            Ok(ValueSelector {
                filter: caps.name("proj").map_or("", |s| s.as_str()),
                properties: caps
                    .name("prop")
                    .map(|s| s.as_str().into())
                    .unwrap_or_default(),
                fragment: caps.name("frag").map_or("", |s| s.as_str()),
            })
        } else {
            bail!("invalid selector: {}", &s)
        }
    }
}

impl<'a> TryFrom<&'a String> for ValueSelector<'a> {
    type Error = zenoh_util::core::Error;

    fn try_from(s: &'a String) -> crate::Result<Self> {
        ValueSelector::try_from(s.as_str())
    }
}
