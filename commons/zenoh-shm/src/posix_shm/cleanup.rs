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

pub(crate) use platform::cleanup_orphaned_segments;

#[cfg(not(unix))]
mod platform {
    pub(crate) fn cleanup_orphaned_segments() {}
}

#[cfg(unix)]
mod platform {
    use std::{borrow::Borrow, collections::HashSet, fs, path::PathBuf};

    use zenoh_result::ZResult;

    #[derive(PartialEq, Eq, Hash)]
    struct ProcFdDir(PathBuf);

    impl ProcFdDir {
        fn enumerate_fds(&self) -> ZResult<HashSet<FdFile>> {
            let fds = self.0.read_dir()?;
            let fd_map: HashSet<FdFile> = fds
                .filter_map(Result::ok)
                .map(|f| std::convert::Into::<FdFile>::into(f.path()))
                .collect();
            Ok(fd_map)
        }
    }

    impl From<PathBuf> for ProcFdDir {
        fn from(value: PathBuf) -> Self {
            Self(value)
        }
    }

    #[derive(PartialEq, Eq, Hash)]
    struct FdFile(PathBuf);

    impl From<PathBuf> for FdFile {
        fn from(value: PathBuf) -> Self {
            Self(value)
        }
    }

    #[derive(PartialEq, Eq, Hash)]
    struct ShmFile(PathBuf);

    impl ShmFile {
        fn cleanup_file(self) {
            let _ = std::fs::remove_file(self.0);
        }
    }

    impl Borrow<PathBuf> for ShmFile {
        fn borrow(&self) -> &PathBuf {
            &self.0
        }
    }

    impl From<PathBuf> for ShmFile {
        fn from(value: PathBuf) -> Self {
            Self(value)
        }
    }

    pub(crate) fn cleanup_orphaned_segments() {
        if let Err(e) = cleanup_orphaned_segments_inner() {
            tracing::error!("Error performing orphaned SHM segments cleanup: {e}")
        }
    }

    fn enumerate_shm_files() -> ZResult<HashSet<ShmFile>> {
        let shm_files = fs::read_dir("/dev/shm")?;
        Ok(shm_files
            .filter_map(Result::ok)
            .filter_map(|f| {
                if let Some(ext) = f.path().extension() {
                    if ext == "zenoh" {
                        return Some(std::convert::Into::<ShmFile>::into(f.path()));
                    }
                }
                None
            })
            .collect())
    }

    fn enumerate_proc_dirs() -> ZResult<HashSet<ProcFdDir>> {
        let proc_dirs = fs::read_dir("/proc")?;
        Ok(proc_dirs
            .filter_map(Result::ok)
            .map(|f| std::convert::Into::<ProcFdDir>::into(f.path().join("fd")))
            .collect())
    }

    fn enumerate_proc_fds() -> ZResult<HashSet<FdFile>> {
        let mut fds = HashSet::default();
        let dirs = enumerate_proc_dirs()?;
        for dir in dirs {
            if let Ok(dir_fds) = dir.enumerate_fds() {
                fds.extend(dir_fds);
            }
        }
        Ok(fds)
    }

    fn cleanup_orphaned_segments_inner() -> ZResult<()> {
        let fd_map = enumerate_proc_fds()?;
        let mut shm_map = enumerate_shm_files()?;

        for fd_file in fd_map {
            if let Ok(resolved_link) = fd_file.0.read_link() {
                shm_map.remove(&resolved_link);
            }
        }

        for shm_file_to_cleanup in shm_map {
            shm_file_to_cleanup.cleanup_file();
        }

        Ok(())
    }
}
