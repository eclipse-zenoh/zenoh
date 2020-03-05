(*
 * Copyright (c) 2017, 2020 ADLINK Technology Inc.
 *
 * This program and the accompanying materials are made available under the
 * terms of the Eclipse Public License 2.0 which is available at
 * http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
 * which is available at https://www.apache.org/licenses/LICENSE-2.0.
 *
 * SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
 *
 * Contributors:
 *   ADLINK zenoh team, <zenoh@adlink-labs.tech>
 *)
module HLC = Apero_time.HLC.Make (Apero_time.Clock_unix)
module Timestamp = HLC.Timestamp

type query_dest = 
| No
| Best_match
| Complete of int
| All

type replies_consolidation =
| KeepAll
| LatestValue

type data_info = {
  srcid:    Abuf.t option;
  srcsn:    int64 option;
  bkrid:    Abuf.t option;
  bkrsn:    int64 option;
  ts:       Timestamp.t option;
  kind:     int64 option;
  encoding: int64 option;
}

let empty_data_info = {srcid=None; srcsn=None; bkrid=None; bkrsn=None; ts=None; encoding=None; kind=None}