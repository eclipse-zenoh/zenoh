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

use zenoh_core::Wait;
use zenoh_shm::api::{
    client::shm_client::ShmClient,
    protocol_implementations::posix::{
        posix_shm_client::PosixShmClient,
        posix_shm_provider_backend_binary_heap::PosixShmProviderBackendBinaryHeap,
        posix_shm_provider_backend_buddy::PosixShmProviderBackendBuddy,
        posix_shm_provider_backend_talc::{PosixShmProviderBackendTalc, ShmPoolConfig},
    },
    provider::{
        memory_layout::MemoryLayout, shm_provider_backend::ShmProviderBackend,
        types::AllocAlignment,
    },
};

static BUFFER_NUM: usize = 100;
static BUFFER_SIZE: usize = 1000;

#[test]
fn posix_shm_provider_create() {
    let size = 1024;
    let backend = PosixShmProviderBackendBinaryHeap::builder(size)
        .wait()
        .expect("Error creating PosixShmProviderBackend!");
    assert!(backend.available() >= size);
}

#[test]
fn posix_shm_provider_alloc() {
    let backend = PosixShmProviderBackendBinaryHeap::builder(1024)
        .wait()
        .expect("Error creating PosixShmProviderBackend!");

    let layout = MemoryLayout::new(100, AllocAlignment::default()).unwrap();

    let _buf = backend
        .alloc(&layout)
        .expect("PosixShmProviderBackend: error allocating buffer");
}

#[test]
fn posix_shm_provider_open() {
    let backend = PosixShmProviderBackendBinaryHeap::builder(1024)
        .wait()
        .expect("Error creating PosixShmProviderBackend!");

    let layout = MemoryLayout::new(100, AllocAlignment::default()).unwrap();

    let buf = backend
        .alloc(&layout)
        .expect("PosixShmProviderBackend: error allocating buffer");

    let client = PosixShmClient {};

    let _segment = client
        .attach(buf.descriptor.segment)
        .expect("Error attaching to segment");
}

#[test]
fn posix_shm_provider_binary_heap_allocator() {
    // size to allocate in the provider
    let size_to_alloc = BUFFER_SIZE * BUFFER_NUM;

    let backend = PosixShmProviderBackendBinaryHeap::builder(size_to_alloc)
        .wait()
        .expect("Error creating PosixShmProviderBackend!");

    // the real size of memory available in the provider
    let real_size = backend.available();
    assert!(real_size >= size_to_alloc);

    // the real number of buffers allocatable in the provider
    let real_num = real_size / BUFFER_SIZE;
    assert!(real_num >= BUFFER_NUM);

    // the remainder in the provider
    let remainder = real_size - real_num * BUFFER_SIZE;
    assert!(remainder < BUFFER_SIZE);

    let layout = MemoryLayout::new(BUFFER_SIZE, AllocAlignment::default()).unwrap();

    // exhaust memory by allocating it all
    let mut buffers = vec![];
    for _ in 0..real_num {
        let buf = backend
            .alloc(&layout)
            .expect("PosixShmProviderBackend: error allocating buffer");
        buffers.push(buf);
    }

    for _ in 0..real_num {
        // there is nothing to allocate at this point
        assert_eq!(backend.available(), remainder);
        assert!(backend.alloc(&layout).is_err());

        // free buffer
        let to_free = buffers.pop().unwrap().descriptor;
        backend.free(&to_free);

        // allocate new one
        let buf = backend
            .alloc(&layout)
            .expect("PosixShmProviderBackend: error allocating buffer");
        buffers.push(buf);
    }

    // free buffers
    while let Some(buffer) = buffers.pop() {
        backend.free(&buffer.descriptor);
    }

    // confirm that allocator is free
    assert_eq!(backend.available(), real_size);
}

#[test]
fn posix_shm_provider_buddy_allocator() {
    // size to allocate in the provider
    let size_to_alloc = BUFFER_SIZE * BUFFER_NUM;

    let backend = PosixShmProviderBackendBuddy::builder(size_to_alloc)
        .wait()
        .expect("Error creating PosixShmProviderBackend!");

    let layout = MemoryLayout::new(BUFFER_SIZE, AllocAlignment::default()).unwrap();

    // exhaust memory by allocating it all
    let mut buffers = vec![];
    while let Ok(buf) = backend.alloc(&layout) {
        buffers.push(buf);
    }

    for _ in 0..100 {
        // there is nothing to allocate at this point
        assert!(backend.alloc(&layout).is_err());

        // free buffer
        let to_free = buffers.pop().unwrap().descriptor;
        backend.free(&to_free);

        // allocate new one
        let buf = backend
            .alloc(&layout)
            .expect("PosixShmProviderBackend: error allocating buffer");
        buffers.push(buf);
    }

    // free buffers
    while let Some(buffer) = buffers.pop() {
        backend.free(&buffer.descriptor);
    }
}

#[test]
fn posix_shm_provider_talc_allocator() {
    // size to allocate in the provider
    let size_to_alloc = BUFFER_SIZE * BUFFER_NUM;

    let backend = PosixShmProviderBackendTalc::builder(size_to_alloc)
        .wait()
        .expect("Error creating PosixShmProviderBackend!");

    let layout = MemoryLayout::new(BUFFER_SIZE, AllocAlignment::default()).unwrap();

    // exhaust memory by allocating it all
    let mut buffers = vec![];
    while let Ok(buf) = backend.alloc(&layout) {
        buffers.push(buf);
    }

    for _ in 0..100 {
        // there is nothing to allocate at this point
        assert!(backend.alloc(&layout).is_err());

        // free buffer
        let to_free = buffers.pop().unwrap().descriptor;
        backend.free(&to_free);

        // allocate new one
        let buf = backend
            .alloc(&layout)
            .expect("PosixShmProviderBackend: error allocating buffer");
        buffers.push(buf);
    }

    // free buffers
    while let Some(buffer) = buffers.pop() {
        backend.free(&buffer.descriptor);
    }
}

// ── Pool tests ─────────────────────────────────────────────────────────────

/// Allocate a buffer, drop it so the segment is returned to the pool, then allocate
/// again with the same size. The second allocation must reuse the same SHM segment
/// (same SegmentID) — no new shm_open.
#[test]
fn shm_segment_pool_reuse() {
    let layout = MemoryLayout::new(4096_usize, AllocAlignment::default()).unwrap();

    let backend = PosixShmProviderBackendTalc::builder_with_pool(
        layout.size(),
        ShmPoolConfig {
            max_idle_per_size: 1,
        },
    )
    .wait()
    .expect("Error creating pooled PosixShmProviderBackendTalc");

    // First allocation — creates a new segment.
    let chunk1 = backend.alloc(&layout).expect("first alloc failed");
    let seg_id_first = chunk1.descriptor.segment;

    // Drop the chunk — PooledPosixShmSegment::drop should return it to the pool.
    drop(chunk1);

    // Second allocation of the same size — must reuse the pooled segment.
    let chunk2 = backend.alloc(&layout).expect("second alloc failed");
    let seg_id_second = chunk2.descriptor.segment;

    assert_eq!(
        seg_id_first, seg_id_second,
        "Expected pool to reuse the segment (same SegmentID), but got a new one"
    );
}

/// With max_idle=1, returning two segments of the same size should drop the second
/// one immediately (munmap), keeping only one in the pool.
#[test]
fn shm_segment_pool_cap_enforcement() {
    let layout = MemoryLayout::new(4096_usize, AllocAlignment::default()).unwrap();

    let backend = PosixShmProviderBackendTalc::builder_with_pool(
        layout.size(),
        ShmPoolConfig {
            max_idle_per_size: 1,
        },
    )
    .wait()
    .expect("Error creating pooled PosixShmProviderBackendTalc");

    // Allocate two chunks (two distinct segments).
    let chunk_a = backend.alloc(&layout).expect("alloc a failed");
    let chunk_b = backend.alloc(&layout).expect("alloc b failed");
    let seg_a = chunk_a.descriptor.segment;
    let seg_b = chunk_b.descriptor.segment;

    // They should be distinct segments.
    assert_ne!(
        seg_a, seg_b,
        "Expected two distinct segments for two concurrent allocs"
    );

    // Drop both — pool can hold at most 1, so one is immediately unmapped.
    drop(chunk_a);
    drop(chunk_b);

    // The next allocation should return one of the two segments (whichever was retained).
    let chunk_c = backend
        .alloc(&layout)
        .expect("alloc c after cap test failed");
    let seg_c = chunk_c.descriptor.segment;

    // seg_c must be one of the two previously-seen IDs (pooled), not a fresh one.
    assert!(
        seg_c == seg_a || seg_c == seg_b,
        "Expected reuse of a pooled segment, but got a fresh segment id {seg_c}"
    );
}

/// Drop the provider (which holds the Arc<ShmSegmentPool>), then drop a chunk that
/// still holds a Weak<ShmSegmentPool>. The Weak upgrade must fail gracefully — no
/// panic, no use-after-free.
#[test]
fn shm_segment_pool_outlived_by_segment() {
    let layout = MemoryLayout::new(4096_usize, AllocAlignment::default()).unwrap();

    let chunk = {
        let backend =
            PosixShmProviderBackendTalc::builder_with_pool(layout.size(), ShmPoolConfig::default())
                .wait()
                .expect("Error creating pooled PosixShmProviderBackendTalc");

        let chunk = backend.alloc(&layout).expect("alloc failed");
        // backend (and its Arc<ShmSegmentPool>) drops here
        chunk
    };

    // chunk still holds a PtrInSegment → Arc<PooledPosixShmSegment>.
    // Dropping it now: Weak::upgrade fails → segment drops normally (munmap).
    // This must not panic.
    drop(chunk);
}
