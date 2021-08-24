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
zconfigurable! {
    // Parameters of the link transmission queue
    // - The size of each queue relates to the number of batches a given queue can contain.
    // - The amount of memory being allocated for each queue is then QUEUE_SIZE_XXX * ZN_BATCH_SIZE.
    //   In case the transport link MTU is smaller than the ZN_BATCH_SIZE, then amount of memory being
    //   allocated for each queue is QUEUE_SIZE_XXX * LINK_MTU.
    pub static ref ZN_QUEUE_SIZE_CONTROL: usize = 1;
    pub static ref ZN_QUEUE_SIZE_REAL_TIME: usize = 4;
    pub static ref ZN_QUEUE_SIZE_INTERACTIVE_HIGH: usize = 4;
    pub static ref ZN_QUEUE_SIZE_INTERACTIVE_LOW: usize = 4;
    pub static ref ZN_QUEUE_SIZE_DATA_HIGH: usize = 4;
    pub static ref ZN_QUEUE_SIZE_DATA: usize = 4;
    pub static ref ZN_QUEUE_SIZE_DATA_LOW: usize = 4;
    pub static ref ZN_QUEUE_SIZE_BACKGROUND: usize = 4;

    // The default backoff time in nanoseconds to allow the batching to potentially progress
    pub static ref ZN_QUEUE_PULL_BACKOFF: u64 = 100;
}
