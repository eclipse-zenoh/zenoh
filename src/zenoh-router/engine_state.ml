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
open Apero
open Apero_net
open NetService
open R_name
open Query

module SIDMap = Map.Make(NetService.Id)
module QIDMap = Map.Make(Qid)

type tx_session_connector = Locator.t -> TxSession.t Lwt.t 

type engine_state = {
    pid : Abuf.t;
    lease : Vle.t;
    locators : Locators.t;
    hlc : Ztypes.HLC.t;
    timestamp : bool;
    smap : Session.t SIDMap.t;
    rmap : Resource.t ResMap.t;
    qmap : Query.t QIDMap.t;
    peers : Locator.t list;
    users : (string * string) list option;
    trees : Spn_trees_mgr.t;
    next_mapping : Vle.t;
    tx_connector : tx_session_connector;
    buffer_pool : Abuf.t Lwt_pool.t;
    next_local_id : NetService.Id.t;
}

let report_resources e = 
    List.fold_left (fun s (_, r) -> s ^ Resource.report r ^ "\n") "" (ResMap.bindings e.rmap)