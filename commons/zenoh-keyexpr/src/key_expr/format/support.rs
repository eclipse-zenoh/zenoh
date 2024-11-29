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

use alloc::{boxed::Box, vec::Vec};
use core::convert::{TryFrom, TryInto};

use zenoh_result::{bail, zerror, Error};

use crate::key_expr::keyexpr;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Spec<'a> {
    pub(crate) spec: &'a str,
    pub(crate) id_end: u16,
    pub(crate) pattern_end: u16,
}
impl<'a> TryFrom<&'a str> for Spec<'a> {
    type Error = Error;
    fn try_from(spec: &'a str) -> Result<Self, Self::Error> {
        let Some(id_end) = spec.find(':') else {
            bail!("Spec {spec} didn't contain `:`")
        };
        let pattern_start = id_end + 1;
        let pattern_end = spec[pattern_start..].find('#').unwrap_or(u16::MAX as usize);
        if pattern_start < spec.len() {
            let Ok(id_end) = id_end.try_into() else {
                bail!("Spec {spec} contains an id longer than {}", u16::MAX)
            };
            if pattern_end > u16::MAX as usize {
                bail!("Spec {spec} contains a pattern longer than {}", u16::MAX)
            }
            Ok(Self {
                spec,
                id_end,
                pattern_end: pattern_end as u16,
            })
        } else {
            bail!("Spec {spec} has an empty pattern")
        }
    }
}
impl Spec<'_> {
    pub fn id(&self) -> &str {
        &self.spec[..self.id_end as usize]
    }
    pub fn pattern(&self) -> &keyexpr {
        unsafe {
            keyexpr::from_str_unchecked(if self.pattern_end != u16::MAX {
                &self.spec[(self.id_end + 1) as usize..self.pattern_end as usize]
            } else {
                &self.spec[(self.id_end + 1) as usize..]
            })
        }
    }
    pub fn default(&self) -> Option<&keyexpr> {
        let pattern_end = self.pattern_end as usize;
        (self.spec.len() > pattern_end)
            .then(|| unsafe { keyexpr::from_str_unchecked(&self.spec[(pattern_end + 1)..]) })
    }
}
impl core::fmt::Debug for Spec<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}
impl core::fmt::Display for Spec<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let spec = self.spec;
        if spec.contains('}') {
            write!(f, "$#{{{spec}}}#")
        } else {
            write!(f, "${{{spec}}}")
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Segment<'a> {
    /// What precedes a spec in a [`KeFormat`].
    /// It may be:
    /// - empty if the spec is the first thing in the format.
    /// - `/` if the spec comes right after another spec.
    /// - a valid keyexpr followed by `/` if the spec comes after a keyexpr.
    pub(crate) prefix: &'a str,
    pub(crate) spec: Spec<'a>,
}
impl Segment<'_> {
    pub fn prefix(&self) -> Option<&keyexpr> {
        match self.prefix {
            "" | "/" => None,
            _ => Some(unsafe {
                keyexpr::from_str_unchecked(trim_suffix_slash(trim_prefix_slash(self.prefix)))
            }),
        }
    }
    pub fn id(&self) -> &str {
        self.spec.id()
    }
    pub fn pattern(&self) -> &keyexpr {
        self.spec.pattern()
    }
    pub fn default(&self) -> Option<&keyexpr> {
        self.spec.default()
    }
}

pub enum IterativeConstructor<Complete, Partial, Error> {
    Complete(Complete),
    Partial(Partial),
    Error(Error),
}
pub trait IKeFormatStorage<'s>: Sized {
    type PartialConstruct;
    type ConstructionError: core::fmt::Display;
    fn new_constructor(
    ) -> IterativeConstructor<Self, Self::PartialConstruct, Self::ConstructionError>;
    fn add_segment(
        constructor: IterativeConstructor<Self, Self::PartialConstruct, Self::ConstructionError>,
        segment: Segment<'s>,
    ) -> IterativeConstructor<Self, Self::PartialConstruct, Self::ConstructionError>;
    fn segments(&self) -> &[Segment<'s>];
    fn segments_mut(&mut self) -> &mut [Segment<'s>];
    fn segment(&self, id: &str) -> Option<&Segment<'s>> {
        self.segments().iter().find(|s| s.spec.id() == id)
    }
    fn segment_mut(&mut self, id: &str) -> Option<&mut Segment<'s>> {
        self.segments_mut().iter_mut().find(|s| s.spec.id() == id)
    }
    type ValuesStorage<T>: AsMut<[T]> + AsRef<[T]>;
    fn values_storage<T, F: FnMut(usize) -> T>(&self, f: F) -> Self::ValuesStorage<T>;
}

impl<'s, const N: usize> IKeFormatStorage<'s> for [Segment<'s>; N] {
    type PartialConstruct = ([core::mem::MaybeUninit<Segment<'s>>; N], u16);
    type ConstructionError = Error;
    fn new_constructor(
    ) -> IterativeConstructor<Self, Self::PartialConstruct, Self::ConstructionError> {
        if N > u16::MAX as usize {
            IterativeConstructor::Error(
                zerror!(
                    "[Segments; {N}] unsupported because {N} is too big (max: {}).",
                    u16::MAX
                )
                .into(),
            )
        } else {
            IterativeConstructor::Partial(([core::mem::MaybeUninit::uninit(); N], 0))
        }
    }
    fn add_segment(
        constructor: IterativeConstructor<Self, Self::PartialConstruct, Self::ConstructionError>,
        segment: Segment<'s>,
    ) -> IterativeConstructor<Self, Self::PartialConstruct, Self::ConstructionError> {
        match constructor {
            IterativeConstructor::Complete(_) => IterativeConstructor::Error(
                zerror!("Attempted to add more than {N} segments to [Segment<'s>; {N}]").into(),
            ),
            IterativeConstructor::Partial((mut this, n)) => {
                let mut n = n as usize;
                this[n] = core::mem::MaybeUninit::new(segment);
                n += 1;
                if n == N {
                    IterativeConstructor::Complete(this.map(|e| unsafe { e.assume_init() }))
                } else {
                    IterativeConstructor::Partial((this, n as u16))
                }
            }
            IterativeConstructor::Error(e) => IterativeConstructor::Error(e),
        }
    }

    fn segments(&self) -> &[Segment<'s>] {
        self
    }
    fn segments_mut(&mut self) -> &mut [Segment<'s>] {
        self
    }

    type ValuesStorage<T> = [T; N];
    fn values_storage<T, F: FnMut(usize) -> T>(&self, mut f: F) -> Self::ValuesStorage<T> {
        let mut values = PartialSlice::new();
        for i in 0..N {
            values.push(f(i));
        }
        match values.try_into() {
            Ok(v) => v,
            Err(_) => unreachable!(),
        }
    }
}
struct PartialSlice<T, const N: usize> {
    buffer: [core::mem::MaybeUninit<T>; N],
    n: u16,
}

impl<T, const N: usize> PartialSlice<T, N> {
    fn new() -> Self {
        Self {
            buffer: [(); N].map(|_| core::mem::MaybeUninit::uninit()),
            n: 0,
        }
    }
    fn push(&mut self, value: T) {
        self.buffer[self.n as usize] = core::mem::MaybeUninit::new(value);
        self.n += 1;
    }
}
impl<T, const N: usize> TryFrom<PartialSlice<T, N>> for [T; N] {
    type Error = PartialSlice<T, N>;
    fn try_from(value: PartialSlice<T, N>) -> Result<Self, Self::Error> {
        let buffer = unsafe { core::ptr::read(&value.buffer) };
        if value.n as usize == N {
            core::mem::forget(value);
            Ok(buffer.map(|v| unsafe { v.assume_init() }))
        } else {
            Err(value)
        }
    }
}
impl<T, const N: usize> Drop for PartialSlice<T, N> {
    fn drop(&mut self) {
        for i in 0..self.n as usize {
            unsafe { core::mem::MaybeUninit::assume_init_drop(&mut self.buffer[i]) }
        }
    }
}

impl<'s> IKeFormatStorage<'s> for Vec<Segment<'s>> {
    type PartialConstruct = core::convert::Infallible;
    type ConstructionError = core::convert::Infallible;
    fn new_constructor(
    ) -> IterativeConstructor<Self, Self::PartialConstruct, Self::ConstructionError> {
        IterativeConstructor::Complete(Self::new())
    }
    fn add_segment(
        constructor: IterativeConstructor<Self, Self::PartialConstruct, Self::ConstructionError>,
        segment: Segment<'s>,
    ) -> IterativeConstructor<Self, Self::PartialConstruct, Self::ConstructionError> {
        // NOTE(fuzzypixelz): Rust 1.82.0 can detect that this pattern is irrefutable but that's not
        // necessarily the case for prior versions. Thus we silence this lint to keep the MSRV minimal.
        #[allow(irrefutable_let_patterns)]
        let IterativeConstructor::Complete(mut this) = constructor
        else {
            unsafe { core::hint::unreachable_unchecked() }
        };
        this.push(segment);
        IterativeConstructor::Complete(this)
    }

    fn segments(&self) -> &[Segment<'s>] {
        self
    }
    fn segments_mut(&mut self) -> &mut [Segment<'s>] {
        self
    }

    type ValuesStorage<T> = Box<[T]>;
    fn values_storage<T, F: FnMut(usize) -> T>(&self, mut f: F) -> Self::ValuesStorage<T> {
        let mut ans = Vec::with_capacity(self.len());
        for i in 0..self.len() {
            ans.push(f(i))
        }
        ans.into()
    }
}

/// Trim the prefix slash from a target string if it has one.
/// # Safety
/// `target` is assumed to be a valid `keyexpr` except for the leading slash.
pub(crate) fn trim_prefix_slash(target: &str) -> &str {
    &target[matches!(target.as_bytes().first(), Some(b'/')) as usize..]
}
pub(crate) fn trim_suffix_slash(target: &str) -> &str {
    &target[..(target.len() - matches!(target.as_bytes().last(), Some(b'/')) as usize)]
}
