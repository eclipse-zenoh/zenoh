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
use anyhow::Error;
use std::fmt;

pub type BoxedStdErr = Box<dyn std::error::Error + Send + Sync + 'static>;

pub type ZResult<T> = Result<T, BoxedStdErr>;

#[derive(Debug)]
pub struct ZError {
    error: Error,
    file: &'static str,
    line: u32,
    source: Option<BoxedStdErr>,
}

unsafe impl Send for ZError {}
unsafe impl Sync for ZError {}

impl ZError {
    pub fn new<E: Into<Error>>(error: E, file: &'static str, line: u32) -> ZError {
        ZError {
            error: error.into(),
            file,
            line,
            source: None,
        }
    }
    pub fn set_source<S: Into<BoxedStdErr>>(mut self, source: S) -> Self {
        self.source = Some(source.into());
        self
    }
}

impl std::error::Error for ZError {
    fn source(&self) -> Option<&'_ (dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|r| unsafe { std::mem::transmute(r.as_ref()) })
    }
}

impl fmt::Display for ZError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at {}:{}.",
            self.error,
            self.file,
            self.line
        )?;
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
impl std::error::Error for ShmError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}
