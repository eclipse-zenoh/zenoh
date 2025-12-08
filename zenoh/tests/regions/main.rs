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

mod scenario1;

use std::time::Duration;

use zenoh::Session;
use zenoh_config::WhatAmI;
use zenoh_core::ztimeout;

const TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Default)]
pub struct Node {
    c: zenoh::Config,
}

impl Node {
    pub fn new(mode: WhatAmI, id: &str) -> Self {
        let mut c = zenoh::Config::default();
        c.insert_json5("mode", &format!("\"{mode}\"")).unwrap();
        c.insert_json5("id", &format!("\"{id}\"")).unwrap();
        c.insert_json5("scouting/multicast/enabled", "false")
            .unwrap();
        Node { c }
    }

    pub fn insert(mut self, key: &str, value: &str) -> Self {
        self.c.insert_json5(key, value).unwrap();
        self
    }

    pub fn listen(self, listen: &str) -> Self {
        self.insert("listen/endpoints", &format!("[\"{listen}\"]"))
    }

    pub fn connect(self, connect: &[&str]) -> Self {
        self.insert("connect/endpoints", &format!("{connect:?}"))
    }

    pub fn endpoints(self, listen: &str, connect: &[&str]) -> Self {
        self.listen(listen).connect(connect)
    }

    pub fn gateway(self, conf: &str) -> Self {
        self.insert("gateway", conf)
    }

    pub async fn open(self) -> Session {
        ztimeout!(zenoh::open(self.c)).unwrap()
    }
}

#[macro_export]
macro_rules! count {
    ($storage:expr  $(, [$k:literal = $v:expr])* $(, $p:literal)* $(,)?) => {
        $storage.all_events().filter(|e| {
            true
                $(&& e.parent().is_some_and(|p| p.value($k).is_some_and(|v|v.as_debug_str().is_some_and(|v| v == $v))))*
                && e.message().is_some_and(|m| {
                    true
                        $(&&  m.contains($p))*
                })
            })
            .count()
    }
}
