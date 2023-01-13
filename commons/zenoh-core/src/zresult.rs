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
use anyhow::Error as AnyError;
use core::{any::Any, fmt};

#[cfg(feature = "std")]
pub type Error = Box<dyn IError + Send + Sync + 'static>;
#[cfg(not(feature = "std"))]
pub type Error = alloc::boxed::Box<dyn IError + Send + Sync + 'static>;
pub use std_impl::IError;
impl dyn IError {
    pub fn is<T: 'static>(&self) -> bool {
        Any::type_id(self) == core::any::TypeId::of::<T>()
    }
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        self.is::<T>()
            .then(|| unsafe { &*(self as *const dyn IError as *const T) })
    }
}
impl dyn IError + Send {
    pub fn is<T: 'static>(&self) -> bool {
        Any::type_id(self) == core::any::TypeId::of::<T>()
    }
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        self.is::<T>()
            .then(|| unsafe { &*(self as *const dyn IError as *const T) })
    }
}
impl dyn IError + Send + Sync {
    pub fn is<T: 'static>(&self) -> bool {
        Any::type_id(self) == core::any::TypeId::of::<T>()
    }
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        self.is::<T>()
            .then(|| unsafe { &*(self as *const dyn IError as *const T) })
    }
}
#[cfg(feature = "std")]
mod std_impl {
    use super::*;
    pub trait IError: std::error::Error + core::any::Any {}
    impl<T: std::error::Error + core::any::Any> IError for T {}
    impl<T: IError + Send + Sync + 'static> From<T> for super::Error {
        fn from(value: T) -> Self {
            Box::new(value)
        }
    }
    impl std::error::Error for ZError {
        fn source(&self) -> Option<&'_ (dyn std::error::Error + 'static)> {
            self.source
                .as_ref()
                .map(|r| unsafe { core::mem::transmute(r.as_ref()) })
        }
    }
    impl std::error::Error for ShmError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            self.0.source()
        }
    }
    impl ErrNo for dyn std::error::Error {
        fn errno(&self) -> NegativeI8 {
            match self.downcast_ref::<ZError>() {
                Some(e) => e.errno(),
                None => NegativeI8::new(i8::MIN),
            }
        }
    }
    impl ErrNo for dyn std::error::Error + Send {
        fn errno(&self) -> NegativeI8 {
            match self.downcast_ref::<ZError>() {
                Some(e) => e.errno(),
                None => NegativeI8::new(i8::MIN),
            }
        }
    }
    impl ErrNo for dyn std::error::Error + Send + Sync {
        fn errno(&self) -> NegativeI8 {
            match self.downcast_ref::<ZError>() {
                Some(e) => e.errno(),
                None => NegativeI8::new(i8::MIN),
            }
        }
    }
}
#[cfg(not(feature = "std"))]
mod std_impl {
    use super::*;
    use core::fmt;
    pub trait IError: fmt::Debug + fmt::Display + core::any::Any {}
    impl<T: IError + Send + Sync + 'static> From<T> for super::Error {
        fn from(value: T) -> super::Error {
            alloc::boxed::Box::new(value)
        }
    }
    impl IError for super::ShmError {}
    impl IError for super::ZError {}
    impl ErrNo for dyn IError {
        fn errno(&self) -> NegativeI8 {
            match self.downcast_ref::<ZError>() {
                Some(e) => e.errno(),
                None => NegativeI8::new(i8::MIN),
            }
        }
    }
    impl ErrNo for dyn IError + Send {
        fn errno(&self) -> NegativeI8 {
            match self.downcast_ref::<ZError>() {
                Some(e) => e.errno(),
                None => NegativeI8::new(i8::MIN),
            }
        }
    }
    impl ErrNo for dyn IError + Send + Sync {
        fn errno(&self) -> NegativeI8 {
            match self.downcast_ref::<ZError>() {
                Some(e) => e.errno(),
                None => NegativeI8::new(i8::MIN),
            }
        }
    }
}

pub type ZResult<T> = Result<T, Error>;
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NegativeI8(i8);
impl NegativeI8 {
    pub const fn new(v: i8) -> Self {
        if v >= 0 {
            panic!("Non-negative value used in NegativeI8")
        }
        NegativeI8(v)
    }
    pub const fn get(self) -> i8 {
        self.0
    }
    pub const MIN: Self = Self::new(i8::MIN);
}

pub struct ZError {
    error: AnyError,
    file: &'static str,
    line: u32,
    errno: NegativeI8,
    source: Option<Error>,
}

unsafe impl Send for ZError {}
unsafe impl Sync for ZError {}

impl ZError {
    pub fn new<E: Into<AnyError>>(
        error: E,
        file: &'static str,
        line: u32,
        errno: NegativeI8,
    ) -> ZError {
        ZError {
            error: error.into(),
            file,
            line,
            errno,
            source: None,
        }
    }
    pub fn set_source<S: Into<Error>>(mut self, source: S) -> Self {
        self.source = Some(source.into());
        self
    }
}

impl fmt::Debug for ZError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self, f)
    }
}
impl fmt::Display for ZError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}:{}.", self.error, self.file, self.line)?;
        if let Some(s) = &self.source {
            write!(f, " - Caused by {}", *s)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct ShmError(pub ZError);
impl fmt::Display for ShmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

pub trait ErrNo {
    fn errno(&self) -> NegativeI8;
}
impl ErrNo for ZError {
    fn errno(&self) -> NegativeI8 {
        self.errno
    }
}
impl ErrNo for ShmError {
    fn errno(&self) -> NegativeI8 {
        self.0.errno
    }
}
