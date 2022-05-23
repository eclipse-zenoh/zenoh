//
// Copyright (c) 2022 ZettaScale Technology
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

//! A "prelude" for crates using the `zenoh` crate.
//!
//! This prelude is similar to the standard library's prelude in that you'll
//! almost always want to import its entire contents, but unlike the standard
//! library's prelude you'll have to do so manually. An example of using this is:
//!
//! ```
//! use zenoh::prelude::*;
//! ```

pub mod sync {
    pub use super::*;
    pub use zenoh_core::SyncResolve;
}
pub mod r#async {
    pub use super::*;
    pub use zenoh_core::AsyncResolve;
}

#[cfg(feature = "shared-memory")]
use crate::buf::SharedMemoryBuf;
use crate::buf::ZBuf;
use crate::data_kind;
use crate::publication::PublishBuilder;
use crate::queryable::{Query, QueryableBuilder};
use crate::subscriber::SubscriberBuilder;
use crate::time::{new_reception_timestamp, Timestamp};
use regex::Regex;
use std::borrow::Cow;
use std::convert::TryFrom;
use std::fmt;
use std::ops::{Deref, DerefMut};
#[cfg(feature = "shared-memory")]
use std::sync::Arc;
pub use zenoh_buffers::SplitBuffer;
use zenoh_core::bail;
use zenoh_core::zresult::ZError;
pub use zenoh_protocol::io::{WBufCodec, ZBufCodec};
use zenoh_protocol::proto::DataInfo;
pub use zenoh_protocol::proto::{MessageReader, MessageWriter};

pub(crate) type Id = usize;

pub use crate::config;
pub use crate::properties::Properties;
pub use zenoh_config::ValidatedMap;

/// A [`Locator`] contains a choice of protocol, an address and port, as well as optional additional properties to work with.
pub use zenoh_protocol_core::Locator;

/// The encoding of a zenoh [`Value`].
pub use zenoh_protocol_core::{Encoding, KnownEncoding};

/// The global unique id of a zenoh peer.
pub use zenoh_protocol_core::ZenohId;

/// A numerical Id mapped to a key expression with [`declare_expr`](crate::Session::declare_expr).
pub use zenoh_protocol_core::ExprId;

/// A key expression.
pub use zenoh_protocol_core::KeyExpr;

/// A zenoh integer.
pub use zenoh_protocol_core::ZInt;

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
            encoding: KnownEncoding::AppOctetStream.into(),
        }
    }

    /// Creates an empty Value.
    pub fn empty() -> Self {
        Value {
            payload: ZBuf::default(),
            encoding: KnownEncoding::AppOctetStream.into(),
        }
    }

    /// Sets the encoding of this zenoh Value.
    #[inline(always)]
    pub fn encoding(mut self, encoding: Encoding) -> Self {
        self.encoding = encoding;
        self
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
        let payload = self.payload.contiguous();
        write!(
            f,
            "{}",
            String::from_utf8(payload.clone().into_owned())
                .unwrap_or_else(|_| base64::encode(payload))
        )
    }
}

// Shared memory conversion
#[cfg(feature = "shared-memory")]
impl From<Arc<SharedMemoryBuf>> for Value {
    fn from(smb: Arc<SharedMemoryBuf>) -> Self {
        Value {
            payload: smb.into(),
            encoding: KnownEncoding::AppOctetStream.into(),
        }
    }
}

#[cfg(feature = "shared-memory")]
impl From<Box<SharedMemoryBuf>> for Value {
    fn from(smb: Box<SharedMemoryBuf>) -> Self {
        Value {
            payload: smb.into(),
            encoding: KnownEncoding::AppOctetStream.into(),
        }
    }
}

#[cfg(feature = "shared-memory")]
impl From<SharedMemoryBuf> for Value {
    fn from(smb: SharedMemoryBuf) -> Self {
        Value {
            payload: smb.into(),
            encoding: KnownEncoding::AppOctetStream.into(),
        }
    }
}

// Bytes conversion
impl From<ZBuf> for Value {
    fn from(buf: ZBuf) -> Self {
        Value {
            payload: buf,
            encoding: KnownEncoding::AppOctetStream.into(),
        }
    }
}

impl TryFrom<&Value> for ZBuf {
    type Error = ZError;

    fn try_from(v: &Value) -> Result<Self, Self::Error> {
        match v.encoding.prefix() {
            KnownEncoding::AppOctetStream => Ok(v.payload.clone()),
            unexpected => Err(zerror!(
                "{:?} can not be converted into Cow<'a, [u8]>",
                unexpected
            )),
        }
    }
}

impl TryFrom<Value> for ZBuf {
    type Error = ZError;

    fn try_from(v: Value) -> Result<Self, Self::Error> {
        Self::try_from(&v)
    }
}

impl From<&[u8]> for Value {
    fn from(buf: &[u8]) -> Self {
        Value::from(ZBuf::from(buf.to_vec()))
    }
}

impl<'a> TryFrom<&'a Value> for Cow<'a, [u8]> {
    type Error = ZError;

    fn try_from(v: &'a Value) -> Result<Self, Self::Error> {
        match v.encoding.prefix() {
            KnownEncoding::AppOctetStream => Ok(v.payload.contiguous()),
            unexpected => Err(zerror!(
                "{:?} can not be converted into Cow<'a, [u8]>",
                unexpected
            )),
        }
    }
}

impl From<Vec<u8>> for Value {
    fn from(buf: Vec<u8>) -> Self {
        Value::from(ZBuf::from(buf))
    }
}

impl TryFrom<&Value> for Vec<u8> {
    type Error = ZError;

    fn try_from(v: &Value) -> Result<Self, Self::Error> {
        match v.encoding.prefix() {
            KnownEncoding::AppOctetStream => Ok(v.payload.contiguous().to_vec()),
            unexpected => Err(zerror!(
                "{:?} can not be converted into Vec<u8>",
                unexpected
            )),
        }
    }
}

impl TryFrom<Value> for Vec<u8> {
    type Error = ZError;

    fn try_from(v: Value) -> Result<Self, Self::Error> {
        Self::try_from(&v)
    }
}

// String conversion
impl From<String> for Value {
    fn from(s: String) -> Self {
        Value {
            payload: ZBuf::from(s.into_bytes()),
            encoding: KnownEncoding::TextPlain.into(),
        }
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value {
            payload: ZBuf::from(Vec::<u8>::from(s)),
            encoding: KnownEncoding::TextPlain.into(),
        }
    }
}

impl TryFrom<&Value> for String {
    type Error = ZError;

    fn try_from(v: &Value) -> Result<Self, Self::Error> {
        match v.encoding.prefix() {
            KnownEncoding::TextPlain => {
                String::from_utf8(v.payload.contiguous().to_vec()).map_err(|e| zerror!("{}", e))
            }
            unexpected => Err(zerror!("{:?} can not be converted into String", unexpected)),
        }
    }
}

impl TryFrom<Value> for String {
    type Error = ZError;

    fn try_from(v: Value) -> Result<Self, Self::Error> {
        Self::try_from(&v)
    }
}

// Sample conversion
impl From<Sample> for Value {
    fn from(s: Sample) -> Self {
        s.value
    }
}

// i64 conversion
impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Value {
            payload: ZBuf::from(Vec::<u8>::from(i.to_string())),
            encoding: KnownEncoding::AppInteger.into(),
        }
    }
}

impl TryFrom<&Value> for i64 {
    type Error = ZError;

    fn try_from(v: &Value) -> Result<Self, Self::Error> {
        match v.encoding.prefix() {
            KnownEncoding::AppInteger => std::str::from_utf8(&v.payload.contiguous())
                .map_err(|e| zerror!("{}", e))?
                .parse()
                .map_err(|e| zerror!("{}", e)),
            unexpected => Err(zerror!("{:?} can not be converted into i64", unexpected)),
        }
    }
}

impl TryFrom<Value> for i64 {
    type Error = ZError;

    fn try_from(v: Value) -> Result<Self, Self::Error> {
        Self::try_from(&v)
    }
}

// f64 conversion
impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Value {
            payload: ZBuf::from(f.to_string().as_bytes().to_vec()),
            encoding: KnownEncoding::AppFloat.into(),
        }
    }
}

impl TryFrom<&Value> for f64 {
    type Error = ZError;

    fn try_from(v: &Value) -> Result<Self, Self::Error> {
        match v.encoding.prefix() {
            KnownEncoding::AppInteger => std::str::from_utf8(&v.payload.contiguous())
                .map_err(|e| zerror!("{}", e))?
                .parse()
                .map_err(|e| zerror!("{}", e)),
            unexpected => Err(zerror!("{:?} can not be converted into f64", unexpected)),
        }
    }
}

impl TryFrom<Value> for f64 {
    type Error = ZError;

    fn try_from(v: Value) -> Result<Self, Self::Error> {
        Self::try_from(&v)
    }
}

// JSON conversion
impl From<&serde_json::Value> for Value {
    fn from(json: &serde_json::Value) -> Self {
        Value {
            payload: ZBuf::from(Vec::<u8>::from(json.to_string())),
            encoding: KnownEncoding::AppJson.into(),
        }
    }
}

impl From<serde_json::Value> for Value {
    fn from(json: serde_json::Value) -> Self {
        Value::from(&json)
    }
}

impl TryFrom<&Value> for serde_json::Value {
    type Error = ZError;

    fn try_from(v: &Value) -> Result<Self, Self::Error> {
        match v.encoding.prefix() {
            KnownEncoding::AppJson | KnownEncoding::TextJson => {
                let r = serde::Deserialize::deserialize(&mut serde_json::Deserializer::from_slice(
                    &v.payload.contiguous(),
                ));
                r.map_err(|e| zerror!("{}", e))
            }
            unexpected => Err(zerror!(
                "{:?} can not be converted into Properties",
                unexpected
            )),
        }
    }
}

impl TryFrom<Value> for serde_json::Value {
    type Error = ZError;

    fn try_from(v: Value) -> Result<Self, Self::Error> {
        Self::try_from(&v)
    }
}

// Properties conversion
impl From<Properties> for Value {
    fn from(p: Properties) -> Self {
        Value {
            payload: ZBuf::from(Vec::<u8>::from(p.to_string())),
            encoding: KnownEncoding::AppProperties.into(),
        }
    }
}

impl TryFrom<&Value> for Properties {
    type Error = ZError;

    fn try_from(v: &Value) -> Result<Self, Self::Error> {
        match *v.encoding.prefix() {
            KnownEncoding::AppProperties => Ok(Properties::from(
                std::str::from_utf8(&v.payload.contiguous()).map_err(|e| zerror!("{}", e))?,
            )),
            unexpected => Err(zerror!(
                "{:?} can not be converted into Properties",
                unexpected
            )),
        }
    }
}

impl TryFrom<Value> for Properties {
    type Error = ZError;

    fn try_from(v: Value) -> Result<Self, Self::Error> {
        Self::try_from(&v)
    }
}

/// Informations on the source of a zenoh [`Sample`].
#[derive(Debug, Clone)]
pub struct SourceInfo {
    /// The [`ZenohId`] of the zenoh instance that published the concerned [`Sample`].
    pub source_id: Option<ZenohId>,
    /// The sequence number of the [`Sample`] from the source.
    pub source_sn: Option<ZInt>,
    /// The [`ZenohId`] of the first zenoh router that routed this [`Sample`].
    pub first_router_id: Option<ZenohId>,
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
    /// The key expression on which this Sample was published.
    pub key_expr: KeyExpr<'static>,
    /// The value of this Sample.
    pub value: Value,
    /// The kind of this Sample.
    pub kind: SampleKind,
    /// The [`Timestamp`] of this Sample.
    pub timestamp: Option<Timestamp>,
    /// Infos on the source of this Sample.
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
            kind: if self.kind == SampleKind::Put {
                None
            } else {
                Some(self.kind as u64)
            },
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

impl Deref for Sample {
    type Target = Value;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl DerefMut for Sample {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
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
#[repr(u8)]
pub enum Priority {
    RealTime = 1,
    InteractiveHigh = 2,
    InteractiveLow = 3,
    DataHigh = 4,
    Data = 5,
    DataLow = 6,
    Background = 7,
}

impl Priority {
    /// The lowest Priority
    pub const MIN: Self = Self::Background;
    /// The highest Priority
    pub const MAX: Self = Self::RealTime;
    /// The number of available priorities
    pub const NUM: usize = 1 + Self::MIN as usize - Self::MAX as usize;
}

impl Default for Priority {
    fn default() -> Priority {
        Priority::Data
    }
}

impl TryFrom<u8> for Priority {
    type Error = zenoh_core::Error;

    /// A Priority is identified by a numeric value.
    /// Lower the value, higher the priority.
    /// Higher the value, lower the priority.
    ///
    /// Admitted values are: 1-7.
    ///
    /// Highest priority: 1
    /// Lowest priority: 7
    fn try_from(priority: u8) -> Result<Self, Self::Error> {
        match priority {
            1 => Ok(Priority::RealTime),
            2 => Ok(Priority::InteractiveHigh),
            3 => Ok(Priority::InteractiveLow),
            4 => Ok(Priority::DataHigh),
            5 => Ok(Priority::Data),
            6 => Ok(Priority::DataLow),
            7 => Ok(Priority::Background),
            unknown => bail!(
                "{} is not a valid priority value. Admitted values are: [{}-{}].",
                unknown,
                Self::MAX as u8,
                Self::MIN as u8
            ),
        }
    }
}

impl From<Priority> for super::net::protocol::core::Priority {
    fn from(prio: Priority) -> Self {
        // The Priority in the prelude differs from the Priority in the core protocol only from
        // the missing Control priority. The Control priority is reserved for zenoh internal use
        // and as such it is not exposed by the zenoh API. Nevertheless, the values of the
        // priorities which are common to the internal and public Priority enums are the same. Therefore,
        // it is possible to safely transmute from the public Priority enum toward the internal
        // Priority enum without risking to be in an invalid state.
        // For better robusteness, the correctness of the unsafe transmute operation is covered
        // by the unit test below.
        unsafe { std::mem::transmute::<Priority, super::net::protocol::core::Priority>(prio) }
    }
}

mod tests {
    #[test]
    fn priority_from() {
        use super::Priority as APrio;
        use crate::net::protocol::core::Priority as TPrio;
        use std::convert::TryInto;

        for i in APrio::MAX as u8..=APrio::MIN as u8 {
            let p: APrio = i.try_into().unwrap();

            match p {
                APrio::RealTime => assert_eq!(p as u8, TPrio::RealTime as u8),
                APrio::InteractiveHigh => assert_eq!(p as u8, TPrio::InteractiveHigh as u8),
                APrio::InteractiveLow => assert_eq!(p as u8, TPrio::InteractiveLow as u8),
                APrio::DataHigh => assert_eq!(p as u8, TPrio::DataHigh as u8),
                APrio::Data => assert_eq!(p as u8, TPrio::Data as u8),
                APrio::DataLow => assert_eq!(p as u8, TPrio::DataLow as u8),
                APrio::Background => assert_eq!(p as u8, TPrio::Background as u8),
            }

            let t: TPrio = p.into();
            assert_eq!(p as u8, t as u8);
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
/// /s1/s2/..../sn?x>1&y<2&...&z=4(p1=v1;p2=v2;...;pn=vn)[a;b;x;y;...;z]
/// |key_selector||---------------- value_selector --------------------|
///                |--- filter --| |---- properties ---|  |--fragment-|
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
    pub value_selector: Cow<'a, str>,
}

impl<'a> Selector<'a> {
    pub fn to_owned(&self) -> Selector<'static> {
        Selector {
            key_selector: self.key_selector.to_owned(),
            value_selector: self.value_selector.to_string().into(),
        }
    }

    /// Sets the `value_selector` part of this `Selector`.
    #[inline(always)]
    pub fn with_value_selector(mut self, value_selector: &'a str) -> Self {
        self.value_selector = value_selector.into();
        self
    }

    /// Parses the `value_selector` part of this `Selector`.
    pub fn parse_value_selector(&'a self) -> crate::Result<ValueSelector<'a>> {
        ValueSelector::try_from(self.value_selector.as_ref())
    }

    /// Returns true if the `Selector` specifies a time-range in its properties
    /// (i.e. using `"starttime"` or `"stoptime"`)
    pub fn has_time_range(&self) -> bool {
        match ValueSelector::try_from(self.value_selector.as_ref()) {
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

impl From<String> for Selector<'_> {
    fn from(s: String) -> Self {
        let (key_selector, value_selector) = s
            .find(|c| c == '?')
            .map_or((s.as_str(), ""), |i| s.split_at(i));
        Selector {
            key_selector: key_selector.to_string().into(),
            value_selector: value_selector.to_string().into(),
        }
    }
}

impl<'a> From<&'a str> for Selector<'a> {
    fn from(s: &'a str) -> Self {
        let (key_selector, value_selector) =
            s.find(|c| c == '?').map_or((s, ""), |i| s.split_at(i));
        Selector {
            key_selector: key_selector.into(),
            value_selector: value_selector.into(),
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
            value_selector: (&q.value_selector).into(),
        }
    }
}

impl<'a> From<&KeyExpr<'a>> for Selector<'a> {
    fn from(key_selector: &KeyExpr<'a>) -> Self {
        Self {
            key_selector: key_selector.clone(),
            value_selector: "".into(),
        }
    }
}

impl<'a> From<KeyExpr<'a>> for Selector<'a> {
    fn from(key_selector: KeyExpr<'a>) -> Self {
        Self {
            key_selector,
            value_selector: "".into(),
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
/// assert_eq!(value_selector.fragment, Some("a;b;x;y;z"));
/// ```
///
/// ```no_run
/// # async_std::task::block_on(async {
/// # use futures::prelude::*;
/// # use zenoh::prelude::r#async::*;
/// # let session = zenoh::open(config::peer()).res().await.unwrap();
///
/// use std::convert::TryInto;
///
/// let queryable = session.queryable("/key/expression").res().await.unwrap();
/// while let Ok(query) = queryable.recv_async().await {
///     let selector = query.selector();
///     let value_selector = selector.parse_value_selector().unwrap();
///     println!("filter: {}", value_selector.filter);
///     println!("properties: {}", value_selector.properties);
///     println!("fragment: {:?}", value_selector.fragment);
/// }
/// # })
/// ```
///
/// ```
/// # async_std::task::block_on(async {
/// # use futures::prelude::*;
/// # use zenoh::prelude::r#async::*;
/// # let session = zenoh::open(config::peer()).res().await.unwrap();
/// # let mut properties = Properties::default();
///
/// let value_selector = ValueSelector::empty()
///     .with_filter("x>1&y<2")
///     .with_properties(properties)
///     .with_fragment(Some("x;y"));
///
/// let mut replies = session.get(
///     &Selector::from("/key/expression").with_value_selector(&value_selector.to_string())
/// ).res().await.unwrap();
/// # })
/// ```
#[derive(Debug, Clone)]
pub struct ValueSelector<'a> {
    /// the filter part of this `ValueSelector`, if any (all characters after `?` and before `(` or `[`)
    pub filter: &'a str,
    /// the properties part of this `ValueSelector`) (all characters between ``( )`` and after `?`)
    pub properties: Properties,
    /// the fragment part of this `ValueSelector`, if any (all characters between ``[ ]`` and after `?`)
    pub fragment: Option<&'a str>,
}

impl<'a> ValueSelector<'a> {
    /// Creates a new `ValueSelector`.
    pub fn new(filter: &'a str, properties: Properties, fragment: Option<&'a str>) -> Self {
        ValueSelector {
            filter,
            properties,
            fragment,
        }
    }

    /// Creates an empty `ValueSelector`.
    pub fn empty() -> Self {
        ValueSelector::new("", Properties::default(), None)
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
    pub fn with_fragment(mut self, fragment: Option<&'a str>) -> Self {
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
        if let Some(frag) = self.fragment {
            s.push('[');
            s.push_str(frag);
            s.push(']');
        }
        write!(f, "{}", s)
    }
}

impl<'a> TryFrom<&'a str> for ValueSelector<'a> {
    type Error = zenoh_core::Error;

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
                fragment: caps.name("frag").map(|s| s.as_str()),
            })
        } else {
            bail!("invalid selector: {}", &s)
        }
    }
}

impl<'a> TryFrom<&'a String> for ValueSelector<'a> {
    type Error = zenoh_core::Error;

    fn try_from(s: &'a String) -> crate::Result<Self> {
        ValueSelector::try_from(s.as_str())
    }
}

/// Functions to create zenoh entities with `'static` lifetime.
///
/// This trait contains functions to create zenoh entities like
/// [`Subscriber`](crate::subscriber::HandlerSubscriber), and
/// [`Queryable`](crate::queryable::HandlerQueryable) with a `'static` lifetime.
/// This is useful to move zenoh entities to several threads and tasks.
///
/// This trait is implemented for `Arc<Session>`.
///
/// # Examples
/// ```no_run
/// # async_std::task::block_on(async {
/// use r#async::AsyncResolve;
/// use zenoh::prelude::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
/// let subscriber = session.subscribe("/key/expression").res().await.unwrap();
/// async_std::task::spawn(async move {
///     while let Ok(sample) = subscriber.recv_async().await {
///         println!("Received : {:?}", sample);
///     }
/// }).await;
/// # })
/// ```
pub trait EntityFactory {
    /// Create a [`Subscriber`](crate::subscriber::HandlerSubscriber) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The resourkey expression to subscribe to
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
    /// let subscriber = session.subscribe("/key/expression").res().await.unwrap();
    /// async_std::task::spawn(async move {
    ///     while let Ok(sample) = subscriber.recv_async().await {
    ///         println!("Received : {:?}", sample);
    ///     }
    /// }).await;
    /// # })
    /// ```
    fn subscribe<'a, IntoKeyExpr>(&self, key_expr: IntoKeyExpr) -> SubscriberBuilder<'static, 'a>
    where
        IntoKeyExpr: Into<KeyExpr<'a>>;

    /// Create a [`Queryable`](crate::queryable::HandlerQueryable) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression matching the queries the
    /// [`Queryable`](crate::queryable::HandlerQueryable) will reply to
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use r#async::AsyncResolve;
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
    /// let queryable = session.queryable("/key/expression").res().await.unwrap();
    /// async_std::task::spawn(async move {
    ///     while let Ok(query) = queryable.recv_async().await {
    ///         query.reply(Ok(Sample::new(
    ///             "/key/expression".to_string(),
    ///             "value",
    ///         ))).res().await.unwrap();
    ///     }
    /// }).await;
    /// # })
    /// ```
    fn queryable<'a, IntoKeyExpr>(&self, key_expr: IntoKeyExpr) -> QueryableBuilder<'static, 'a>
    where
        IntoKeyExpr: Into<KeyExpr<'a>>;

    /// Create a [`Publisher`](crate::publication::Publisher) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression matching resources to write
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
    /// let publisher = session.publish("/key/expression").res().await.unwrap();
    /// publisher.put("value").unwrap();
    /// # })
    /// ```
    fn publish<'a, IntoKeyExpr>(&self, key_expr: IntoKeyExpr) -> PublishBuilder<'a>
    where
        IntoKeyExpr: Into<KeyExpr<'a>>;
}

pub type Dyn<T> = std::boxed::Box<T>;
pub type Callback<T> = Dyn<dyn Fn(T) + Send + Sync>;
pub type CallbackMut<T> = Dyn<dyn FnMut(T) + Send + Sync>;

/// A Handler is the combination of:
///  - a callback which is called on
/// some events like reception of a Sample in a Subscriber or reception
/// of a Query in a Queryable
///  - a receiver that allows to collect and access those events one way
/// or another.
///
/// For example a flume channel can be transformed into a Handler by
/// implmenting the [`IntoHandler`] Trait.
pub type Handler<T, Receiver> = (Callback<T>, Receiver);

/// A value-to-value conversion that consumes the input value and
/// transforms it into a [`Handler`].
pub trait IntoHandler<T, Receiver> {
    /// Converts this type into a [`Handler`].
    fn into_handler(self) -> Handler<T, Receiver>;
}

/// A function that can transform a [`FnMut`]`(T)` to
/// a [`Fn`]`(T)` with the help of a [`Mutex`](std::sync::Mutex).
pub fn locked<T>(fnmut: impl FnMut(T)) -> impl Fn(T) {
    let lock = std::sync::Mutex::new(fnmut);
    move |x| zlock!(lock)(x)
}
