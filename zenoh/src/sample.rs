//
// Copyright (c) 2023 ZettaScale Technology
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

//! Sample primitives
use crate::encoding::Encoding;
use crate::payload::Payload;
use crate::prelude::{KeyExpr, Value};
use crate::time::{new_reception_timestamp, Timestamp};
use crate::Priority;
#[zenoh_macros::unstable]
use serde::Serialize;
use std::{
    convert::{TryFrom, TryInto},
    fmt,
};
use zenoh_protocol::core::EntityGlobalId;
use zenoh_protocol::{core::CongestionControl, network::push::ext::QoSType};

pub type SourceSn = u64;

/// The locality of samples to be received by subscribers or targeted by publishers.
#[zenoh_macros::unstable]
#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
pub enum Locality {
    SessionLocal,
    Remote,
    #[default]
    Any,
}
#[cfg(not(feature = "unstable"))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Locality {
    SessionLocal,
    Remote,
    #[default]
    Any,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct DataInfo {
    pub kind: SampleKind,
    pub encoding: Option<Encoding>,
    pub timestamp: Option<Timestamp>,
    pub source_id: Option<EntityGlobalId>,
    pub source_sn: Option<SourceSn>,
    pub qos: QoS,
}

/// Informations on the source of a zenoh [`Sample`].
#[zenoh_macros::unstable]
#[derive(Debug, Clone)]
pub struct SourceInfo {
    /// The [`EntityGlobalId`] of the zenoh entity that published the concerned [`Sample`].
    pub source_id: Option<EntityGlobalId>,
    /// The sequence number of the [`Sample`] from the source.
    pub source_sn: Option<SourceSn>,
}

#[test]
#[cfg(feature = "unstable")]
fn source_info_stack_size() {
    use crate::{
        sample::{SourceInfo, SourceSn},
        ZenohId,
    };

    assert_eq!(std::mem::size_of::<ZenohId>(), 16);
    assert_eq!(std::mem::size_of::<Option<ZenohId>>(), 17);
    assert_eq!(std::mem::size_of::<Option<SourceSn>>(), 16);
    assert_eq!(std::mem::size_of::<SourceInfo>(), 17 + 16 + 7);
}

#[zenoh_macros::unstable]
impl SourceInfo {
    pub(crate) fn empty() -> Self {
        SourceInfo {
            source_id: None,
            source_sn: None,
        }
    }
}

#[zenoh_macros::unstable]
impl From<DataInfo> for SourceInfo {
    fn from(data_info: DataInfo) -> Self {
        SourceInfo {
            source_id: data_info.source_id,
            source_sn: data_info.source_sn,
        }
    }
}

#[zenoh_macros::unstable]
impl From<Option<DataInfo>> for SourceInfo {
    fn from(data_info: Option<DataInfo>) -> Self {
        match data_info {
            Some(data_info) => data_info.into(),
            None => SourceInfo::empty(),
        }
    }
}

mod attachment {
    #[zenoh_macros::unstable]
    use zenoh_buffers::{
        reader::{HasReader, Reader},
        writer::HasWriter,
        ZBuf, ZBufReader, ZSlice,
    };
    #[zenoh_macros::unstable]
    use zenoh_codec::{RCodec, WCodec, Zenoh080};
    #[zenoh_macros::unstable]
    use zenoh_protocol::zenoh::ext::AttachmentType;

    /// A builder for [`Attachment`]
    #[zenoh_macros::unstable]
    #[derive(Debug)]
    pub struct AttachmentBuilder {
        pub(crate) inner: Vec<u8>,
    }
    #[zenoh_macros::unstable]
    impl Default for AttachmentBuilder {
        fn default() -> Self {
            Self::new()
        }
    }
    #[zenoh_macros::unstable]
    impl AttachmentBuilder {
        pub fn new() -> Self {
            Self { inner: Vec::new() }
        }
        fn _insert(&mut self, key: &[u8], value: &[u8]) {
            let codec = Zenoh080;
            let mut writer = self.inner.writer();
            codec.write(&mut writer, key).unwrap(); // Infallible, barring alloc failure
            codec.write(&mut writer, value).unwrap(); // Infallible, barring alloc failure
        }
        /// Inserts a key-value pair to the attachment.
        ///
        /// Note that [`Attachment`] is a list of non-unique key-value pairs: inserting at the same key multiple times leads to both values being transmitted for that key.
        pub fn insert<Key: AsRef<[u8]> + ?Sized, Value: AsRef<[u8]> + ?Sized>(
            &mut self,
            key: &Key,
            value: &Value,
        ) {
            self._insert(key.as_ref(), value.as_ref())
        }
        pub fn build(self) -> Attachment {
            Attachment {
                inner: self.inner.into(),
            }
        }
    }
    #[zenoh_macros::unstable]
    impl From<AttachmentBuilder> for Attachment {
        fn from(value: AttachmentBuilder) -> Self {
            Attachment {
                inner: value.inner.into(),
            }
        }
    }
    #[zenoh_macros::unstable]
    #[derive(Clone)]
    pub struct Attachment {
        pub(crate) inner: ZBuf,
    }
    #[zenoh_macros::unstable]
    impl Default for Attachment {
        fn default() -> Self {
            Self::new()
        }
    }
    #[zenoh_macros::unstable]
    impl<const ID: u8> From<Attachment> for AttachmentType<ID> {
        fn from(this: Attachment) -> Self {
            AttachmentType { buffer: this.inner }
        }
    }
    #[zenoh_macros::unstable]
    impl<const ID: u8> From<AttachmentType<ID>> for Attachment {
        fn from(this: AttachmentType<ID>) -> Self {
            Attachment { inner: this.buffer }
        }
    }
    #[zenoh_macros::unstable]
    impl Attachment {
        pub fn new() -> Self {
            Self {
                inner: ZBuf::empty(),
            }
        }
        pub fn is_empty(&self) -> bool {
            self.len() == 0
        }
        pub fn len(&self) -> usize {
            self.iter().count()
        }
        pub fn iter(&self) -> AttachmentIterator {
            self.into_iter()
        }
        fn _get(&self, key: &[u8]) -> Option<ZSlice> {
            self.iter()
                .find_map(|(k, v)| (k.as_slice() == key).then_some(v))
        }
        pub fn get<Key: AsRef<[u8]>>(&self, key: &Key) -> Option<ZSlice> {
            self._get(key.as_ref())
        }
        fn _insert(&mut self, key: &[u8], value: &[u8]) {
            let codec = Zenoh080;
            let mut writer = self.inner.writer();
            codec.write(&mut writer, key).unwrap(); // Infallible, barring alloc failure
            codec.write(&mut writer, value).unwrap(); // Infallible, barring alloc failure
        }
        /// Inserts a key-value pair to the attachment.
        ///
        /// Note that [`Attachment`] is a list of non-unique key-value pairs: inserting at the same key multiple times leads to both values being transmitted for that key.
        ///
        /// [`Attachment`] is not very efficient at inserting, so if you wish to perform multiple inserts, it's generally better to [`Attachment::extend`] after performing the inserts on an [`AttachmentBuilder`]
        pub fn insert<Key: AsRef<[u8]> + ?Sized, Value: AsRef<[u8]> + ?Sized>(
            &mut self,
            key: &Key,
            value: &Value,
        ) {
            self._insert(key.as_ref(), value.as_ref())
        }
        fn _extend(&mut self, with: Self) -> &mut Self {
            for slice in with.inner.zslices().cloned() {
                self.inner.push_zslice(slice);
            }
            self
        }
        pub fn extend(&mut self, with: impl Into<Self>) -> &mut Self {
            let with = with.into();
            self._extend(with)
        }
    }
    #[zenoh_macros::unstable]
    pub struct AttachmentIterator<'a> {
        reader: ZBufReader<'a>,
    }
    #[zenoh_macros::unstable]
    impl<'a> core::iter::IntoIterator for &'a Attachment {
        type Item = (ZSlice, ZSlice);
        type IntoIter = AttachmentIterator<'a>;
        fn into_iter(self) -> Self::IntoIter {
            AttachmentIterator {
                reader: self.inner.reader(),
            }
        }
    }
    #[zenoh_macros::unstable]
    impl core::fmt::Debug for Attachment {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{{")?;
            for (key, value) in self {
                let key = key.as_slice();
                let value = value.as_slice();
                match core::str::from_utf8(key) {
                    Ok(key) => write!(f, "\"{key}\": ")?,
                    Err(_) => {
                        write!(f, "0x")?;
                        for byte in key {
                            write!(f, "{byte:02X}")?
                        }
                    }
                }
                match core::str::from_utf8(value) {
                    Ok(value) => write!(f, "\"{value}\", ")?,
                    Err(_) => {
                        write!(f, "0x")?;
                        for byte in value {
                            write!(f, "{byte:02X}")?
                        }
                        write!(f, ", ")?
                    }
                }
            }
            write!(f, "}}")
        }
    }
    #[zenoh_macros::unstable]
    impl<'a> core::iter::Iterator for AttachmentIterator<'a> {
        type Item = (ZSlice, ZSlice);
        fn next(&mut self) -> Option<Self::Item> {
            let key = Zenoh080.read(&mut self.reader).ok()?;
            let value = Zenoh080.read(&mut self.reader).ok()?;
            Some((key, value))
        }
        fn size_hint(&self) -> (usize, Option<usize>) {
            (
                (self.reader.remaining() != 0) as usize,
                Some(self.reader.remaining() / 2),
            )
        }
    }
    #[zenoh_macros::unstable]
    impl<'a> core::iter::FromIterator<(&'a [u8], &'a [u8])> for AttachmentBuilder {
        fn from_iter<T: IntoIterator<Item = (&'a [u8], &'a [u8])>>(iter: T) -> Self {
            let codec = Zenoh080;
            let mut buffer: Vec<u8> = Vec::new();
            let mut writer = buffer.writer();
            for (key, value) in iter {
                codec.write(&mut writer, key).unwrap(); // Infallible, barring allocation failures
                codec.write(&mut writer, value).unwrap(); // Infallible, barring allocation failures
            }
            Self { inner: buffer }
        }
    }
    #[zenoh_macros::unstable]
    impl<'a> core::iter::FromIterator<(&'a [u8], &'a [u8])> for Attachment {
        fn from_iter<T: IntoIterator<Item = (&'a [u8], &'a [u8])>>(iter: T) -> Self {
            AttachmentBuilder::from_iter(iter).into()
        }
    }
}

/// The kind of a `Sample`.
#[repr(u8)]
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub enum SampleKind {
    /// if the `Sample` was issued by a `put` operation.
    #[default]
    Put = 0,
    /// if the `Sample` was issued by a `delete` operation.
    Delete = 1,
}

impl fmt::Display for SampleKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SampleKind::Put => write!(f, "PUT"),
            SampleKind::Delete => write!(f, "DELETE"),
        }
    }
}

impl TryFrom<u64> for SampleKind {
    type Error = u64;
    fn try_from(kind: u64) -> Result<Self, u64> {
        match kind {
            0 => Ok(SampleKind::Put),
            1 => Ok(SampleKind::Delete),
            _ => Err(kind),
        }
    }
}

#[zenoh_macros::unstable]
pub use attachment::{Attachment, AttachmentBuilder, AttachmentIterator};

/// A zenoh sample.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct Sample {
    pub(crate) key_expr: KeyExpr<'static>,
    pub(crate) payload: Payload,
    pub(crate) kind: SampleKind,
    pub(crate) encoding: Encoding,
    pub(crate) timestamp: Option<Timestamp>,
    pub(crate) qos: QoS,

    #[cfg(feature = "unstable")]
    pub(crate) source_info: SourceInfo,

    #[cfg(feature = "unstable")]
    pub(crate) attachment: Option<Attachment>,
}

impl Sample {
    /// Creates a new Sample.
    #[inline]
    pub fn new<IntoKeyExpr, IntoPayload>(key_expr: IntoKeyExpr, payload: IntoPayload) -> Self
    where
        IntoKeyExpr: Into<KeyExpr<'static>>,
        IntoPayload: Into<Payload>,
    {
        Sample {
            key_expr: key_expr.into(),
            payload: payload.into(),
            encoding: Encoding::default(),
            kind: SampleKind::default(),
            timestamp: None,
            qos: QoS::default(),
            #[cfg(feature = "unstable")]
            source_info: SourceInfo::empty(),
            #[cfg(feature = "unstable")]
            attachment: None,
        }
    }
    /// Creates a new Sample.
    #[inline]
    pub fn try_from<TryIntoKeyExpr, IntoPayload>(
        key_expr: TryIntoKeyExpr,
        payload: IntoPayload,
    ) -> Result<Self, zenoh_result::Error>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'static>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'static>>>::Error: Into<zenoh_result::Error>,
        IntoPayload: Into<Payload>,
    {
        Ok(Sample {
            key_expr: key_expr.try_into().map_err(Into::into)?,
            payload: payload.into(),
            encoding: Encoding::default(),
            kind: SampleKind::default(),
            timestamp: None,
            qos: QoS::default(),
            #[cfg(feature = "unstable")]
            source_info: SourceInfo::empty(),
            #[cfg(feature = "unstable")]
            attachment: None,
        })
    }

    /// Creates a new Sample with optional data info.
    #[inline]
    pub(crate) fn with_info(mut self, mut data_info: Option<DataInfo>) -> Self {
        if let Some(mut data_info) = data_info.take() {
            self.kind = data_info.kind;
            if let Some(encoding) = data_info.encoding.take() {
                self.encoding = encoding;
            }
            self.qos = data_info.qos;
            self.timestamp = data_info.timestamp;
            #[cfg(feature = "unstable")]
            {
                self.source_info = SourceInfo {
                    source_id: data_info.source_id,
                    source_sn: data_info.source_sn,
                };
            }
        }
        self
    }

    /// Sets the encoding of this Sample.
    #[inline]
    pub fn with_encoding(mut self, encoding: Encoding) -> Self {
        self.encoding = encoding;
        self
    }

    /// Gets the key expression on which this Sample was published.
    #[inline]
    pub fn key_expr(&self) -> &KeyExpr<'static> {
        &self.key_expr
    }

    /// Gets the payload of this Sample.
    #[inline]
    pub fn payload(&self) -> &Payload {
        &self.payload
    }

    /// Gets the kind of this Sample.
    #[inline]
    pub fn kind(&self) -> SampleKind {
        self.kind
    }

    /// Sets the kind of this Sample.
    #[inline]
    #[doc(hidden)]
    #[zenoh_macros::unstable]
    pub fn with_kind(mut self, kind: SampleKind) -> Self {
        self.kind = kind;
        self
    }

    /// Gets the encoding of this sample
    #[inline]
    pub fn encoding(&self) -> &Encoding {
        &self.encoding
    }

    /// Gets the timestamp of this Sample.
    #[inline]
    pub fn timestamp(&self) -> Option<&Timestamp> {
        self.timestamp.as_ref()
    }

    /// Sets the timestamp of this Sample.
    #[inline]
    #[doc(hidden)]
    #[zenoh_macros::unstable]
    pub fn with_timestamp(mut self, timestamp: Timestamp) -> Self {
        self.timestamp = Some(timestamp);
        self
    }

    /// Gets the quality of service settings this Sample was sent with.
    #[inline]
    pub fn qos(&self) -> &QoS {
        &self.qos
    }

    /// Gets infos on the source of this Sample.
    #[zenoh_macros::unstable]
    #[inline]
    pub fn source_info(&self) -> &SourceInfo {
        &self.source_info
    }

    /// Sets the source info of this Sample.
    #[zenoh_macros::unstable]
    #[inline]
    pub fn with_source_info(mut self, source_info: SourceInfo) -> Self {
        self.source_info = source_info;
        self
    }

    /// Ensure that an associated Timestamp is present in this Sample.
    /// If not, a new one is created with the current system time and 0x00 as id.
    /// Get the timestamp of this sample (either existing one or newly created)
    #[inline]
    #[doc(hidden)]
    #[zenoh_macros::unstable]
    pub fn ensure_timestamp(&mut self) -> &Timestamp {
        if let Some(ref timestamp) = self.timestamp {
            timestamp
        } else {
            let timestamp = new_reception_timestamp();
            self.timestamp = Some(timestamp);
            self.timestamp.as_ref().unwrap()
        }
    }

    /// Gets the sample attachment: a map of key-value pairs, where each key and value are byte-slices.
    #[zenoh_macros::unstable]
    #[inline]
    pub fn attachment(&self) -> Option<&Attachment> {
        self.attachment.as_ref()
    }

    /// Gets the mutable sample attachment: a map of key-value pairs, where each key and value are byte-slices.
    #[inline]
    #[doc(hidden)]
    #[zenoh_macros::unstable]
    pub fn attachment_mut(&mut self) -> &mut Option<Attachment> {
        &mut self.attachment
    }

    #[inline]
    #[doc(hidden)]
    #[zenoh_macros::unstable]
    pub fn with_attachment(mut self, attachment: Attachment) -> Self {
        self.attachment = Some(attachment);
        self
    }
}

impl From<Sample> for Value {
    fn from(sample: Sample) -> Self {
        Value::new(sample.payload).with_encoding(sample.encoding)
    }
}

/// Structure containing quality of service data
#[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
pub struct QoS {
    inner: QoSType,
}

impl QoS {
    /// Gets priority of the message.
    pub fn priority(&self) -> Priority {
        match Priority::try_from(self.inner.get_priority()) {
            Ok(p) => p,
            Err(e) => {
                log::trace!(
                    "Failed to convert priority: {}; replacing with default value",
                    e.to_string()
                );
                Priority::default()
            }
        }
    }

    /// Gets congestion control of the message.
    pub fn congestion_control(&self) -> CongestionControl {
        self.inner.get_congestion_control()
    }

    /// Gets express flag value. If `true`, the message is not batched during transmission, in order to reduce latency.
    pub fn express(&self) -> bool {
        self.inner.is_express()
    }

    /// Sets priority value.
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.inner.set_priority(priority.into());
        self
    }

    /// Sets congestion control value.
    pub fn with_congestion_control(mut self, congestion_control: CongestionControl) -> Self {
        self.inner.set_congestion_control(congestion_control);
        self
    }

    /// Sets express flag vlaue.
    pub fn with_express(mut self, is_express: bool) -> Self {
        self.inner.set_is_express(is_express);
        self
    }
}

impl From<QoSType> for QoS {
    fn from(qos: QoSType) -> Self {
        QoS { inner: qos }
    }
}

impl From<QoS> for QoSType {
    fn from(qos: QoS) -> Self {
        qos.inner
    }
}
