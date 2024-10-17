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

#[cfg(not(target_os = "linux"))]
mod platform {
    pub(crate) fn cleanup_orphaned_segments() {}
}

#[cfg(target_os = "linux")]
mod platform {
    use std::fs;

    use zenoh_result::ZResult;

    use crate::posix_shm::segment_lock::unix::ExclusiveShmLock;

    pub(crate) fn cleanup_orphaned_segments() {
        if let Err(e) = cleanup_orphaned_segments_inner() {
            tracing::error!("Error performing orphaned SHM segments cleanup: {e}")
        }
    }

    fn cleanup_orphaned_segments_inner() -> ZResult<()> {
        let shm_files = fs::read_dir("/dev/shm")?;

        for segment_file in shm_files.filter_map(Result::ok).filter(|f| {
            if let Some(ext) = f.path().extension() {
                return ext == "zenoh";
            }
            false
        }) {
            let os_id = segment_file.file_name();
            if let Ok(lock) = ExclusiveShmLock::open_exclusive(&os_id) {
                let _ = std::fs::remove_file(segment_file.path());
                drop(lock);
            }
        }

        Ok(())
    }
}
