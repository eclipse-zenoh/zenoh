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
use std::fmt;

pub type ZResult<T> = Result<T, ZError>;

#[derive(Debug, PartialEq)]
pub enum ZErrorKind {
    BufferOverflow {
        missing: usize,
    },
    BufferUnderflow {
        missing: usize,
    },
    InvalidLink {
        descr: String,
    },
    InvalidLocator {
        descr: String,
    },
    InvalidMessage {
        descr: String,
    },
    InvalidReference {
        descr: String,
    },
    InvalidResolution {
        descr: String,
    },
    InvalidSession {
        descr: String,
    },
    InvalidPath {
        path: String,
    },
    InvalidPathExpr {
        path: String,
    },
    InvalidSelector {
        selector: String,
    },
    IoError {
        descr: String,
    },
    Other {
        descr: String,
    },
    Timeout {},
    UnkownResourceId {
        rid: String,
    },
    ValueDecodingFailed {
        descr: String,
    },
    TranscodingFailed {
        origin_encoding: String,
        target_encoding: String,
    },
    SharedMemoryError {
        descr: String,
    },
}

impl fmt::Display for ZErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ZErrorKind::BufferOverflow { missing } => write!(
                f,
                "Failed to write in full buffer ({} bytes missing)",
                missing
            ),
            ZErrorKind::BufferUnderflow { missing } => write!(
                f,
                "Failed to read from empty buffer ({} bytes missing)",
                (if *missing == 0 {
                    "some".to_string()
                } else {
                    missing.to_string()
                })
            ),
            ZErrorKind::InvalidLocator { descr } => write!(f, "Invalid locator ({})", descr),
            ZErrorKind::InvalidLink { descr } => write!(f, "Invalid link ({})", descr),
            ZErrorKind::InvalidMessage { descr } => write!(f, "Invalid message ({})", descr),
            ZErrorKind::InvalidReference { descr } => write!(f, "Invalid Reference ({})", descr),
            ZErrorKind::InvalidResolution { descr } => write!(f, "Invalid Resolution ({})", descr),
            ZErrorKind::InvalidSession { descr } => write!(f, "Invalid Session ({})", descr),
            ZErrorKind::InvalidPath { path } => write!(f, "Invalid Path ({})", path),
            ZErrorKind::InvalidPathExpr { path } => write!(f, "Invalid PathExpr ({})", path),
            ZErrorKind::InvalidSelector { selector } => {
                write!(f, "Invalid Selector ({})", selector)
            }
            ZErrorKind::IoError { descr } => write!(f, "IO error ({})", descr),
            ZErrorKind::Other { descr } => write!(f, "zenoh error: ({})", descr),
            ZErrorKind::Timeout {} => write!(f, "Timeout"),
            ZErrorKind::UnkownResourceId { rid } => write!(f, "Unkown ResourceId ({})", rid),
            ZErrorKind::ValueDecodingFailed { descr } => {
                write!(f, "Failed to decode Value ({})", descr)
            }
            ZErrorKind::TranscodingFailed {
                origin_encoding,
                target_encoding,
            } => write!(
                f,
                "Failed to transcode Value from {} to {}",
                origin_encoding, target_encoding
            ),
            ZErrorKind::SharedMemoryError { descr } => write!(f, "Shared Memory error ({})", descr),
        }
    }
}

#[derive(Debug)]
pub struct ZErrorNoLife {
    file: &'static str,
}

#[derive(Debug)]
pub struct ZError {
    kind: ZErrorKind,
    file: &'static str,
    line: u32,
    source: Option<Box<dyn std::error::Error>>,
}

unsafe impl Send for ZError {}
unsafe impl Sync for ZError {}

impl ZError {
    pub fn new(
        kind: ZErrorKind,
        file: &'static str,
        line: u32,
        source: Option<Box<dyn std::error::Error>>,
    ) -> ZError {
        ZError {
            kind,
            file,
            line,
            source,
        }
    }

    pub fn get_kind(&self) -> &ZErrorKind {
        &self.kind
    }
}

impl std::error::Error for ZError {
    fn source(&self) -> Option<&'_ (dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|bx| bx.as_ref())
    }
}

impl fmt::Display for ZError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at {}:{}.",
            self.kind.to_string(),
            self.file,
            self.line
        )?;
        if let Some(s) = &self.source {
            write!(f, " - Caused by {}", *s)?;
        }
        Ok(())
    }
}
