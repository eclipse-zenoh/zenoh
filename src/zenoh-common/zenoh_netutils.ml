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

let kind_put = 0L
let kind_update = 1L
let kind_remove = 2L

let zenohnetid_to_zenohid id = Printf.sprintf "%s-%s-%s-%s-%s" 
  (Astring.with_index_range ~first:0 ~last:7 id)
  (Astring.with_index_range ~first:8 ~last:11 id)
  (Astring.with_index_range ~first:12 ~last:15 id)
  (Astring.with_index_range ~first:16 ~last:19 id)
  (Astring.with_index_range ~first:20 ~last:31 id)

let empty_buf = Abuf.create 0

let encoding_to_flag v = Value.encoding v |> Value.encoding_to_int |> Int64.of_int
let encoding_of_flag f = match f with 
  | None -> Value.RAW 
  | Some i -> Option.get_or_default (Value.int_to_encoding (Int64.to_int i)) Value.RAW

let encode_value v = match Value.transcode v Value.RAW with
  | Ok Value.RawValue(_, b) -> Abuf.from_bytes b
  | Ok v' -> Logs.err (fun m -> m "[Znu]: INTERNAL ERROR: transcode of value '%s' to RAW didn't return a RawValue but: '%s'" (Value.to_string v) (Value.to_string v')); empty_buf
  | Error e -> Logs.err (fun m -> m "[Znu]: INTERNAL ERROR: transcode of value '%s' to RAW failed: '%s'" (Value.to_string v) (show_yerror e)); empty_buf
let decode_value buf encoding =
  let raw_value = Value.RawValue(None, Abuf.get_bytes ~at:(Abuf.r_pos buf) (Abuf.readable_bytes buf) buf) in
  match Value.transcode raw_value encoding with
  | Ok v -> v
  | Error e -> raise @@ YException e

let timestamp0 = Timestamp.create 
  (Option.get @@ Uuid.of_string "00000000-0000-0000-0000-000000000000")
  (Option.get @@ Time.of_string "0")

let decode_time ?hlc (info:Ztypes.data_info) = match (info.ts, hlc) with 
  | (Some t, _) -> Lwt.return t 
  | (None, Some hlc) -> Logs.warn (fun m -> m "Received a data from Zenoh without timestamp; generate it");
    HLC.new_timestamp hlc
  | (None, None) -> Logs.warn (fun m -> m "Received a data from Zenoh without timestamp; set time to 0 !!!!!");
    Lwt.return timestamp0

let decode_timedvalue ?hlc (buf:Abuf.t) (info:Ztypes.data_info) =
  let%lwt time = decode_time ?hlc info in
  let encoding = encoding_of_flag info.encoding in
  let value = decode_value buf encoding in
  let (tv:TimedValue.t) = { time; value } in
  Lwt.return tv

let decode_change ?hlc (buf:Abuf.t) (info:Ztypes.data_info) =
  let open Lwt.Infix in
  match Option.get_or_default info.kind kind_put with
  | k when k=kind_put    -> decode_timedvalue ?hlc buf info >|= fun tv -> Put tv
  | k when k=kind_update -> decode_timedvalue ?hlc buf info >|= fun tv -> Update tv
  | k when k=kind_remove -> decode_time ?hlc info  >|= fun time -> Remove time
  | k -> Lwt.fail @@ YException (`InternalError (`Msg ("Unkown kind value in incoming Zenoh message: "^(Int64.to_string k))))

let decode_changes ?hlc samples =
  List.map (fun (buf, (info:Ztypes.data_info)) -> decode_change ?hlc buf info) samples |>
  Lwt_list.fold_left_s (fun acc lwt ->
    (* drop the failing Lwt.t (e.g. decoding failure), logging an error for each *)
    Lwt.try_bind (fun () -> lwt) (fun x -> Lwt.return @@ x::acc)
    (fun e -> Logs.err (fun m -> m "[Znu]: INTERNAL ERROR receiving data via Zenoh: %s" (Printexc.to_string e)); Lwt.return acc)
  ) []

let query_timedvalues zenoh ?hlc ?dest_storages ?dest_evals selector =
  let open Lwt.Infix in
  let reply_to_ktv (resname, buf, (info:Ztypes.data_info)) =
    let path = Path.of_string resname in
    let%lwt timedvalue = decode_timedvalue ?hlc buf info in
    Lwt.return (path, timedvalue)
  in
  let resname = Selector.path selector in
  let predicate = Selector.optional_part selector in
  Zenoh_net.lquery zenoh ?dest_storages ?dest_evals resname predicate
  >>= Lwt_list.map_p reply_to_ktv

let query_values zenoh selector =
  let open Lwt.Infix in
  let reply_to_kv (resname, buf, (info:Ztypes.data_info)) =
    let path = Path.of_string resname in
    let encoding = encoding_of_flag info.encoding in
    let value = decode_value buf encoding in
    (path, value)
  in
  let resname = Selector.path selector in
  let predicate = Selector.optional_part selector in
  Zenoh_net.lquery zenoh resname predicate
  >|= List.map reply_to_kv

let squery_values zenoh selector =
  let qreply_to_kv (qreply:Zenoh_net.queryreply) = match qreply with
    | StorageData {stoid=_; rsn=_; resname; data; info} ->
      let path = Path.of_string resname in
      let encoding = encoding_of_flag info.encoding in
      let value = decode_value data encoding in
      Some (path, value)
    | _ -> None
  in
  let resname = Selector.path selector in
  let predicate = Selector.optional_part selector in
  Zenoh_net.squery zenoh resname predicate
  |> Lwt_stream.filter_map qreply_to_kv

let write_put zenoh ?timestamp path (value:Value.t) =
  let res = Path.to_string path in
  let buf = encode_value value in
  let encoding = encoding_to_flag value in
    Logs.warn (fun m -> m "[Zapi]: PUT on %s : %s" (Path.to_string path) (Abuf.hexdump buf));
  Zenoh_net.write zenoh res ?timestamp ~encoding buf

let write_update zenoh ?timestamp path (value:Value.t) =
  let res = Path.to_string path in
  let buf = encode_value value in
  let encoding = encoding_to_flag value in
  Zenoh_net.write zenoh res ?timestamp ~encoding ~kind:kind_update buf

let write_remove zenoh ?timestamp  path =
  let res = Path.to_string path in
  Zenoh_net.write zenoh res ?timestamp ~kind:kind_remove empty_buf

let stream_put pub ?timestamp (value:Value.t) =
  let buf = encode_value value in
  let encoding = encoding_to_flag value in
  Zenoh_net.stream pub ?timestamp ~encoding buf

let stream_update pub ?timestamp (value:Value.t) =
  let buf = encode_value value in
  let encoding = encoding_to_flag value in
  Zenoh_net.stream pub ?timestamp ~encoding ~kind:kind_update buf

let stream_remove ?timestamp pub =
  Zenoh_net.stream pub ?timestamp ~kind:kind_remove empty_buf


let subscribe zns ?hlc ?listener selector =
  let open Lwt.Infix in
  let (zmode, zlistener) = match listener with
    | None -> (Zenoh_net.pull_mode, fun _ _ -> Lwt.return_unit)
    | Some callback -> 
      (Zenoh_net.push_mode, 
      fun resname samples -> match Path.of_string_opt resname with
        | Some path -> decode_changes ?hlc samples >>= callback path
        | None -> Logs.err (fun m -> m "[Znu]: Subscriber received data via zenoh-net for an invalid path: %s" resname); Lwt.return_unit
      )
  in
  Zenoh_net.subscribe zns (Selector.to_string selector) zlistener ~mode:zmode

let unsubscribe = Zenoh_net.unsubscribe
