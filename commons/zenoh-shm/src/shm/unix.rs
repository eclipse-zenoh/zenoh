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
use std::os::fd::FromRawFd;
use std::{
    ffi::c_void,
    num::NonZeroUsize,
    os::fd::{AsRawFd, OwnedFd},
    ptr::NonNull,
};

use advisory_lock::{AdvisoryFileLock, FileLockMode};
#[cfg(any(bsd, target_os = "redox"))]
use nix::fcntl::open;
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
    lock_fd: OwnedFd,
    len: NonZeroUsize,
    data_ptr: NonNull<c_void>,
    id: ID,
}

// PUBLIC
impl<ID: SegmentID> SegmentImpl<ID> {
    pub fn create(id: ID, len: NonZeroUsize) -> ShmCreateResult<Self> {
        // we use separate lockfile on non-tmpfs for bsd
        #[cfg(any(bsd, target_os = "redox"))]
        let lock_fd = unsafe {
            OwnedFd::from_raw_fd({
                let lockpath = std::env::temp_dir().join(Self::id_str(id));
                let flags = OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_RDWR;
                let mode = Mode::S_IRUSR | Mode::S_IWUSR;
                open(&lockpath, flags, mode).map_err(|e| match e {
                    nix::Error::EEXIST => SegmentCreateError::SegmentExists,
                    e => SegmentCreateError::OsError(e as u32),
                })
            }?)
        };

        // create unique shm fd
        let fd = {
            let id = Self::id_str(id);
            let flags = OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_RDWR;

            // todo: these flags probably can be exposed to the config
            let mode = Mode::S_IRUSR | Mode::S_IWUSR;

            tracing::trace!("shm_open(name={}, flag={:?}, mode={:?})", id, flags, mode);
            match shm_open(id.as_str(), flags, mode) {
                Ok(v) => v,
                Err(nix::Error::EEXIST) => return Err(SegmentCreateError::SegmentExists),
                Err(e) => return Err(SegmentCreateError::OsError(e as u32)),
            }
        };

        // on non-bsd we use our SHM file also for locking
        #[cfg(not(any(bsd, target_os = "redox")))]
        let lock_fd = fd;
        #[cfg(not(any(bsd, target_os = "redox")))]
        let fd = &lock_fd;

        #[cfg(any(bsd, target_os = "redox"))]
        let fd = &fd;

        // put shared advisory lock on lock_fd
        lock_fd
            .as_raw_fd()
            .try_lock(FileLockMode::Shared)
            .map_err(|e| match e {
                advisory_lock::FileLockError::AlreadyLocked => SegmentCreateError::SegmentExists,
                advisory_lock::FileLockError::Io(e) => {
                    SegmentCreateError::OsError(e.raw_os_error().unwrap_or(0) as _)
                }
            })?;

        // resize shm segment to requested size
        tracing::trace!("ftruncate(fd={}, len={})", fd.as_raw_fd(), len);
        ftruncate(fd, len.get() as _).map_err(|e| SegmentCreateError::OsError(e as u32))?;

        // get real segment size
        let len = {
            let stat = fstat(fd.as_raw_fd()).map_err(|e| SegmentCreateError::OsError(e as u32))?;
            NonZeroUsize::new(stat.st_size as usize).ok_or(SegmentCreateError::OsError(0))?
        };

        // map segment into our address space
        let data_ptr = Self::map(len, fd).map_err(|e| SegmentCreateError::OsError(e as _))?;

        Ok(Self {
            lock_fd,
            len,
            data_ptr,
            id,
        })
    }

    pub fn open(id: ID) -> ShmOpenResult<Self> {
        // we use separate lockfile on non-tmpfs for bsd
        #[cfg(any(bsd, target_os = "redox"))]
        let lock_fd = unsafe {
            OwnedFd::from_raw_fd({
                let lockpath = std::env::temp_dir().join(Self::id_str(id));
                let flags = OFlag::O_RDWR;
                let mode = Mode::S_IRUSR | Mode::S_IWUSR;
                open(&lockpath, flags, mode).map_err(|e| SegmentOpenError::OsError(e as _))
            }?)
        };

        // open shm fd
        let fd = {
            let id = Self::id_str(id);
            let flags = OFlag::O_RDWR;

            // todo: these flags probably can be exposed to the config
            let mode = Mode::S_IRUSR | Mode::S_IWUSR;

            tracing::trace!("shm_open(name={}, flag={:?}, mode={:?})", id, flags, mode);
            match shm_open(id.as_str(), flags, mode) {
                Ok(v) => v,
                Err(e) => return Err(SegmentOpenError::OsError(e as u32)),
            }
        };

        // on non-bsd we use our SHM file also for locking
        #[cfg(not(any(bsd, target_os = "redox")))]
        let lock_fd = fd;
        #[cfg(not(any(bsd, target_os = "redox")))]
        let fd = &lock_fd;

        #[cfg(any(bsd, target_os = "redox"))]
        let fd = &fd;

        // put shared advisory lock on lock_fd
        lock_fd
            .as_raw_fd()
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
        let data_ptr = Self::map(len, fd).map_err(|e| SegmentOpenError::OsError(e as _))?;

        Ok(Self {
            lock_fd,
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
        self.data_ptr.as_ptr() as _
    }
}

// PRIVATE
impl<ID: SegmentID> SegmentImpl<ID> {
    fn id_str(id: ID) -> String {
        format!("{id}.zenoh")
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

impl<ID: SegmentID> Drop for SegmentImpl<ID> {
    fn drop(&mut self) {
        tracing::trace!("munmap(addr={:p},len={})", self.data_ptr, self.len);
        if let Err(e) = unsafe { munmap(self.data_ptr, self.len.get()) } {
            tracing::debug!("munmap() failed : {}", e);
        };

        if self
            .lock_fd
            .as_raw_fd()
            .try_lock(FileLockMode::Exclusive)
            .is_ok()
        {
            let id = Self::id_str(self.id);
            tracing::trace!("shm_unlink(name={})", id);
            let _ = shm_unlink(id.as_str());
            #[cfg(any(bsd, target_os = "redox"))]
            {
                let lockpath = std::env::temp_dir().join(id);
                let _ = std::fs::remove_file(lockpath);
            }
        }
    }
}
