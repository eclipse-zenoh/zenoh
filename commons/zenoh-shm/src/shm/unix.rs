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

#[cfg(any(bsd, target_os = "redox"))]
use std::mem::ManuallyDrop;
use std::{
    ffi::c_void,
    num::NonZeroUsize,
    os::fd::{AsRawFd, OwnedFd},
    ptr::NonNull,
};

// we use flock() on non-BSD systems
#[cfg(not(any(bsd, target_os = "redox")))]
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
    #[cfg(not(any(bsd, target_os = "redox")))]
    fd: OwnedFd,
    #[cfg(any(bsd, target_os = "redox"))]
    fd: ManuallyDrop<OwnedFd>,
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

            // open shm file with shared lock (BSD feature)
            #[cfg(any(bsd, target_os = "redox"))]
            let flags = flags | OFlag::O_SHLOCK | OFlag::O_NONBLOCK;

            // todo: these flags probably can be exposed to the config
            let mode = Mode::S_IRUSR | Mode::S_IWUSR;

            tracing::trace!("shm_open(name={}, flag={:?}, mode={:?})", id, flags, mode);
            match shm_open(id.as_str(), flags, mode) {
                Ok(v) => v,
                #[cfg(any(bsd, target_os = "redox"))]
                Err(nix::Error::EWOULDBLOCK) => return Err(SegmentCreateError::SegmentExists),
                Err(nix::Error::EEXIST) => return Err(SegmentCreateError::SegmentExists),
                Err(e) => return Err(SegmentCreateError::OsError(e as u32)),
            }
        };

        // we use flock() on non-BSD systems
        #[cfg(not(any(bsd, target_os = "redox")))]
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
            #[cfg(any(bsd, target_os = "redox"))]
            let flags = flags | OFlag::O_SHLOCK | OFlag::O_NONBLOCK;
            let mode = Mode::S_IRUSR;
            tracing::trace!("shm_open(name={}, flag={:?}, mode={:?})", id, flags, mode);

            match shm_open(id.as_str(), flags, mode) {
                Ok(v) => v,
                #[cfg(any(bsd, target_os = "redox"))]
                Err(nix::Error::EWOULDBLOCK) => return Err(SegmentOpenError::InvalidatedSegment),
                Err(e) => return Err(SegmentOpenError::OsError(e as u32)),
            }
        };

        // we use flock() on non-BSD systems
        #[cfg(not(any(bsd, target_os = "redox")))]
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
            NonZeroUsize::new(stat.st_size as usize).ok_or(SegmentOpenError::InvalidatedSegment)?
        };

        // map segment into our address space
        let data_ptr = Self::map(len, &fd).map_err(|e| SegmentOpenError::OsError(e as _))?;

        Ok(Self {
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
            #[cfg(any(bsd, target_os = "redox"))]
            let flags = flags | OFlag::O_EXLOCK | OFlag::O_NONBLOCK;
            let mode = Mode::S_IRUSR;
            tracing::trace!("shm_open(name={}, flag={:?}, mode={:?})", id, flags, mode);
            shm_open(id.as_str(), flags, mode)
        };

        #[cfg(any(bsd, target_os = "redox"))]
        // sussessful open means that we are the last owner of this file - unlink it
        if fd.is_ok() {
            let id = Self::id_str(id);
            tracing::trace!("shm_unlink(name={})", id);
            let _ = shm_unlink(id.as_str());
        }

        #[cfg(not(any(bsd, target_os = "redox")))]
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

    // we use flock() on non-BSD systems
    #[cfg(not(any(bsd, target_os = "redox")))]
    fn unlink_if_unique(id: ID, fd: &OwnedFd) {
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

        #[cfg(not(any(bsd, target_os = "redox")))]
        Self::unlink_if_unique(self.id, &self.fd);

        #[cfg(any(bsd, target_os = "redox"))]
        {
            // drop file descriptor to release O_SHLOCK we hold
            let fd = unsafe { ManuallyDrop::take(&mut self.fd) };
            drop(fd);

            // generate shm id string
            let id = Self::id_str(id);

            // try to open shm fd with O_EXLOCK
            let fd = {
                let flags = OFlag::O_RDWR | OFlag::O_EXLOCK | OFlag::O_NONBLOCK;
                let mode = Mode::S_IRUSR;
                tracing::trace!("shm_open(name={}, flag={:?}, mode={:?})", id, flags, mode);
                shm_open(id.as_str(), flags, mode)
            };

            // sussessful open means that we are the last owner of this file - unlink it
            if fd.is_ok() {
                tracing::trace!("shm_unlink(name={})", id);
                let _ = shm_unlink(id.as_str());
            }
        }
    }
}
