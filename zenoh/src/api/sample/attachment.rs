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

use zenoh_buffers::{
    reader::{HasReader, Reader},
    writer::HasWriter,
    ZBuf, ZBufReader, ZSlice,
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_protocol::zenoh::ext::AttachmentType;

/// A builder for [`Attachment`]
#[derive(Debug)]
pub struct AttachmentBuilder {
    pub(crate) inner: Vec<u8>,
}
impl Default for AttachmentBuilder {
    fn default() -> Self {
        Self::new()
    }
}
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
impl From<AttachmentBuilder> for Attachment {
    fn from(value: AttachmentBuilder) -> Self {
        Attachment {
            inner: value.inner.into(),
        }
    }
}
impl From<AttachmentBuilder> for Option<Attachment> {
    fn from(value: AttachmentBuilder) -> Self {
        if value.inner.is_empty() {
            None
        } else {
            Some(value.into())
        }
    }
}

#[derive(Clone)]
pub struct Attachment {
    pub(crate) inner: ZBuf,
}
impl Default for Attachment {
    fn default() -> Self {
        Self::new()
    }
}
impl<const ID: u8> From<Attachment> for AttachmentType<ID> {
    fn from(this: Attachment) -> Self {
        AttachmentType { buffer: this.inner }
    }
}
impl<const ID: u8> From<AttachmentType<ID>> for Attachment {
    fn from(this: AttachmentType<ID>) -> Self {
        Attachment { inner: this.buffer }
    }
}
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
pub struct AttachmentIterator<'a> {
    reader: ZBufReader<'a>,
}
impl<'a> core::iter::IntoIterator for &'a Attachment {
    type Item = (ZSlice, ZSlice);
    type IntoIter = AttachmentIterator<'a>;
    fn into_iter(self) -> Self::IntoIter {
        AttachmentIterator {
            reader: self.inner.reader(),
        }
    }
}
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
impl<'a> core::iter::FromIterator<(&'a [u8], &'a [u8])> for Attachment {
    fn from_iter<T: IntoIterator<Item = (&'a [u8], &'a [u8])>>(iter: T) -> Self {
        AttachmentBuilder::from_iter(iter).into()
    }
}
