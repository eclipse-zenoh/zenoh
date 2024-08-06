//
// Copyright (c) 2021 - 2024 ZettaScale Technology
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

use flume::{Receiver, Sender};
use zenoh::{key_expr::OwnedKeyExpr, sample::Sample, time::Timestamp};

pub struct ReplicationService {
    pub empty_start: bool,
    pub aligner_updates: Receiver<Sample>,
    pub log_propagation: Sender<(OwnedKeyExpr, Timestamp)>,
}
