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

//! ⚠️ WARNING ⚠️
//!
//! This crate is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](../zenoh/index.html)
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

use anyhow::Error as AnyError;
use core::fmt;

#[cfg(not(feature = "std"))]
use alloc::string::String;

// +-------+
// | ERROR |
// +-------+

#[cfg(feature = "std")]
pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>;
#[cfg(not(feature = "std"))]
pub type Error = String;

// +---------+
// | ZRESULT |
// +---------+

pub type ZResult<T> = core::result::Result<T, Error>;

// +--------+
// | ZERROR |
// +--------+

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ZError {
    #[cfg_attr(feature = "defmt", defmt(Debug2Format))]
    error: AnyError,
    file: &'static str,
    line: u32,
    errno: NegativeI8,
    #[cfg_attr(all(feature = "defmt", feature = "std"), defmt(Debug2Format))]
    // Use fmt::Debug only for std version of Error
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

#[cfg(feature = "std")]
impl std::error::Error for ZError {
    fn source(&self) -> Option<&'_ (dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|r| unsafe { core::mem::transmute(r.as_ref()) })
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

// +----------+
// | SHMERROR |
// +----------+

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ShmError(pub ZError);

impl fmt::Display for ShmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ShmError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

// +-------+
// | ERRNO |
// +-------+

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

#[cfg(feature = "std")]
impl ErrNo for dyn std::error::Error {
    fn errno(&self) -> NegativeI8 {
        match self.downcast_ref::<ZError>() {
            Some(e) => e.errno(),
            None => NegativeI8::new(i8::MIN),
        }
    }
}

#[cfg(feature = "std")]
impl ErrNo for dyn std::error::Error + Send {
    fn errno(&self) -> NegativeI8 {
        match self.downcast_ref::<ZError>() {
            Some(e) => e.errno(),
            None => NegativeI8::new(i8::MIN),
        }
    }
}

#[cfg(feature = "std")]
impl ErrNo for dyn std::error::Error + Send + Sync {
    fn errno(&self) -> NegativeI8 {
        match self.downcast_ref::<ZError>() {
            Some(e) => e.errno(),
            None => NegativeI8::new(i8::MIN),
        }
    }
}

// +--------+
// | MACROS |
// +--------+

pub use anyhow::anyhow;
#[macro_export]
macro_rules! zerror {
    (($errno:expr) $source: expr => $($t: tt)*) => {
        $crate::ZError::new($crate::anyhow!($($t)*), file!(), line!(), $crate::NegativeI8::new($errno as i8)).set_source($source)
    };
    (($errno:expr) $t: literal) => {
        $crate::ZError::new($crate::anyhow!($t), file!(), line!(), $crate::NegativeI8::new($errno as i8))
    };
    (($errno:expr) $t: expr) => {
        $crate::ZError::new($t, file!(), line!(), $crate::NegativeI8::new($errno as i8))
    };
    (($errno:expr) $($t: tt)*) => {
        $crate::ZError::new($crate::anyhow!($($t)*), file!(), line!(), $crate::NegativeI8::new($errno as i8))
    };
    ($source: expr => $($t: tt)*) => {
        $crate::ZError::new($crate::anyhow!($($t)*), file!(), line!(), $crate::NegativeI8::MIN).set_source($source)
    };
    ($t: literal) => {
        $crate::ZError::new($crate::anyhow!($t), file!(), line!(), $crate::NegativeI8::MIN)
    };
    ($t: expr) => {
        $crate::ZError::new($t, file!(), line!(), $crate::NegativeI8::MIN)
    };
    ($($t: tt)*) => {
        $crate::ZError::new($crate::anyhow!($($t)*), file!(), line!(), $crate::NegativeI8::MIN)
    };
}

// @TODO: re-design ZError and macros
// This macro is a shorthand for the creation of a ZError
#[macro_export]
macro_rules! bail{
    ($($t: tt)*) => {
        return Err($crate::zerror!($($t)*).into())
    };
}

// This macro is a shorthand for the conversion of any Error into a ZError
#[macro_export]
macro_rules! to_zerror {
    ($kind:ident, $descr:expr) => {
        |e| Err(zerror!($kind, $descr, e))
    };
}
