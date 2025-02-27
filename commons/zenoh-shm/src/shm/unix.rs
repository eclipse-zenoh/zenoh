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

use std::{
    ffi::c_void,
    num::NonZeroUsize,
    os::fd::{AsRawFd, OwnedFd},
    ptr::NonNull,
};

// todo: flock() doesn't work on Mac in some cases, but we can fix it
#[cfg(target_os = "linux")]
use advisory_lock::{AdvisoryFileLock, FileLockMode};
use nix::{
    fcntl::OFlag,
    sys::{
        mman::{mmap, munmap, shm_open, shm_unlink, MapFlags, ProtFlags},
        stat::{fstat, Mode},
    },
    unistd::ftruncate,
};

use super::{SegmentCreateError, SegmentID, SegmentOpenError, ShmCreateResult, ShmOpenResult};

pub struct SegmentImpl<ID: SegmentID> {
    #[cfg(target_os = "linux")]
    fd: OwnedFd,
    len: NonZeroUsize,
    data_ptr: NonNull<c_void>,
    id: ID,
}

// PUBLIC
impl<ID: SegmentID> SegmentImpl<ID> {
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

        // todo: flock() doesn't work on Mac in some cases, but we can fix it
        #[cfg(target_os = "linux")]
        // put shared advisory lock on shm fd
        fd.as_raw_fd()
            .try_lock(FileLockMode::Shared)
            .map_err(|e| match e {
                advisory_lock::FileLockError::AlreadyLocked => SegmentCreateError::SegmentExists,
                advisory_lock::FileLockError::Io(e) => {
                    SegmentCreateError::OsError(e.raw_os_error().unwrap_or(0) as _)
                }
            })?;

        // resize shm segment to requested size
        tracing::trace!("ftruncate(fd={}, len={})", fd.as_raw_fd(), len);
        ftruncate(&fd, len.get() as _).map_err(|e| SegmentCreateError::OsError(e as u32))?;

        // get real segment size
        let len = {
            let stat = fstat(fd.as_raw_fd()).map_err(|e| SegmentCreateError::OsError(e as u32))?;
            NonZeroUsize::new(stat.st_size as usize).ok_or(SegmentCreateError::OsError(0))?
        };

        // map segment into our address space
        let data_ptr = Self::map(len, &fd).map_err(|e| SegmentCreateError::OsError(e as _))?;

        Ok(Self {
            #[cfg(target_os = "linux")]
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
            shm_open(id.as_str(), flags, mode).map_err(|e| SegmentOpenError::OsError(e as u32))?
        };

        // todo: flock() doesn't work on Mac in some cases, but we can fix it
        #[cfg(target_os = "linux")]
        // put shared advisory lock on shm fd
        fd.as_raw_fd()
            .try_lock(FileLockMode::Shared)
            .map_err(|e| match e {
                advisory_lock::FileLockError::AlreadyLocked => SegmentOpenError::InvalidatedSegment,
                advisory_lock::FileLockError::Io(e) => {
                    SegmentOpenError::OsError(e.raw_os_error().unwrap_or(0) as _)
                }
            })?;

        // get real segment size
        let len = {
            let stat = fstat(fd.as_raw_fd()).map_err(|e| SegmentOpenError::OsError(e as u32))?;
            NonZeroUsize::new(stat.st_size as usize).ok_or(SegmentOpenError::OsError(0))?
        };

        // map segment into our address space
        let data_ptr = Self::map(len, &fd).map_err(|e| SegmentOpenError::OsError(e as _))?;

        Ok(Self {
            #[cfg(target_os = "linux")]
            fd,
            len,
            data_ptr,
            id,
        })
    }

    pub fn ensure_not_persistent(id: ID) {
        // open shm fd
        let fd = {
            let id = Self::id_str(id);
            let flags = OFlag::O_RDWR;
            let mode = Mode::S_IRUSR;
            tracing::trace!("shm_open(name={}, flag={:?}, mode={:?})", id, flags, mode);
            shm_open(id.as_str(), flags, mode)
        };

        if let Ok(fd) = fd {
            Self::unlink_if_unique(id, &fd);
        }
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
impl<ID: SegmentID> SegmentImpl<ID> {
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

    #[allow(unused_variables)]
    fn unlink_if_unique(id: ID, fd: &OwnedFd) {
        // todo: flock() doesn't work on Mac in some cases, but we can fix it
        #[cfg(target_os = "linux")]
        if fd.as_raw_fd().try_lock(FileLockMode::Exclusive).is_ok() {
            let id = Self::id_str(id);
            tracing::trace!("shm_unlink(name={})", id);
            let _ = shm_unlink(id.as_str());
        }
    }
}

impl<ID: SegmentID> Drop for SegmentImpl<ID> {
    fn drop(&mut self) {
        tracing::trace!("munmap(addr={:p},len={})", self.data_ptr, self.len);
        if let Err(_e) = unsafe { munmap(self.data_ptr, self.len.get()) } {
            tracing::debug!("munmap() failed : {}", _e);
        };

        #[cfg(target_os = "linux")]
        Self::unlink_if_unique(self.id, &self.fd);

        #[cfg(not(target_os = "linux"))]
        {
            let id = Self::id_str(self.id);
            tracing::trace!("shm_unlink(name={})", id);
            let _ = shm_unlink(id.as_str());
        }
    }
}
