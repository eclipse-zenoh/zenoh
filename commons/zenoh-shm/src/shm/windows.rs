//
// Copyright (c) 2025 ZettaScale Technology
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

use std::num::NonZeroUsize;

use win_sys::*;
use winapi::um::errhandlingapi::GetLastError;

use super::{SegmentCreateError, SegmentID, SegmentOpenError, ShmCreateResult, ShmOpenResult};

pub struct SegmentImpl<ID: SegmentID> {
    _fd: FileMapping,
    len: NonZeroUsize,
    data_ptr: ViewOfFile,
    id: ID,
}

// PUBLIC
impl<ID: SegmentID> SegmentImpl<ID> {
    pub fn create(id: ID, len: NonZeroUsize) -> ShmCreateResult<Self> {
        let fd = {
            let id = Self::id_str(id);
            let high_size = ((len.get() as u64 & 0xFFFF_FFFF_0000_0000_u64) >> 32) as _;
            let low_size = (len.get() as u64 & 0xFFFF_FFFF_u64) as _;
            tracing::trace!(
                "CreateFileMapping({:?}, NULL, {:X}, {}, {}, '{}')",
                INVALID_HANDLE_VALUE,
                PAGE_READWRITE.0,
                high_size,
                low_size,
                id,
            );

            // If the mapping already exists, GetLastError() will return ERROR_ALREADY_EXISTS,
            // and you'll receive a handle to the existing mapping instead of creating a new one.
            let fd = CreateFileMapping(
                INVALID_HANDLE_VALUE,
                None,
                PAGE_READWRITE,
                high_size,
                low_size,
                id.as_str(),
            )
            .map_err(|e| match e.win32_error().unwrap() {
                ERROR_ALREADY_EXISTS => SegmentCreateError::SegmentExists,
                err_code => SegmentCreateError::OsError(err_code.0 as _),
            })?;

            // check error
            if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS.0 {
                return Err(SegmentCreateError::SegmentExists);
            }

            fd
        };

        let (data_ptr, len) =
            Self::map(&fd).map_err(|e| SegmentCreateError::OsError(e.win32_error().unwrap().0))?;

        let len = len
            .try_into()
            .map_err(|_e| SegmentCreateError::OsError(0))?;

        Ok(Self {
            _fd: fd,
            len,
            data_ptr,
            id,
        })
    }

    pub fn open(id: ID) -> ShmOpenResult<Self> {
        let fd = {
            let id = Self::id_str(id);
            tracing::trace!(
                "OpenFileMappingW({:?}, {}, '{}')",
                FILE_MAP_ALL_ACCESS,
                false,
                id,
            );

            OpenFileMapping(FILE_MAP_ALL_ACCESS, false, id.as_str())
                .map_err(|e| SegmentOpenError::OsError(e.win32_error().unwrap().0))
        }?;

        let (data_ptr, len) =
            Self::map(&fd).map_err(|e| SegmentOpenError::OsError(e.win32_error().unwrap().0))?;

        let len = len
            .try_into()
            .map_err(|_| SegmentOpenError::InvalidatedSegment)?;

        Ok(Self {
            _fd: fd,
            len,
            data_ptr,
            id,
        })
    }

    pub fn id(&self) -> ID {
        self.id
    }

    pub fn len(&self) -> NonZeroUsize {
        self.len
    }

    pub fn as_ptr(&self) -> *mut u8 {
        self.data_ptr.as_mut_ptr() as _
    }
}

// PRIVATE
impl<ID: SegmentID> SegmentImpl<ID> {
    fn id_str(id: ID) -> String {
        format!("{id}.zenoh")
    }

    fn map(fd: &FileMapping) -> Result<(ViewOfFile, usize), Error> {
        let data_ptr = {
            tracing::trace!(
                "MapViewOfFile(0x{:X}, {:X}, 0, 0, 0)",
                fd,
                (FILE_MAP_READ | FILE_MAP_WRITE).0,
            );
            MapViewOfFile(fd.as_handle(), FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, 0)
        }?;

        let len = {
            let mut info = MEMORY_BASIC_INFORMATION::default();
            VirtualQuery(data_ptr.as_mut_ptr(), &mut info)?;
            info.RegionSize
        };

        Ok((data_ptr, len))
    }
}
