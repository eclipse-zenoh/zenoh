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

use alloc::{boxed::Box, string::String, vec::Vec};
use core::{
    convert::{TryFrom, TryInto},
    fmt::Display,
    num::NonZeroU32,
};

use zenoh_result::{bail, zerror, Error, IError, ZResult};

use super::{keyexpr, OwnedKeyExpr};

mod support;
pub use support::{IKeFormatStorage, Segment};
use support::{IterativeConstructor, Spec};

/// A utility to define Key Expression (KE) formats.
///
/// Formats are written like KEs, except sections can be substituted for specs using the `${id:pattern#default}` format to define fields.  
/// `id` is the name of the field that gets encoded in that section, it must be non-empty and will stop at the first encountered `:`.  
/// `pattern` is a KE pattern that any value set for that field must match. It stops at the first encountered `#` or end of spec.  
/// `default` is optional, and lets you specify a value at construction for the field.
///
/// Note that the spec is considered to end at the first encountered `}`; if you need your id, pattern or default to contain `}`, you may use `$#{spec}#.
///
/// Specs may only be preceded and followed by `/`.
#[derive(Debug, Clone, Copy, Hash)]
pub struct KeFormat<'s, Storage: IKeFormatStorage<'s> + 's = Vec<Segment<'s>>> {
    storage: Storage,
    suffix: &'s str,
}
impl<'s> KeFormat<'s, Vec<Segment<'s>>> {
    pub fn new<S: AsRef<str> + ?Sized>(value: &'s S) -> ZResult<Self> {
        value.as_ref().try_into()
    }
    pub fn noalloc_new<const N: usize>(value: &'s str) -> ZResult<KeFormat<'s, [Segment<'s>; N]>> {
        value.try_into()
    }
}

pub mod macro_support {
    use super::*;
    /// DO NOT USE THIS
    ///
    /// This is a support structure for [`const_new`], which is only meant to be used through the `zenoh::keformat` macro
    #[derive(Clone, Copy)]
    pub struct SegmentBuilder {
        pub segment_start: usize,
        pub prefix_end: usize,
        pub spec_start: usize,
        pub id_end: u16,
        pub pattern_end: u16,
        pub spec_end: usize,
        pub segment_end: usize,
    }

    /// # Safety
    /// DO NOT USE THIS, EVER
    ///
    /// This is a support function which is only meant to be used through the `zenoh::keformat` macro
    pub unsafe fn specs<'s>(this: &KeFormat<'s, Vec<Segment<'s>>>) -> Vec<SegmentBuilder> {
        let segments = this.storage.segments();
        if segments.is_empty() {
            return Vec::new();
        }
        let source_start = segments[0].prefix.as_ptr() as usize;
        segments
            .iter()
            .map(|segment| {
                let segment_start = segment.prefix.as_ptr() as usize - source_start;
                let prefix_end = segment_start + segment.prefix.len();
                let spec_start = segment.spec.spec.as_ptr() as usize - source_start;
                let spec_end = spec_start + segment.spec.spec.len();
                let segment_end = spec_end + spec_start - prefix_end - 1;
                SegmentBuilder {
                    segment_start,
                    prefix_end,
                    spec_start,
                    id_end: segment.spec.id_end,
                    pattern_end: segment.spec.pattern_end,
                    spec_end,
                    segment_end,
                }
            })
            .collect()
    }
    /// # Safety
    /// DO NOT USE THIS, EVER
    ///
    /// This is a support function which is only meant to be used through the `zenoh::keformat` macro
    pub const unsafe fn const_new<const N: usize>(
        source: &'static str,
        segments: [SegmentBuilder; N],
    ) -> KeFormat<'static, [Segment<'static>; N]> {
        const unsafe fn substr(source: &'static str, start: usize, end: usize) -> &'static str {
            core::str::from_utf8_unchecked(core::slice::from_raw_parts(
                source.as_ptr().add(start),
                end - start,
            ))
        }
        let mut storage = [Segment {
            prefix: "",
            spec: Spec {
                spec: "",
                id_end: 0,
                pattern_end: 0,
            },
        }; N];
        let mut suffix_start = 0;
        let mut i = 0;
        while i < N {
            let segment = segments[i];
            let prefix = substr(source, segment.segment_start, segment.prefix_end);
            let spec = Spec {
                spec: substr(source, segment.spec_start, segment.spec_end),
                id_end: segment.id_end,
                pattern_end: segment.pattern_end,
            };
            storage[i] = Segment { prefix, spec };
            suffix_start = segment.segment_end;
            i += 1;
        }
        let suffix = substr(source, suffix_start, source.len());
        KeFormat { storage, suffix }
    }
}
impl<'s, Storage: IKeFormatStorage<'s> + 's> KeFormat<'s, Storage> {
    pub fn formatter(&'s self) -> KeFormatter<'s, Storage> {
        KeFormatter {
            format: self,
            buffer: String::new(),
            values: Storage::values_storage(&self.storage, |_| None),
        }
    }
}
impl<'s, Storage: IKeFormatStorage<'s> + 's> TryFrom<&'s String> for KeFormat<'s, Storage> {
    type Error = Error;
    fn try_from(value: &'s String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}
impl<'s, Storage: IKeFormatStorage<'s> + 's> TryFrom<&'s str> for KeFormat<'s, Storage> {
    type Error = Error;
    fn try_from(value: &'s str) -> Result<Self, Self::Error> {
        let mut storage = Storage::new_constructor();
        let mut segment_start = 0;
        let mut i = 0;
        let bvalue = value.as_bytes();
        while i < bvalue.len() {
            if bvalue[i] == b'$' {
                let prefix_end = i;
                i += 1;
                let (terminator, spec_start) = match bvalue[i] {
                    b'{' => ("}", i+ 1),
                    b'*' => {i+= 1; continue;}
                    b'#' if bvalue[i+1] == b'{' => ("}#", i +2),
                    c => bail!("Invalid KeFormat: {value} contains `${}` which is not legal in KEs or formats", c as char)
                };
                let spec = &value[spec_start..];
                let Some(spec_end) = spec.find(terminator) else {
                    bail!("Invalid KeFormat: {value} contains an unterminated spec")
                };
                let spec = &spec[..spec_end];
                let spec = match Spec::try_from(spec) {
                    Ok(spec) => spec,
                    Err(e) => {
                        bail!(e => "Invalid KeFormat: {value} contains an invalid spec: {spec}")
                    }
                };
                let segment_end = spec_end + spec_start + terminator.len();
                if prefix_end != 0 && bvalue[prefix_end - 1] != b'/' {
                    bail!("Invalid KeFormat: a spec in {value} is preceded a non-`/` character")
                }
                if !matches!(bvalue.get(segment_end), None | Some(b'/')) {
                    bail!("Invalid KeFormat: a spec in {value} is followed by a non-`/` character")
                }
                keyexpr::new(spec.pattern().as_str())?; // Check that the pattern is indeed a keyexpr. We can make this more flexible in the future.
                let prefix = &value[segment_start..prefix_end];
                if prefix.contains('*') {
                    bail!("Invalid KeFormat: wildcards are only allowed in specs when writing formats")
                }
                let segment = Segment { prefix, spec };
                storage = match Storage::add_segment(storage, segment) {
                    IterativeConstructor::Error(e) => {
                        bail!("Couldn't construct KeFormat because its Storage's add_segment failed: {e}")
                    }
                    s => s,
                };
                segment_start = segment_end;
            } else {
                i += 1;
            }
        }
        let IterativeConstructor::Complete(storage) = storage else {bail!("Couldn't construct KeFormat because its Storage construction was only partial after adding the last segment.")};
        let segments = storage.segments();
        for i in 0..(segments.len() - 1) {
            if segments[(i + 1)..]
                .iter()
                .any(|s| s.spec.id() == segments[i].spec.id())
            {
                bail!("Invalid KeFormat: {value} contains duplicated ids")
            }
        }
        Ok(KeFormat {
            storage,
            suffix: &value[segment_start..],
        })
    }
}

impl<'s, Storage: IKeFormatStorage<'s> + 's> core::convert::TryFrom<&KeFormat<'s, Storage>>
    for String
{
    type Error = core::fmt::Error;
    fn try_from(value: &KeFormat<'s, Storage>) -> Result<Self, Self::Error> {
        use core::fmt::Write;
        let mut s = String::new();
        for segment in value.storage.segments() {
            s += segment.prefix;
            write!(&mut s, "{}", segment.spec.pattern())?;
        }
        s += value.suffix;
        Ok(s)
    }
}
impl<'s, Storage: IKeFormatStorage<'s> + 's> core::convert::TryFrom<&KeFormat<'s, Storage>>
    for OwnedKeyExpr
{
    type Error = Error;
    fn try_from(value: &KeFormat<'s, Storage>) -> Result<Self, Self::Error> {
        let s: String = value
            .try_into()
            .map_err(|e| zerror!("failed to write into String: {e}"))?;
        OwnedKeyExpr::autocanonize(s)
    }
}

impl<'s, Storage: IKeFormatStorage<'s> + 's> core::fmt::Display for KeFormat<'s, Storage> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for segment in self.storage.segments() {
            write!(f, "{}{}", segment.prefix, segment.spec)?;
        }
        write!(f, "{}", self.suffix)
    }
}
#[derive(Clone)]
pub struct KeFormatter<'s, Storage: IKeFormatStorage<'s>> {
    format: &'s KeFormat<'s, Storage>,
    buffer: String,
    values: Storage::ValuesStorage<Option<(u32, NonZeroU32)>>,
}

impl<'s, Storage: IKeFormatStorage<'s>> core::fmt::Debug for KeFormatter<'s, Storage> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let values = self.values.as_ref();
        let segments = self.format.storage.segments();
        for i in 0..values.len() {
            let Segment { prefix, spec } = segments[i];
            let value =
                values[i].map(|(start, end)| &self.buffer[start as usize..end.get() as usize]);
            let id = spec.id();
            let pattern = spec.pattern();
            let sharp = if id.contains('}')
                || pattern.contains('}')
                || value.map_or_else(
                    || spec.default().map_or(false, |v| v.contains('}')),
                    |v| v.contains('}'),
                ) {
                "#"
            } else {
                ""
            };
            write!(f, "{prefix}${sharp}{{{id}:{pattern}")?;
            if let Some(value) = value {
                write!(f, "={value}")?
            } else if let Some(default) = spec.default() {
                write!(f, "#{default}")?
            };
            write!(f, "}}{sharp}")?;
        }
        write!(f, "{}", self.format.suffix)
    }
}

impl<'s, Storage: IKeFormatStorage<'s>> TryFrom<&KeFormatter<'s, Storage>> for OwnedKeyExpr {
    type Error = Error;

    fn try_from(value: &KeFormatter<'s, Storage>) -> Result<Self, Self::Error> {
        let values = value.values.as_ref();
        let segments = value.format.storage.segments();
        let mut len = value.format.suffix.len();
        for i in 0..values.len() {
            len += segments[i].prefix.len()
                + if let Some((start, end)) = values[i] {
                    (end.get() - start) as usize
                } else if let Some(default) = segments[i].spec.default() {
                    default.len()
                } else {
                    bail!("Missing field `{}` in {value:?}", segments[i].spec.id())
                };
        }
        let mut ans = String::with_capacity(len);
        let mut concatenate = |s: &str| {
            let skip_slash = matches!(ans.as_bytes().last(), None | Some(b'/'))
                && s.as_bytes().first() == Some(&b'/');
            ans += &s[skip_slash as usize..];
        };
        for i in 0..values.len() {
            concatenate(segments[i].prefix);
            if let Some((start, end)) = values[i] {
                let end = end.get();
                concatenate(&value.buffer[start as usize..end as usize])
            } else if let Some(default) = segments[i].spec.default() {
                concatenate(default)
            } else {
                unsafe { core::hint::unreachable_unchecked() }
            };
        }
        concatenate(value.format.suffix);
        if ans.ends_with('/') {
            ans.pop();
        }
        OwnedKeyExpr::autocanonize(ans)
    }
}
impl<'s, Storage: IKeFormatStorage<'s>> TryFrom<&mut KeFormatter<'s, Storage>> for OwnedKeyExpr {
    type Error = Error;

    fn try_from(value: &mut KeFormatter<'s, Storage>) -> Result<Self, Self::Error> {
        (&*value).try_into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FormatSetError {
    InvalidId,
    PatternNotMatched,
}
impl core::fmt::Display for FormatSetError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl IError for FormatSetError {}
impl<'s, Storage: IKeFormatStorage<'s>> KeFormatter<'s, Storage> {
    pub fn format(&self) -> &KeFormat<'s, Storage> {
        self.format
    }
    pub fn clear(&mut self) -> &mut Self {
        self.buffer.clear();
        for value in self.values.as_mut() {
            *value = None
        }
        self
    }
    pub fn build(&self) -> ZResult<OwnedKeyExpr> {
        self.try_into()
    }
    pub fn get(&self, id: &str) -> Option<&str> {
        let segments = self.format.storage.segments();
        segments
            .iter()
            .position(|s| s.spec.id() == id)
            .and_then(|i| {
                self.values.as_ref()[i]
                    .map(|(start, end)| &self.buffer[start as usize..end.get() as usize])
            })
    }
    pub fn set<S: Display>(&mut self, id: &str, value: S) -> Result<&mut Self, FormatSetError> {
        use core::fmt::Write;
        let segments = self.format.storage.segments();
        let Some(i) = segments.iter().position(|s|s.spec.id() == id) else {return Err(FormatSetError::InvalidId)};
        if let Some((start, end)) = self.values.as_ref()[i] {
            let end = end.get();
            let shift = end - start;
            self.buffer.replace_range(start as usize..end as usize, "");
            for (start, end) in self.values.as_mut()[(i + 1)..].iter_mut().flatten() {
                *start -= shift;
                *end = NonZeroU32::new(end.get() - shift).unwrap()
            }
        }
        let pattern = segments[i].spec.pattern();
        let start = self.buffer.len();
        write!(&mut self.buffer, "{value}").unwrap(); // Writing on `&mut String` should be infallible.
        match (|| {
            let end = self.buffer.len();
            if !(end == start && pattern.as_str() == "**") {
                let Ok(ke) = keyexpr::new(&self.buffer[start..end]) else {return Err(())};
                if end > u32::MAX as usize || !pattern.includes(ke) {
                    return Err(());
                }
            }
            self.values.as_mut()[i] = Some((start as u32, unsafe {
                // `end` must be >0 for self.buffer[start..end] to be a keyexpr
                NonZeroU32::new_unchecked(end as u32)
            }));
            Ok(())
        })() {
            Ok(()) => Ok(self),
            Err(()) => {
                self.buffer.truncate(start);
                Err(FormatSetError::PatternNotMatched)
            }
        }
    }
}

pub struct OwnedKeFormat<Storage: IKeFormatStorage<'static> + 'static = Vec<Segment<'static>>> {
    _owner: Box<str>,
    format: KeFormat<'static, Storage>,
}
impl<Storage: IKeFormatStorage<'static> + 'static> core::ops::Deref for OwnedKeFormat<Storage> {
    type Target = KeFormat<'static, Storage>;
    fn deref(&self) -> &Self::Target {
        &self.format
    }
}
impl<Storage: IKeFormatStorage<'static> + 'static> TryFrom<Box<str>> for OwnedKeFormat<Storage> {
    type Error = <KeFormat<'static, Storage> as TryFrom<&'static str>>::Error;
    fn try_from(value: Box<str>) -> Result<Self, Self::Error> {
        let owner = value;
        let format: KeFormat<'static, Storage> = unsafe {
            // This is safe because
            core::mem::transmute::<&str, &'static str>(&owner)
        }
        .try_into()?;
        Ok(Self {
            _owner: owner,
            format,
        })
    }
}
impl<Storage: IKeFormatStorage<'static> + 'static> TryFrom<String> for OwnedKeFormat<Storage> {
    type Error = <KeFormat<'static, Storage> as TryFrom<&'static str>>::Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.into_boxed_str().try_into()
    }
}
impl<Storage: IKeFormatStorage<'static> + 'static> core::str::FromStr for OwnedKeFormat<Storage> {
    type Err = <KeFormat<'static, Storage> as TryFrom<&'static str>>::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Box::<str>::from(s).try_into()
    }
}
impl<'s, S1: IKeFormatStorage<'s> + 's, S2: IKeFormatStorage<'s> + 's> PartialEq<KeFormat<'s, S2>>
    for KeFormat<'s, S1>
{
    fn eq(&self, other: &KeFormat<'s, S2>) -> bool {
        self.suffix == other.suffix && self.storage.segments() == other.storage.segments()
    }
}

#[test]
fn formatting() {
    let format = KeFormat::new("a/${a:*}/b/$#{b:**}#/c").unwrap();
    assert_eq!(format.storage[0].prefix, "a/");
    assert_eq!(format.storage[0].spec.id(), "a");
    assert_eq!(format.storage[0].spec.pattern(), "*");
    assert_eq!(format.storage[1].prefix, "/b/");
    assert_eq!(format.storage[1].spec.id(), "b");
    assert_eq!(format.storage[1].spec.pattern(), "**");
    assert_eq!(format.suffix, "/c");
    let ke: OwnedKeyExpr = format
        .formatter()
        .set("a", 1)
        .unwrap()
        .set("b", "hi/there")
        .unwrap()
        .try_into()
        .unwrap();
    assert_eq!(ke.as_str(), "a/1/b/hi/there/c");
    let ke: OwnedKeyExpr = format
        .formatter()
        .set("a", 1)
        .unwrap()
        .set("b", "")
        .unwrap()
        .try_into()
        .unwrap();
    assert_eq!(ke.as_str(), "a/1/b/c");
}

mod parsing;
pub use parsing::{Iter, Parsed};
