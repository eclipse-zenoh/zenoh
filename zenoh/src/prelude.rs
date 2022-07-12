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
    pub use super::common::*;
    pub use zenoh_core::SyncResolve;
}
pub mod r#async {
    pub use super::common::*;
    pub use zenoh_core::AsyncResolve;
}

pub use common::*;
pub(crate) mod common {
    #[cfg(feature = "shared-memory")]
    use crate::buf::SharedMemoryBuf;
    use crate::buf::ZBuf;
    pub use crate::key_expr::{keyexpr, KeyExpr, OwnedKeyExpr};
    use crate::publication::PublisherBuilder;
    use crate::queryable::QueryableBuilder;
    use crate::subscriber::{PushMode, SubscriberBuilder};
    use crate::time::{new_reception_timestamp, Timestamp};
    use std::borrow::Cow;
    use std::convert::{TryFrom, TryInto};
    use std::fmt;
    use std::ops::{Deref, DerefMut};
    #[cfg(feature = "shared-memory")]
    use std::sync::Arc;
    pub use zenoh_buffers::SplitBuffer;
    use zenoh_core::bail;
    use zenoh_core::zresult::ZError;
    use zenoh_protocol::proto::DataInfo;

    pub(crate) type Id = usize;

    pub use crate::config;
    pub use crate::properties::Properties;
    pub use crate::selector::{Selector, ValueSelector, ValueSelectorProperty};
    pub use zenoh_config::ValidatedMap;

    /// A [`Locator`] contains a choice of protocol, an address and port, as well as optional additional properties to work with.
    pub use zenoh_protocol_core::Locator;

    /// The encoding of a zenoh [`Value`].
    pub use zenoh_protocol_core::{Encoding, KnownEncoding};

    /// The global unique id of a zenoh peer.
    pub use zenoh_protocol_core::ZenohId;

    pub use zenoh_protocol_core::ZInt;

    /// A zenoh Value.
    #[non_exhaustive]
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
                    let r = serde::Deserialize::deserialize(
                        &mut serde_json::Deserializer::from_slice(&v.payload.contiguous()),
                    );
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

    pub use zenoh_protocol_core::SampleKind;

    /// A zenoh sample.
    #[non_exhaustive]
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
            }
        }
        /// Creates a new Sample.
        #[inline]
        pub fn try_from<TryIntoKeyExpr, IntoValue>(
            key_expr: TryIntoKeyExpr,
            value: IntoValue,
        ) -> Result<Self, zenoh_core::Error>
        where
            TryIntoKeyExpr: TryInto<KeyExpr<'static>>,
            <TryIntoKeyExpr as TryInto<KeyExpr<'static>>>::Error: Into<zenoh_core::Error>,
            IntoValue: Into<Value>,
        {
            Ok(Sample {
                key_expr: key_expr.try_into().map_err(Into::into)?,
                value: value.into(),
                kind: SampleKind::default(),
                timestamp: None,
            })
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
                    kind: data_info.kind,
                    timestamp: data_info.timestamp,
                }
            } else {
                Sample {
                    key_expr,
                    value,
                    kind: SampleKind::default(),
                    timestamp: None,
                }
            }
        }

        #[inline]
        pub(crate) fn split(self) -> (KeyExpr<'static>, ZBuf, DataInfo) {
            let info = DataInfo {
                kind: self.kind,
                encoding: Some(self.value.encoding),
                timestamp: self.timestamp,
                #[cfg(feature = "shared-memory")]
                sliced: false,
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

        #[inline]
        /// Ensure that an associated Timestamp is present in this Sample.
        /// If not, a new one is created with the current system time and 0x00 as id.
        /// Get the timestamp of this sample (either existing one or newly created)
        pub fn ensure_timestamp(&mut self) -> &Timestamp {
            if let Some(ref timestamp) = self.timestamp {
                timestamp
            } else {
                let timestamp = new_reception_timestamp();
                self.timestamp = Some(timestamp);
                self.timestamp.as_ref().unwrap()
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

    /// The Priority of zenoh messages.
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

    impl From<Priority> for super::super::net::protocol::core::Priority {
        fn from(prio: Priority) -> Self {
            // The Priority in the prelude differs from the Priority in the core protocol only from
            // the missing Control priority. The Control priority is reserved for zenoh internal use
            // and as such it is not exposed by the zenoh API. Nevertheless, the values of the
            // priorities which are common to the internal and public Priority enums are the same. Therefore,
            // it is possible to safely transmute from the public Priority enum toward the internal
            // Priority enum without risking to be in an invalid state.
            // For better robusteness, the correctness of the unsafe transmute operation is covered
            // by the unit test below.
            unsafe {
                std::mem::transmute::<Priority, super::super::net::protocol::core::Priority>(prio)
            }
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
    /// let subscriber = session.declare_subscriber("key/expression").res().await.unwrap();
    /// async_std::task::spawn(async move {
    ///     while let Ok(sample) = subscriber.recv_async().await {
    ///         println!("Received : {:?}", sample);
    ///     }
    /// }).await;
    /// # })
    /// ```
    pub trait SessionDeclarations {
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
        /// let subscriber = session.declare_subscriber("key/expression").res().await.unwrap();
        /// async_std::task::spawn(async move {
        ///     while let Ok(sample) = subscriber.recv_async().await {
        ///         println!("Received : {:?}", sample);
        ///     }
        /// }).await;
        /// # })
        /// ```
        fn declare_subscriber<'a, TryIntoKeyExpr>(
            &self,
            key_expr: TryIntoKeyExpr,
        ) -> SubscriberBuilder<'static, 'a, PushMode>
        where
            TryIntoKeyExpr: TryInto<KeyExpr<'a>>,
            <TryIntoKeyExpr as TryInto<KeyExpr<'a>>>::Error: Into<zenoh_core::Error>;

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
        /// let queryable = session.declare_queryable("key/expression").res().await.unwrap();
        /// async_std::task::spawn(async move {
        ///     while let Ok(query) = queryable.recv_async().await {
        ///         query.reply(Ok(Sample::try_from(
        ///             "key/expression",
        ///             "value",
        ///         ).unwrap())).res().await.unwrap();
        ///     }
        /// }).await;
        /// # })
        /// ```
        fn declare_queryable<'a, TryIntoKeyExpr>(
            &self,
            key_expr: TryIntoKeyExpr,
        ) -> QueryableBuilder<'static, 'a>
        where
            TryIntoKeyExpr: TryInto<KeyExpr<'a>>,
            <TryIntoKeyExpr as TryInto<KeyExpr<'a>>>::Error: Into<zenoh_core::Error>;

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
        /// let publisher = session.declare_publisher("key/expression").res().await.unwrap();
        /// publisher.put("value").res().await.unwrap();
        /// # })
        /// ```
        fn declare_publisher<'a, TryIntoKeyExpr>(
            &self,
            key_expr: TryIntoKeyExpr,
        ) -> PublisherBuilder<'a>
        where
            TryIntoKeyExpr: TryInto<KeyExpr<'a>>,
            <TryIntoKeyExpr as TryInto<KeyExpr<'a>>>::Error: Into<zenoh_core::Error>;
    }

    /// An alias for `Box<T>`.
    pub type Dyn<T> = std::boxed::Box<T>;
    /// An immutable callback function.
    pub type Callback<T> = Dyn<dyn Fn(T) + Send + Sync>;
    /// A mutable callback function.
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
}
