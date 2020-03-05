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
open Zenoh_types

include Zenoh_netutils

let store zenoh ?hlc selector (sublistener:Path.t -> change list -> unit Lwt.t) (query_handler:Selector.t -> (Path.t * TimedValue.t) list Lwt.t) =
  Logs.debug (fun m -> m "[Znu]: Declare storage on %s" (Selector.to_string selector));
  let zsublistener (resname:string) (samples:(Abuf.t * Ztypes.data_info) list) =
    let open Lwt.Infix in
    match Path.of_string_opt resname with
    | Some path -> decode_changes ?hlc samples >>= sublistener path
    | None -> Logs.warn (fun m -> m "[Znu]: Store received data for resource %s which is not a valid path" resname); Lwt.return_unit
  in
  let zquery_handler resname predicate =
    let s = if predicate = "" then resname else resname ^"?"^predicate in
    match Selector.of_string_opt s with
    | Some selector ->
      let%lwt kvs = query_handler selector in
      Lwt.return @@ List.map (fun ((path,tv):(Path.t*TimedValue.t)) ->
        let spath = Path.to_string path in
        let encoding = Some(encoding_to_flag tv.value) in
        let data_info = { Ztypes.empty_data_info with encoding; ts=Some(tv.time) } in
        let buf = encode_value tv.value in
        (spath, buf, data_info)) kvs
    | None -> Logs.warn (fun m -> m "[Znu]: Store received query for resource/predicate %s?%s which is not a valid selector" resname predicate); Lwt.return []

  in
  Zenoh_net.store zenoh (Selector.to_string selector) zsublistener zquery_handler

let unstore = Zenoh_net.unstore


