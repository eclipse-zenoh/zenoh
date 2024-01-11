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

use zenoh_result::ZResult;

pub const TEST_SEGMENT_PREFIX: &str = "test";

pub fn validate_memory(mem1: &mut [u8], mem2: &[u8]) {
    assert!(mem1.len() == mem2.len());
    for cycle in 0..255u8 {
        // sequentially fill segment1 with values checking segment2 having these changes
        for i in 0..mem1.len() {
            mem1[i] = cycle;
            assert!(mem2[i] == cycle);
        }

        // check the whole segment2 having proper values
        for i in mem2 {
            assert!(*i == cycle);
        }
    }
}

pub fn execute_concurrent<TaskFun>(concurrent_tasks: usize, iterations: usize, task_fun: TaskFun)
where
    TaskFun: Fn(usize, usize) -> ZResult<()> + Clone + Send + Sync + 'static,
{
    let mut tasks = vec![];
    for task_index in 0..concurrent_tasks {
        let c_task_fun = task_fun.clone();
        let task_handle = std::thread::spawn(move || {
            for iteration in 0..iterations {
                if let Err(e) = c_task_fun(task_index, iteration) {
                    panic!("task {task_index}: iteration {iteration}: {e}")
                }
            }
        });
        tasks.push(task_handle);
    }
    for task in tasks {
        task.join().expect("Error joining thread!");
    }
}
