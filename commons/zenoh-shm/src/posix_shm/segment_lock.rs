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
#[cfg(unix)]
pub(crate) mod unix {
    use std::{
        ffi::OsStr,
        fs::{File, OpenOptions},
        path::PathBuf,
    };

    use advisory_lock::{AdvisoryFileLock, FileLockMode};
    use zenoh_result::ZResult;

    #[repr(transparent)]
    pub(crate) struct ShmLock(LockInner);

    impl Drop for ShmLock {
        fn drop(&mut self) {
            if self.0._tempfile.try_lock(FileLockMode::Exclusive).is_ok() {
                let _ = std::fs::remove_file(self.0.path.clone());
            }
        }
    }

    impl ShmLock {
        pub(crate) fn create<T>(os_id: &T) -> ZResult<Self>
        where
            T: ?Sized + AsRef<OsStr>,
        {
            // calculate tempfile path
            let path = tmp_file_path(os_id);

            // create tempfile just to lock it
            let tempfile = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path.clone())?;

            // lock tempfile with shared lock to indicate that file is managed
            tempfile.try_lock(FileLockMode::Shared)?;

            Ok(Self(LockInner {
                path,
                _tempfile: tempfile,
            }))
        }

        pub(crate) fn open<T>(os_id: &T) -> ZResult<Self>
        where
            T: ?Sized + AsRef<OsStr>,
        {
            // calculate tempfile path
            let path = tmp_file_path(os_id);

            // open tempfile just to lock it
            let tempfile = OpenOptions::new().read(true).open(path.clone())?;

            // lock tempfile with shared lock to indicate that file is managed
            tempfile.try_lock(FileLockMode::Shared)?;

            Ok(Self(LockInner {
                path,
                _tempfile: tempfile,
            }))
        }
    }

    #[repr(transparent)]
    pub(crate) struct ExclusiveShmLock(LockInner);

    impl ExclusiveShmLock {
        pub(crate) fn open_exclusive<T>(os_id: &T) -> ZResult<Self>
        where
            T: ?Sized + AsRef<OsStr>,
        {
            // calculate tempfile path
            let path = tmp_file_path(os_id);

            // create or open tempfile just to lock it
            let tempfile = OpenOptions::new()
                .write(true)
                .truncate(false)
                .create(true)
                .open(path.clone())?;

            // lock tempfile with exclusive lock to guarantee that the file is unmanaged
            tempfile.try_lock(FileLockMode::Exclusive)?;

            Ok(Self(LockInner {
                path,
                _tempfile: tempfile,
            }))
        }
    }

    impl Drop for ExclusiveShmLock {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(self.0.path.clone());
        }
    }

    impl TryFrom<ShmLock> for ExclusiveShmLock {
        type Error = ();

        fn try_from(value: ShmLock) -> Result<Self, Self::Error> {
            if value.0._tempfile.try_lock(FileLockMode::Exclusive).is_ok() {
                return Ok(unsafe { core::mem::transmute::<ShmLock, ExclusiveShmLock>(value) });
            }
            Err(())
        }
    }

    struct LockInner {
        path: PathBuf,
        _tempfile: File,
    }

    fn tmp_file_path<T>(os_id: &T) -> PathBuf
    where
        T: ?Sized + AsRef<OsStr>,
    {
        std::env::temp_dir().join(PathBuf::from(os_id))
    }
}
