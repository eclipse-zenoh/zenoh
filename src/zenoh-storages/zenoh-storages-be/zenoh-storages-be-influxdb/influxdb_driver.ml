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
open Lwt.Infix
open Cohttp
open Cohttp_lwt
open Cohttp_lwt_unix
open Zenoh_types
open Zenoh_common_errors
open Influxdb_types



let results_to_string results = results_to_yojson results |> Yojson.Safe.to_string


let raise_unexpected_results (db:db_info) query results =
  Lwt.fail @@ YException (`StoreError (`Msg (
    Printf.sprintf "Query '%s' on InfluxDB '%s' returned an unexpected results: %s"
    query db.name (results_to_string results))))


let response_to_results (response, body) =
  let resp_code = Code.code_of_status @@ Response.status response in
  let%lwt bodystr = Body.to_string body in
  if Code.is_success resp_code then
    begin
      if bodystr = "" then Lwt.return (Ok {results=[]})
      else begin
        let results = results_of_yojson_exn @@ Yojson.Safe.from_string bodystr in
        match results with
        | {results=[{ statement_id=_; series=_; error=Some err }]} -> Lwt.return (Error err)
        | _ -> Lwt.return (Ok results)
      end
    end
  else
    if bodystr = "" then
      Lwt.return (Error (Printf.sprintf "http error code %d" resp_code))
    else
      begin
        let err = json_error_of_yojson_exn @@ Yojson.Safe.from_string bodystr in
        Lwt.return (Error (Printf.sprintf "http error code %d : %s" resp_code err.error))
      end


let timedvalue_to_point measurement (tv:TimedValue.t) =
  let ts = Timestamp.get_time tv.time |> Timestamp.Time.to_seconds in
  Printf.sprintf "%s,encoding=%s,timestamp=%s value=\"%s\" %d%09d"
    measurement Value.(encoding_to_string @@ encoding tv.value)
    (Timestamp.to_string tv.time) (Value.to_string tv.value)
    Float.(to_int ts) Float.(to_int @@ (ts -. (floor ts)) *. 1000000000.0)
    (* Note: InfluxDB timestamp format is the number of nanosec since Epoch.
      See https://docs.influxdata.com/influxdb/v1.7/write_protocols/line_protocol_tutorial/#timestamp *)

let fields_to_timedvalue serie_name time encoding timestamp value =
  ignore time;
  match Value.string_to_encoding encoding , Timestamp.of_string timestamp with
  | Some encoding , Some time -> begin
      match Value.of_string value encoding with
      | Ok value -> Some ({time; value}:TimedValue.t)
      | Error err ->
        Logs.err (fun m -> m "[Infx] Failed to decode the value '%s' from serie %s (value ignored): %s"
          value serie_name (show_yerror err));
        None
    end
  | None , _ -> 
    Logs.err (fun m -> m "[Infx] Unkown encoding '%s' found in serie %s (value ignored)"
      encoding serie_name);
      None
  | _ , None ->
    Logs.err (fun m -> m "[Infx] Failed to decode timestamp '%s' found in serie %s (value ignored)"
      timestamp serie_name);
      None

let serie_to_timedvalues serie =
  if serie.columns = ["time"; "encoding"; "timestamp"; "value"] then (
    List.fold_left (fun (result:TimedValue.t list) fields ->
      match fields with
      | [ `String time ; `String encoding ; `String timestamp ; `String value] ->
        (match fields_to_timedvalue serie.name time encoding timestamp value with
        | Some tv -> tv::result
        | None -> result)
      | _ -> Logs.err (fun m -> m "[Infx] A point from serie %s has incorect tags/fields types (value ignored): %s"
                serie.name (String.concat "," (List.map Yojson.Safe.to_string fields)));
             result
    ) [] serie.values
  ) else (
    Logs.err (fun m -> m "[Infx] Serie %s has invalid tags and fields for a zenoh value: %s (serie ignored)"
      serie.name (String.concat " , " serie.columns));
    []
  )


let ping base_url =
  let ping_url = Uri.of_string @@ base_url^"/ping" in
  Logs.debug (fun m -> m "[Infx] ping InfluxDB on %a" Uri.pp ping_url);
  match%lwt Client.get ping_url >>= response_to_results with
  | Ok _ -> Lwt.return_unit
  | Error err -> Lwt.fail @@ YException (`InvalidBackendProperty (`Msg (
      Printf.sprintf "Failed to connect to %s: %s" base_url err)))


let query (db:db_info) ?(post=false) q =
  Logs.debug (fun m -> m "[Infx] query on db %s : %s" db.name q);
  let query_param = ("q", q) in
  let uri = Uri.add_query_param' db.query_url query_param in
  let%lwt response = match post with
    | false -> Client.get uri
    | true -> Client.post uri
  in
  match%lwt response_to_results response with
  | Ok r -> Lwt.return r
  | Error err -> Lwt.fail @@ YException (`StoreError (`Msg (
      Printf.sprintf "Query '%s' on InfluxDB '%s' failed: %s" q db.name err)))

let query_keyvalues (db:db_info) q =
  match%lwt query db q with
  | {results=[{ statement_id=0; series=series; error=None }]} ->
    Lwt.return @@ List.fold_left begin
      fun result (serie: serie)  ->
        let sub_path = serie.name in
        let values = serie_to_timedvalues serie in
        (sub_path, values)::result
    end [] series 
  | res -> raise_unexpected_results db q res

let write (db:db_info) measurement (tv:TimedValue.t) =
  Logs.debug (fun m -> m "[Infx] write on db %s measurement %s" db.name measurement);
  let point = timedvalue_to_point measurement tv in
  match%lwt Client.post ~body:(Body.of_string point) db.write_url >>= response_to_results with
  | Ok _ -> Lwt.return_unit
  | Error err -> Lwt.fail @@ YException (`StoreError (`Msg (
      Printf.sprintf "Failed to write measurement %s to InfluxDB %s: %s" measurement db.name err)))



let create_database (db:db_info) =
  match%lwt query db "SHOW DATABASES" with
  | {results=[{ statement_id=0; series=[{name="databases"; columns=["name"]; values=v}]; error=None }]} ->
    let existing_db = List.flatten v in
    if List.exists (fun name -> name = `String db.name) existing_db then
      Lwt.return_unit
    else
      begin
        Logs.debug (fun m -> m "[Infx] create new db %s" db.name);
        match %lwt query ~post:true db ("CREATE DATABASE "^db.name) with
        | {results=[{ statement_id=_; series=_; error=None }]} -> Lwt.return_unit
        | {results=[{ statement_id=_; series=_; error=Some err }]} -> raise @@ YException (`StoreError (`Msg (
            Printf.sprintf "Query 'CREATE DATABASE %s' failed: %s" db.name err)))
        | res -> raise_unexpected_results db ("CREATE DATABASE "^db.name) res
      end

  | res -> raise_unexpected_results db "SHOW DATABASES " res

let drop_database db =
    query db ("DROP DATABASE "^db.name) >>= fun _ -> Lwt.return_unit

let drop_all_series db =
    query db ("DROP SERIES FROM /.*/") >>= fun _ -> Lwt.return_unit





let get_db base_url name =
  let db_info = {
      name
    ; query_url=(Uri.of_string @@ base_url^"/query?db="^name)
    ; write_url=(Uri.of_string @@ base_url^"/write?precision=ns&db="^name)
  }
  in
  let%lwt () = create_database db_info in
  Lwt.return db_info




