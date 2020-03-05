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
open Zenoh_common_errors
open Be_sql_types

let to_sql_string s = "'"^s^"'"


let get_kv_condition sub_selector = 
  let sql_selector = Selector.path sub_selector |> Astring.replace '*' '%'
  in
  match Selector.predicate sub_selector with
  | Some q -> "k like '"^sql_selector^"' AND "^q
  | None   -> "k like '"^sql_selector^"'"


let timed_value_of_strings storage_info k v e t : TimedValue.t option =
  match Value.string_to_encoding e with
  | Some encoding ->
    (match Value.of_string v encoding with 
    | Ok value ->
      (match Timestamp.of_string t with
      | Some time -> Some {time; value}
      | None -> Logs.warn (fun m -> m "[SQL]: In KV table %s for key %s: failed to read timestamp: '%s'" storage_info.table_name k t); None
      )
    | Error err ->
      Logs.warn (fun m -> m "[SQL]: In KV table %s for key %s: failed to decode value '%s' with encoding '%s': %s"
            storage_info.table_name k v e (show_yerror err));
      None)
  | None -> Logs.warn (fun m -> m "[SQL]: In KV table %s for key %s: unkown encoding: '%s'" storage_info.table_name k e); None


let get storage_info (selector:Selector.t) =
  Logs.debug (fun m -> m "[SQL]: get(%s) from kv table %s"
        (Selector.to_string selector) (storage_info.table_name));
  let open LwtM.InfixM in
  let (_, typ) = storage_info.schema in
  match Selector.remaining_after_match storage_info.keys_prefix selector with
  | None -> Lwt.return []
  | Some sub_sel ->
    let condition = get_kv_condition sub_sel in
    Caqti_driver.get storage_info.connection storage_info.table_name typ ~condition ()
    >|= List.map (fun row -> match row with
      | k::v::e::t::[] ->
        (* NOTE: replacing '*' with '%' in LIKE clause gives more keys than we want (as % is similar to %%). 
           We need to check matching with sub-selector, and to discard those that don't match *)
        if Selector.is_matching_path (Path.of_string k) sub_sel then
          (match timed_value_of_strings storage_info k v e t with
          | Some tv -> Some (Path.of_string @@ (Path.to_string storage_info.keys_prefix)^k, tv)
          | None -> None)
        else
          None
      | _ -> Logs.warn (fun m -> m "[SQL]: get in KV table %s returned non k+v+e+t value: %s" storage_info.table_name (String.concat "," row));
             None
      )
    >|= List.filter Option.is_some
    >|= List.map (fun o -> Option.get o)


let sql_key_from_path storage_info path =
  Astring.with_range ~first:storage_info.keys_prefix_length (Path.to_string path) |> to_sql_string

let sql_values_from_timed_value (tv:TimedValue.t) = 
  let v = Value.to_string tv.value |> to_sql_string in
  let e = Value.encoding_to_string @@ Value.encoding tv.value |> to_sql_string in
  let t = Timestamp.to_string tv.time |> to_sql_string in
  v::e::t::[]

type key_status = | Unkown | Removed of Timestamp.t * unit Lwt.t | Present_time of Timestamp.t | Present_value of TimedValue.t


let get_key_status ?(with_value=false) storage_info key =
  let removals_map = Guard.get storage_info.removals in
  match RemovalMap.find_opt key removals_map with
  | Some (t, cleanup) -> Lwt.return @@ Removed (t, cleanup)
  | None ->
    let condition = "k="^key in
    let columns, typ = if with_value then "v,e,t", Caqti_driver.Dyntype.(add string (add string string)) else "t", Caqti_driver.Dyntype.string in
    match%lwt Caqti_driver.get storage_info.connection storage_info.table_name ~columns typ ~condition () with
    | [] -> Lwt.return Unkown
    | (t::[])::_ -> (match Timestamp.of_string t with
      | Some time -> Lwt.return @@ Present_time time
      | None -> Logs.warn (fun m -> m "[SQL]: In KV table %s for key %s: failed to read timestamp: '%s'" storage_info.table_name key t);
                Lwt.return Unkown)
    | (v::e::t::[])::_ -> (match timed_value_of_strings storage_info key v e t with
      | Some tv -> Lwt.return @@ Present_value tv
      | None -> Lwt.return Unkown)
    | x::_ -> Logs.warn (fun m -> m "[SQL]: In KV table %s for key %s: returned non k+v+e+t value: %s" storage_info.table_name key (String.concat "," x));
              Lwt.return Unkown


let do_sql_put storage_info key tv =
  Caqti_driver.put storage_info.connection storage_info.table_name storage_info.schema (key::(sql_values_from_timed_value tv)) ()

let put storage_info (path:Path.t) (tv:TimedValue.t) =
  Logs.debug (fun m -> m "[SQL]: put(%s) into kv table %s" (Path.to_string path) (storage_info.table_name));
  let key = sql_key_from_path storage_info path in
  match%lwt get_key_status storage_info key with
  | Unkown -> 
    do_sql_put storage_info key tv
  | Present_time t | Present_value {time=t; value=_} ->
    if Timestamp.Infix.(t < tv.time) then
      do_sql_put storage_info key tv
    else (
      Logs.debug (fun m -> m "[SQL] put on %s dropped: out-of-date" (Path.to_string path));
      Lwt.return_unit)
  | Removed (t, cleanup) ->
    if Timestamp.Infix.(t < tv.time) then
      (Lwt.cancel cleanup; do_sql_put storage_info key tv)
    else (
      Logs.debug (fun m -> m "[SQL] put on %s dropped: out-of-date" (Path.to_string path));
      Lwt.return_unit)

let update storage_info path (delta:TimedValue.t) =
  Logs.debug (fun m -> m "[SQL]: update(%s) into kv table %s" (Path.to_string path) (storage_info.table_name));
  let key = sql_key_from_path storage_info path in
  match%lwt get_key_status ~with_value:true storage_info key with
  | Unkown -> do_sql_put storage_info key delta
  | Present_value tv ->
    if TimedValue.preceeds ~first:tv ~second:delta
    then 
      (match TimedValue.update tv ~delta with
      | Ok tv' -> do_sql_put storage_info key tv'
      | Error e -> Logs.warn (fun m -> m "[SQL] update on %s failed: %s" (Path.to_string path) (show_yerror e));
        Lwt.return_unit
      )
    else (
      Logs.debug (fun m -> m "[SQL] update on %s dropped: out-of-date" (Path.to_string path));
      Lwt.return_unit)
  | Removed (t, cleanup) ->
    if Timestamp.Infix.(t < delta.time) then
      (Lwt.cancel cleanup; do_sql_put storage_info key delta)
    else (
      Logs.debug (fun m -> m "[SQL] update on %s dropped: out-of-date" (Path.to_string path));
      Lwt.return_unit)
  | Present_time _ -> 
    Logs.warn (fun m -> m "[SQL] update on %s failed: internal error !! Present_value was expected instead of Present_time" (Path.to_string path));
    Lwt.return_unit


let remove storage_info path time =
  let open Lwt.Infix in
  Logs.debug (fun m -> m "[SQL]: remove(%s) from kv table %s" (Path.to_string path) (storage_info.table_name));
  let key = sql_key_from_path storage_info path in
  match%lwt get_key_status storage_info key with
  | Removed _ ->
    (* do nothing *)
    Lwt.return_unit
  | Present_time t | Present_value {time=t; value=_} ->
    if Timestamp.Infix.(t < time) then
      Guard.guarded storage_info.removals
      @@ fun removals ->
        Caqti_driver.remove storage_info.connection storage_info.table_name ("k="^key) ()
        >>= fun () -> Guard.return () (RemovalMap.remove key removals)
    else (
      Logs.debug (fun m -> m "[SQL] remove on %s dropped: out-of-date" (Path.to_string path));
      Lwt.return_unit)
  | Unkown ->
    (* NOTE: even if path is not known yet, we need to store the removal time:
       if ever a put with a lower timestamp arrive (e.g. msg inversion between put and remove) 
       we must drop the put.
    *)
    Guard.guarded storage_info.removals
    @@ fun removals -> Guard.return () (RemovalMap.remove key removals)

