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

(************************************************)
(*   Operations on structured (legacy) tables   *)
(************************************************)
let get_sql_table storage_info (selector:Selector.t) =
    Logs.debug (fun m -> m "[SQL]: get(%s) from legacy table %s"
                (Selector.to_string selector) (storage_info.table_name));
    let open Lwt.Infix in
    match Selector.as_unique_path storage_info.selector with
    | Some path ->
    let (col_names, typ) = storage_info.schema in
    Caqti_driver.get storage_info.connection storage_info.table_name typ ?condition:(Selector.predicate selector) ()
    >|= List.map (fun row -> path, Value.SqlValue (row, Some col_names))
    | None -> Lwt.fail @@ YException (`InternalError (`Msg 
        ("Selector for SQL storage on legacy table "^storage_info.table_name^" is not a unique path as it was assumed")))

let put_sql_table storage_info (path:Path.t) (value:Value.t) =
    Logs.debug (fun m -> m "[SQL]: put(%s) into legacy table %s"
                (Path.to_string path) (storage_info.table_name));
    let open Value in 
    match transcode value SQL with 
    | Ok SqlValue (row, _) -> Caqti_driver.put storage_info.connection storage_info.table_name storage_info.schema row ()
    | Ok _ -> Lwt.fail @@ YException (`UnsupportedTranscoding (`Msg "Transcoding to SQL didn't return an SqlValue"))
    | Error e -> Lwt.fail @@ YException e

let update_sql_table storage_info path delta =
    let open LwtM.InfixM in
    Logs.debug (fun m -> m "[SQL]: put_delta(%s) into legacy table %s"
                (Path.to_string path) (storage_info.table_name));
    get_sql_table storage_info (Selector.of_path path)
    >>= Lwt_list.iter_p (fun (_,v) -> 
        match Value.update v ~delta with
        | Ok SqlValue (row, _) ->
          Caqti_driver.put storage_info.connection storage_info.table_name storage_info.schema row ()
        | Ok _ -> Logs.warn (fun m -> m "[SQL]: put_delta on value %s failed: update of SqlValue didn't return a SqlValue" (Value.to_string v));
          Lwt.return_unit
        | Error e -> Logs.warn (fun m -> m "[SQL]: put_delta on value %s failed: %s" (Value.to_string v) (show_yerror e));
          Lwt.return_unit)

let remove_sql_table storage_info path time = 
    (* TODO:
        Remove operation used to be on Selectors with a query (see commented code below).
        But now that a Path is given as argument, there is no longer a query part, and thus
        we can't decide which ro to remove in the SQL table.
        What we need is really to expose the SQL keys of the table in the zenoh path (similarly to KV tables below) 
    *)
    (*
    Logs.debug (fun m -> m "[SQL]: remove(%s) from legacy table %s"
                (Path.to_string path) (storage_info.table_name));
    match Path.get_query path with
    | Some q -> Caqti_driver.remove C.conx storage_info.table_name q ()
    | None -> 
    Logs.debug (fun m -> m "[SQL]: Can't remove path %s without a query" (Path.to_string path));
    Lwt.return_unit
    *)
    let _ = ignore time in
    Logs.err (fun m -> m "[SQL]: Can't remove Path %s in Storage with selector %s - remove operation for legacy SQL tables is not implemented"
                                (Path.to_string path) (Selector.to_string storage_info.selector));
    Lwt.return_unit
