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
    ffi::c_void, num::NonZeroUsize, os::fd::{AsRawFd, OwnedFd}, ptr::NonNull
};

use nix::{
    fcntl::OFlag,
    sys::{
        mman::{mmap, munmap, shm_open, shm_unlink, MapFlags, ProtFlags},
        stat::{fstat, Mode},
    },
    unistd::ftruncate,
};

use super::{SegmentError, ShmResult};

pub(crate) struct SegmentImpl {
    fd: OwnedFd,
    len: NonZeroUsize,
    data_ptr: NonNull<c_void>,
}

impl SegmentImpl {
    pub fn create(id: &str, len: NonZeroUsize) -> ShmResult<Self> {
        // create unique shm fd
        let fd = {
            let flags = OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_RDWR;
            let mode = Mode::S_IRUSR | Mode::S_IWUSR;
            tracing::trace!("shm_open(name={}, flag={:?}, mode={:?})", id, flags, mode);
            match shm_open(id, flags, mode) {
                Ok(v) => v,
                Err(nix::Error::EEXIST) => return Err(SegmentError::SegmentExists),
                Err(e) => return Err(SegmentError::OsError(e as u32)),
            }
        };

        // resize shm segment to requested size
        tracing::trace!("ftruncate(fd={}, len={})", fd.as_raw_fd(), len);
        if let Err(e) = ftruncate(fd, len.get() as _) {
            return Err(SegmentError::OsError(e as u32));
        };

        //map segment into our address space
        let data_ptr = {
            let prot = ProtFlags::PROT_READ | ProtFlags::PROT_WRITE;
            let flags = MapFlags::MAP_SHARED;

            tracing::trace!(
                "mmap(addr=NULL, length={}, prot={:X}, flags={:X}, f={}, offset=0)",
                len,
                prot,
                flags,
                fd.as_raw_fd()
            );

            match unsafe { mmap(None, len, prot, flags, fd, 0) } {
                Ok(v) => v,
                Err(e) => return Err(SegmentError::OsError(e as u32)),
            }
        };

        Ok(Self { fd, len, data_ptr })
    }

    pub fn open(id: &str) -> ShmResult<Self> {
        // open shm fd
        let fd = {
            let flags = OFlag::O_RDWR;
            let mode = Mode::S_IRUSR;
            tracing::trace!("shm_open(name={}, flag={:?}, mode={:?})", id, flags, mode);
            match shm_open(id, flags, mode) {
                Ok(v) => v,
                Err(e) => return Err(SegmentError::OsError(e as u32)),
            }
        };

        // get segment size
        let len = match fstat(fd.as_raw_fd()) {
            Ok(v) => v.st_size as usize,
            Err(e) => return Err(SegmentError::OsError(e as u32)),
        };
        let len = NonZeroUsize::new(len).ok_or(SegmentError::ZeroSizedSegment)?;

        // map segment into our address space
        let data_ptr = {
            let prot = ProtFlags::PROT_READ | ProtFlags::PROT_WRITE;
            let flags = MapFlags::MAP_SHARED;

            tracing::trace!(
                "mmap(addr=NULL, length={}, prot={:X}, flags={:X}, f={}, offset=0)",
                len,
                prot,
                flags,
                fd.as_raw_fd()
            );

            match unsafe { mmap(None, len, prot, flags, fd, 0) } {
                Ok(v) => v,
                Err(e) => return Err(SegmentError::OsError(e as u32)),
            }
        };

        Ok(Self { fd, len, data_ptr })
    }

    fn map(id: &str) -> ShmResult<Self> {
        // map segment into our address space
        let data_ptr = {
            let prot = ProtFlags::PROT_READ | ProtFlags::PROT_WRITE;
            let flags = MapFlags::MAP_SHARED;

            tracing::trace!(
                "mmap(addr=NULL, length={}, prot={:X}, flags={:X}, f={}, offset=0)",
                len,
                prot,
                flags,
                fd.as_raw_fd()
            );

            match unsafe { mmap(None, len, prot, flags, fd, 0) } {
                Ok(v) => v,
                Err(e) => return Err(SegmentError::OsError(e as u32)),
            }
        };

        Ok(Self { fd, len, data_ptr })
    }
}

impl Drop for SegmentImpl {
    ///Takes care of properly closing the SharedMem (munmap(), shmem_unlink(), close())
    fn drop(&mut self) {
        //Unmap memory
        tracing::trace!(
            "munmap(addr:{:p},len:{})",
            self.map_ptr,
            self.map_size
        );
        if let Err(_e) = unsafe { munmap(self.map_ptr as *mut _, self.map_size) } {
            debug!("Failed to munmap() shared memory mapping : {}", _e);
        };

        //Unlink shmem
        if self.map_fd != 0 {
            //unlink shmem if we created it
            if self.owner {
                debug!("Deleting persistent mapping");
                trace!("shm_unlink({})", self.unique_id.as_str());
                if let Err(_e) = shm_unlink(self.unique_id.as_str()) {
                    debug!("Failed to shm_unlink() shared memory : {}", _e);
                };
            }

            trace!("close({})", self.map_fd);
            if let Err(_e) = close(self.map_fd) {
                debug!(
                    "os_impl::Linux : Failed to close() shared memory file descriptor : {}",
                    _e
                );
            };
        }
    }
}