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
use serde::{Deserialize, Serialize};
use shared_memory::{Shmem, ShmemConf, ShmemError};
use std::cmp::Ordering;
use std::collections::binary_heap::BinaryHeap;
use std::collections::HashMap;
use std::mem::align_of;
use std::sync::atomic;
use std::sync::atomic::{AtomicPtr, AtomicUsize};
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::zerror;

const MIN_FREE_CHUNK_SIZE: usize = 1024;
const ACCOUNTED_OVERHEAD: usize = 4096;
const ZENOH_SHM_PREFIX: &str = "zenoh_shm_pid";

fn align_addr_at(addr: usize, align: usize) -> usize {
    match addr % align {
        0 => addr,
        r => addr + (align - r),
    }
}

#[derive(Eq, Copy, Clone, Debug)]
struct Chunk {
    base_addr: *mut u8,
    offset: usize,
    size: usize,
}

impl Ord for Chunk {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.size.cmp(&other.size)
    }
}

impl PartialOrd for Chunk {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Chunk {
    fn eq(&self, other: &Self) -> bool {
        self.size == other.size
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SharedMemoryBufInfo {
    pub offset: usize,
    pub length: usize,
    pub shm_manager: String,
    pub kind: u8,
}

impl SharedMemoryBufInfo {
    pub fn new(offset: usize, length: usize, manager: String, kind: u8) -> SharedMemoryBufInfo {
        SharedMemoryBufInfo {
            offset,
            length,
            shm_manager: manager,
            kind,
        }
    }
}

impl Clone for SharedMemoryBufInfo {
    fn clone(&self) -> SharedMemoryBufInfo {
        SharedMemoryBufInfo {
            shm_manager: self.shm_manager.clone(),
            kind: self.kind,
            offset: self.offset,
            length: self.length,
        }
    }
}

pub struct SharedMemoryBuf {
    pub(crate) rc_ptr: AtomicPtr<AtomicUsize>,
    pub(crate) buf: AtomicPtr<u8>,
    pub(crate) len: usize,
    pub(crate) info: SharedMemoryBufInfo,
}

impl std::fmt::Debug for SharedMemoryBuf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ptr = self.rc_ptr.load(atomic::Ordering::SeqCst);
        let rc = unsafe { (*ptr).load(atomic::Ordering::SeqCst) };
        f.debug_struct("SharedMemoryBuf")
            .field("rc", &rc)
            .field("buf", &self.buf)
            .field("len", &self.len)
            .field("info", &self.info)
            .finish()
    }
}
impl SharedMemoryBuf {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get_kind(&self) -> u8 {
        self.info.kind
    }

    pub fn set_kind(&mut self, v: u8) {
        self.info.kind = v
    }

    pub fn owner(&self) -> String {
        self.info.shm_manager.clone()
    }

    pub fn ref_count(&self) -> usize {
        let rc = self.rc_ptr.load(atomic::Ordering::SeqCst);
        unsafe { (*rc).load(atomic::Ordering::SeqCst) }
    }

    pub fn inc_ref_count(&self) {
        let rc = self.rc_ptr.load(atomic::Ordering::SeqCst);
        unsafe { (*rc).fetch_add(1, atomic::Ordering::SeqCst) };
    }

    pub fn dec_ref_count(&self) {
        let rc = self.rc_ptr.load(atomic::Ordering::SeqCst);
        unsafe { (*rc).fetch_sub(1, atomic::Ordering::SeqCst) };
    }

    pub fn as_slice(&self) -> &[u8] {
        log::debug!("SharedMemoryBuf::as_slice() == len = {:?}", self.len);
        let bp = self.buf.load(atomic::Ordering::SeqCst);
        unsafe { std::slice::from_raw_parts(bp, self.len) }
    }

    ///
    /// Get a mutable slice.
    ///
    /// # Safety
    /// This operation is marked unsafe since we cannot guarantee the single mutable reference
    /// across multiple processes. Thus if you use it, and you'll inevitable have to use it,
    /// you have to keep in mind that if you have multiple process retrieving a mutable slice
    /// you may get into concurrent writes. That said, if you have a serial pipeline and
    /// the buffer is flowing through the pipeline this will not create any issues.
    ///
    /// In short, whilst this operation is marked as unsafe, you are safe if you can
    /// guarantee that your in applications only one process at the time will actually write.
    pub unsafe fn as_mut_slice(&mut self) -> &mut [u8] {
        let bp = self.buf.load(atomic::Ordering::SeqCst);
        std::slice::from_raw_parts_mut(bp, self.len)
    }
}

impl Drop for SharedMemoryBuf {
    fn drop(&mut self) {
        self.dec_ref_count();
    }
}

impl Clone for SharedMemoryBuf {
    fn clone(&self) -> Self {
        self.inc_ref_count();
        let rc = self.rc_ptr.load(atomic::Ordering::SeqCst);
        let bp = self.buf.load(atomic::Ordering::SeqCst);
        SharedMemoryBuf {
            rc_ptr: AtomicPtr::new(rc),
            buf: AtomicPtr::new(bp),
            len: self.len,
            info: self.info.clone(),
        }
    }
}

pub struct SharedMemoryManager {
    segment_path: String,
    size: usize,
    available: usize,
    chunk_header_size: usize,
    own_segment: Shmem,
    segments: HashMap<String, Shmem>,
    free_list: BinaryHeap<Chunk>,
    busy_list: Vec<Chunk>,
    alignment: usize,
}

unsafe impl Send for SharedMemoryManager {}

impl SharedMemoryManager {
    /// Creates a new SharedMemoryManager managing allocations of a region of the
    /// given size.
    pub fn new(id: String, size: usize) -> Result<SharedMemoryManager, ShmemError> {
        let mut temp_dir = std::env::temp_dir();
        let file_name: String = format!("{}_{}", ZENOH_SHM_PREFIX, id);
        temp_dir.push(file_name);
        let path: String = temp_dir.to_str().unwrap().to_string();
        log::trace!("Creating file at: {}", path);
        let real_size = size + ACCOUNTED_OVERHEAD;
        let shmem = match ShmemConf::new()
            .size(real_size)
            .flink(path.clone())
            .create()
        {
            Ok(m) => m,
            Err(ShmemError::LinkExists) => {
                log::trace!("Shared Memory already exists, opening it");
                ShmemConf::new().flink(path.clone()).open()?
            }
            Err(e) => return Err(e),
        };
        let base_ptr = shmem.as_ptr();

        let mut free_list = BinaryHeap::new();
        let chunk = Chunk {
            base_addr: base_ptr as *mut u8,
            offset: 0,
            size: real_size,
        };
        free_list.push(chunk);
        let busy_list = std::vec::Vec::new();
        let shm = SharedMemoryManager {
            segment_path: path,
            size,
            available: real_size,
            chunk_header_size: std::mem::size_of::<AtomicUsize>(),
            own_segment: shmem,
            segments: HashMap::new(),
            free_list,
            busy_list,
            alignment: align_of::<AtomicUsize>(),
        };
        log::trace!(
            "Created SharedMemoryManager for {:?}",
            shm.own_segment.as_ptr()
        );
        Ok(shm)
    }

    fn free_chunk_to_shmbuf(&self, chunk: &Chunk) -> SharedMemoryBuf {
        let info = SharedMemoryBufInfo {
            offset: chunk.offset,
            length: chunk.size,
            shm_manager: self.segment_path.clone(),
            kind: 0,
        };
        let rc = chunk.base_addr as *mut AtomicUsize;
        unsafe { (*rc).store(1, atomic::Ordering::SeqCst) };
        let rc_ptr = AtomicPtr::<AtomicUsize>::new(rc);
        SharedMemoryBuf {
            rc_ptr,
            buf: AtomicPtr::<u8>::new(unsafe { chunk.base_addr.add(self.chunk_header_size) }),
            len: chunk.size - self.chunk_header_size,
            info,
        }
    }

    pub fn alloc(&mut self, len: usize) -> Option<SharedMemoryBuf> {
        log::debug!("SharedMemoryManager::alloc({})", len);
        // Always allocate a size that will keep the proper alignment requirements
        let required_len = align_addr_at(len + self.chunk_header_size, self.alignment);
        if self.available < required_len {
            self.garbage_collect();
        }
        if self.available >= required_len {
            // The strategy taken is the same for some Unix System V implementations -- as described in the
            // famous Bach's book --  in essence keep an ordered list of free slot and always look for the
            // biggest as that will give the biggest left-over.
            match self.free_list.pop() {
                Some(mut chunk) if chunk.size >= required_len => {
                    self.available -= required_len;
                    log::debug!("Allocator selected Chunk ({:?})", &chunk);
                    if chunk.size - required_len >= MIN_FREE_CHUNK_SIZE {
                        let free_chunk = Chunk {
                            base_addr: unsafe { chunk.base_addr.add(required_len) },
                            offset: chunk.offset + required_len,
                            size: chunk.size - required_len,
                        };
                        log::debug!("The allocation will leave a Free Chunk: {:?}", &free_chunk);
                        self.free_list.push(free_chunk);
                    }
                    chunk.size = required_len;
                    let shm_buf = self.free_chunk_to_shmbuf(&chunk);
                    log::debug!("The allocated Chunk is ({:?})", &chunk);
                    log::debug!("Allocated Shared Memory Buffer: {:?}", &shm_buf);
                    self.busy_list.push(chunk);
                    Some(shm_buf)
                }
                Some(c) => {
                    self.free_list.push(c);
                    log::debug!(
                        "SharedMemoryManager::alloc({}) cannot find any available chunk of the appropriate size.",
                        len
                    );
                    log::debug!("SharedMemoryManager::free_list = {:?}", self.free_list);
                    None
                }
                None => {
                    log::debug!(
                        "SharedMemoryManager::alloc({}) cannot find any available chunk",
                        len
                    );
                    log::debug!("SharedMemoryManager::free_list = {:?}", self.free_list);
                    None
                }
            }
        } else {
            log::warn!(
                "SharedMemoryManager does not sufficient free memory to allocate {} bytes, try de-fragmenting!",
                len
            );
            None
        }
    }

    pub fn try_make_buf(&mut self, info: SharedMemoryBufInfo) -> ZResult<SharedMemoryBuf> {
        // From info does not increment the reference count as it is assumed
        // that the sender of this buffer has incremented for us.
        match self.segments.get(&info.shm_manager) {
            Some(shm) => {
                let base_ptr = shm.as_ptr();
                let rc = unsafe { base_ptr.add(info.offset) as *mut AtomicUsize };
                let rc_ptr = AtomicPtr::<AtomicUsize>::new(rc);
                let buf = unsafe { base_ptr.add(info.offset + self.chunk_header_size) as *mut u8 };
                let shm_buf = SharedMemoryBuf {
                    rc_ptr,
                    buf: AtomicPtr::new(buf),
                    len: info.length - self.chunk_header_size,
                    info,
                };
                Ok(shm_buf)
            }
            None => match ShmemConf::new().flink(&info.shm_manager).open() {
                Ok(shm) => {
                    let base_ptr = shm.as_ptr() as *mut u8;
                    self.segments.insert(info.shm_manager.clone(), shm);
                    let rc = unsafe { base_ptr.add(info.offset) as *mut AtomicUsize };
                    let rc_ptr = AtomicPtr::<AtomicUsize>::new(rc);
                    let buf =
                        unsafe { base_ptr.add(info.offset + self.chunk_header_size) as *mut u8 };
                    let shm_buf = SharedMemoryBuf {
                        rc_ptr,
                        buf: AtomicPtr::new(buf),
                        len: info.length - self.chunk_header_size,
                        info,
                    };
                    Ok(shm_buf)
                }
                Err(e) => {
                    log::trace!(
                        "Unable to bind shared segment: {} -- {:?}",
                        info.shm_manager,
                        e
                    );
                    zerror!(ZErrorKind::SharedMemoryError {
                        descr: format!("Unable to open shared memory @ {}", info.shm_manager),
                    })
                }
            },
        }
    }

    fn is_free_chunk(chunk: &Chunk) -> bool {
        let rc_ptr = chunk.base_addr as *mut AtomicUsize;
        let rc = unsafe { (*rc_ptr).load(atomic::Ordering::SeqCst) };
        rc == 0
    }
    fn try_merge_adjacent_chunks(a: &Chunk, b: &Chunk) -> Option<Chunk> {
        let end_addr = unsafe { a.base_addr.add(a.size) };
        if end_addr == b.base_addr {
            Some(Chunk {
                base_addr: a.base_addr,
                size: a.size + b.size,
                offset: a.offset,
            })
        } else {
            None
        }
    }
    // Returns the amount of memory that it was able to de-fragment
    pub fn defragment(&mut self) -> usize {
        if self.free_list.len() > 1 {
            let mut fbs: Vec<Chunk> = self.free_list.drain().collect();
            fbs.sort_by(|x, y| x.offset.partial_cmp(&y.offset).unwrap());
            let mut current = fbs.remove(0);
            let mut defrag_mem = 0;
            let mut i = 0;
            let n = fbs.len();
            for chunk in fbs.iter() {
                i += 1;
                let next = *chunk;
                match SharedMemoryManager::try_merge_adjacent_chunks(&current, &next) {
                    Some(c) => {
                        current = c;
                        defrag_mem += current.size;
                        if i == n {
                            self.free_list.push(current)
                        }
                    }
                    None => {
                        self.free_list.push(current);
                        if i == n {
                            self.free_list.push(next);
                        } else {
                            current = next;
                        }
                    }
                }
            }
            defrag_mem
        } else {
            0
        }
    }

    /// Returns the amount of memory freed
    pub fn garbage_collect(&mut self) -> usize {
        log::debug!("Running Garbage Collector");

        let mut freed = 0;
        let (free, busy) = self
            .busy_list
            .iter()
            .partition(|&c| SharedMemoryManager::is_free_chunk(c));
        self.busy_list = busy;

        for f in free {
            freed += f.size;
            log::debug!("Garbage Collecting Chunk: {:?}", f);
            self.free_list.push(f)
        }
        self.available += freed;
        freed
    }
}

impl std::fmt::Debug for SharedMemoryManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _ = f
            .debug_struct("SharedMemoryManager")
            .field("segment_path", &self.segment_path)
            .field("size", &self.size)
            .field("available", &self.available)
            .field("free_list.len", &self.free_list.len())
            .field("busy_list.len", &self.busy_list.len())
            .finish();
        f.debug_list()
            .entries(self.segments.keys().into_iter())
            .finish()
    }
}
