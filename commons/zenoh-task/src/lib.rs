//
// Copyright (c) 2024 ZettaScale Technology
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

//! ⚠️ WARNING ⚠️
//!
//! This module is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](../zenoh/index.html)

use rand::random;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::future::Future;
use std::ops::DerefMut;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::task::{self, JoinHandle};
use zenoh_runtime::ZRuntime;

#[derive(Clone)]
pub struct TaskController {
    running_task_id_to_handle: Arc<Mutex<HashMap<u64, Option<JoinHandle<()>>>>>,
}

impl Default for TaskController {
    fn default() -> Self {
        TaskController {
            running_task_id_to_handle: Arc::new(Mutex::new(
                HashMap::<u64, Option<JoinHandle<()>>>::new(),
            )),
        }
    }
}

impl TaskController {
    /// Spawns a task (similarly to task::spawn) that can be later terminated by call to terminate_all()
    /// Task output is ignored
    pub fn spawn<F, T>(&self, future: F)
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let mut tasks = self.running_task_id_to_handle.lock().unwrap();
        let id = TaskController::get_next_task_id(tasks.deref_mut());
        let tasks_mutex = self.running_task_id_to_handle.clone();
        let jh = task::spawn(futures::FutureExt::map(future, move |_| {
            tasks_mutex.lock().unwrap().remove(&id);
        }));
        tasks.insert(id, Some(jh));
    }

    /// Spawns a task using a specified runtime (similarly to Runtime::spawn) that can be later terminated by call to terminate_all()
    /// Task output is ignored
    pub fn spawn_with_rt<F, T>(&self, rt: ZRuntime, future: F)
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let mut tasks = self.running_task_id_to_handle.lock().unwrap();
        let id = TaskController::get_next_task_id(tasks.deref_mut());
        let tasks_mutex = self.running_task_id_to_handle.clone();
        let jh = rt.spawn(futures::FutureExt::map(future, move |_| {
            tasks_mutex.lock().unwrap().remove(&id);
        }));
        tasks.insert(id, Some(jh));
    }

    fn get_next_task_id(hm: &mut HashMap<u64, Option<JoinHandle<()>>>) -> u64 {
        loop {
            let id = random::<u64>();
            match hm.entry(id) {
                Entry::Occupied(_) => {
                    continue;
                }
                Entry::Vacant(v) => {
                    v.insert(None);
                    return id;
                }
            }
        }
    }

    /// Terminates all prevously spawned tasks
    pub fn terminate_all(&self) {
        let mut tasks_lock = self.running_task_id_to_handle.lock().unwrap();

        let tasks: Vec<(u64, Option<JoinHandle<()>>)> = tasks_lock.drain().collect();
        for (_id, jh) in tasks {
            jh.unwrap().abort();
        }
    }
}
