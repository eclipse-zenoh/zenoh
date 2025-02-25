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

use std::{
    ffi::c_void,
    fmt::Display,
    num::NonZeroUsize,
    os::fd::{AsRawFd, OwnedFd},
    ptr::NonNull,
};

use nix::{
    fcntl::OFlag,
    sys::{
        mman::{mmap, munmap, shm_open, shm_unlink, MapFlags, ProtFlags},
        stat::{fstat, Mode},
    },
    unistd::ftruncate,
};
use num_traits::Unsigned;

use super::{SegmentCreateError, SegmentOpenError, ShmCreateResult, ShmOpenResult};

pub struct SegmentImpl<ID: Unsigned + Display + Copy> {
    fd: OwnedFd,
    len: NonZeroUsize,
    data_ptr: NonNull<c_void>,
    id: ID,
}

// PUBLIC
impl<ID: Unsigned + Display + Copy> SegmentImpl<ID> {
    pub fn create(id: ID, len: NonZeroUsize) -> ShmCreateResult<Self> {
        // create unique shm fd
        let fd = {
            let id = Self::id_str(id);
            let flags = OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_RDWR;
            let mode = Mode::S_IRUSR | Mode::S_IWUSR;
            tracing::trace!("shm_open(name={}, flag={:?}, mode={:?})", id, flags, mode);
            match shm_open(id.as_str(), flags, mode) {
                Ok(v) => v,
                Err(nix::Error::EEXIST) => return Err(SegmentCreateError::SegmentExists),
                Err(e) => return Err(SegmentCreateError::OsError(e as u32)),
            }
        };

        // resize shm segment to requested size
        tracing::trace!("ftruncate(fd={}, len={})", fd.as_raw_fd(), len);
        if let Err(e) = ftruncate(&fd, len.get() as _) {
            return Err(SegmentCreateError::OsError(e as u32));
        };

        // map segment into our address space
        let data_ptr = Self::map(len, &fd).map_err(|e| SegmentCreateError::OsError(e as _))?;

        Ok(Self {
            fd,
            len,
            data_ptr,
            id,
        })
    }

    pub fn open(id: ID) -> ShmOpenResult<Self> {
        // open shm fd
        let fd = {
            let id = Self::id_str(id);
            let flags = OFlag::O_RDWR;
            let mode = Mode::S_IRUSR;
            tracing::trace!("shm_open(name={}, flag={:?}, mode={:?})", id, flags, mode);
            match shm_open(id.as_str(), flags, mode) {
                Ok(v) => v,
                Err(e) => return Err(SegmentOpenError::OsError(e as u32)),
            }
        };

        // get segment size
        let len = match fstat(fd.as_raw_fd()) {
            Ok(v) => v.st_size as usize,
            Err(e) => return Err(SegmentOpenError::OsError(e as u32)),
        };
        let len = NonZeroUsize::new(len).ok_or(SegmentOpenError::ZeroSizedSegment)?;

        // map segment into our address space
        let data_ptr = Self::map(len, &fd).map_err(|e| SegmentOpenError::OsError(e as _))?;

        Ok(Self {
            fd,
            len,
            data_ptr,
            id,
        })
    }

    pub fn unlink(&self) {
        let id = Self::id_str(self.id);
        tracing::trace!("shm_unlink(name={})", id);
        let _ = shm_unlink(id.as_str());
    }

    pub fn id(&self) -> ID {
        self.id
    }

    pub fn len(&self) -> NonZeroUsize {
        self.len
    }

    pub fn as_ptr(&self) -> *mut u8 {
        self.data_ptr.as_ptr() as _
    }
}

// PRIVATE
impl<ID: Unsigned + Display + Copy> SegmentImpl<ID> {
    fn id_str(id: ID) -> String {
        format!("/{}.zenoh", id)
    }

    fn map(len: NonZeroUsize, fd: &OwnedFd) -> nix::Result<NonNull<c_void>> {
        let prot = ProtFlags::PROT_READ | ProtFlags::PROT_WRITE;
        let flags = MapFlags::MAP_SHARED;

        tracing::trace!(
            "mmap(addr=NULL, length={}, prot={:X}, flags={:X}, f={}, offset=0)",
            len,
            prot,
            flags,
            fd.as_raw_fd()
        );

        unsafe { mmap(None, len, prot, flags, fd, 0) }
    }
}

impl<ID: Unsigned + Display + Copy> Drop for SegmentImpl<ID> {
    fn drop(&mut self) {
        tracing::trace!("munmap(addr={:p},len={})", self.data_ptr, self.len);
        if let Err(_e) = unsafe { munmap(self.data_ptr, self.len.get()) } {
            tracing::debug!("munmap() failed : {}", _e);
        };
    }
}
