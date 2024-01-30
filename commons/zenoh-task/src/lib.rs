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

//! ⚠️ WARNING ⚠️
//!
//! This module is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](../zenoh/index.html)

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::future::Future;
use std::ops::DerefMut;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::task::{self, JoinHandle};
use uuid::Uuid;

#[derive(Clone)]
pub struct TaskController {
    running_task_id_to_handle: Arc<Mutex<HashMap<Uuid, Option<JoinHandle<()>>>>>,
}

impl TaskController {
    pub fn new() -> TaskController {
        TaskController {
            running_task_id_to_handle: Arc::new(Mutex::new(
                HashMap::<Uuid, Option<JoinHandle<()>>>::new(),
            )),
        }
    }

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
            ()
        }));
        tasks.insert(id, Some(jh));
    }

    fn get_next_task_id(hm: &mut HashMap<Uuid, Option<JoinHandle<()>>>) -> Uuid {
        loop {
            let uuid = Uuid::new_v4();
            match hm.entry(uuid.clone()) {
                Entry::Occupied(_) => {
                    continue;
                }
                Entry::Vacant(v) => {
                    v.insert(None);
                    return uuid;
                }
            }
        }
    }

    /// Terminates all prevously spawned tasks
    pub fn terminate_all(&self) {
        let tasks: Vec<(Uuid, Option<JoinHandle<()>>)> = self
            .running_task_id_to_handle
            .lock()
            .unwrap()
            .drain()
            .collect();
        for (_id, jh) in tasks {
            let _ = jh.unwrap().abort();
        }
    }
}
