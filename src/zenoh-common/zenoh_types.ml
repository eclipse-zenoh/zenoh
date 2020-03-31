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
open Zenoh_common_errors



module Path = Apero.Path

module Selector = struct

  type t = { path: PathExpr.t; pred: string option; props: string option; frag: string option }


  let sel_regex =
    let path = "[^][?#]+" in
    let predicate = "[^][()#]+" in
    let properties = ".*" in
    let fragment = ".*" in
    Str.regexp @@ Printf.sprintf "^\\(%s\\)\\(\\?\\(%s\\)?\\((\\(%s\\))\\)?\\)?\\(#\\(%s\\)\\)?$" path predicate properties fragment


  let is_valid s = Str.string_match sel_regex s 0

  let of_string_opt s =
    let s = Astring.trim s in
    if is_valid s then
      match PathExpr.of_string_opt @@ Str.matched_group 1 s with
      | Some path ->
        let pred = try Some(Str.matched_group 3 s) with Not_found -> None
        and props = try Some(Str.matched_group 5 s) with Not_found -> None
        and frag = try Some(Str.matched_group 7 s) with Not_found -> None
        in
        Some { path; pred; props; frag }
      | None -> None
    else None

  let of_string s =
    Apero.Option.get_or_else (of_string_opt s)
    (fun () -> raise (YException (`InvalidPath (`Msg s))))


  let to_string s =
    Printf.sprintf "%s%s%s%s%s"
      (PathExpr.to_string s.path)
      (if Option.is_some s.pred || Option.is_some s.props then "?" else "")
      (match s.pred with | Some(q) -> q | None -> "")
      (match s.props with | Some(p) -> "("^p^")" | None -> "")
      (match s.frag with | Some(f) -> "#"^f | None -> "")

  let of_path ?predicate ?properties ?fragment p = { path=PathExpr.of_path p; pred=predicate; props=properties; frag=fragment }

  let with_path p s = { s with path=PathExpr.of_path p }

  let path s = PathExpr.to_string s.path

  let predicate s = s.pred

  let properties s = s.props

  let fragment s = s.frag

  let optional_part s = 
    Printf.sprintf "%s%s%s"
      (match s.pred with | Some(q) -> q | None -> "")
      (match s.props with | Some(p) -> "("^p^")" | None -> "")
      (match s.frag with | Some(f) -> "#"^f | None -> "")

  let is_relative sel = PathExpr.is_relative sel.path

  let add_prefix ~prefix sel = { sel with path = PathExpr.add_prefix ~prefix sel.path }

  let get_prefix sel = PathExpr.get_prefix sel.path

  let is_path_unique sel = PathExpr.is_unique sel.path

  let as_unique_path sel = PathExpr.as_unique_path sel.path

  let is_matching_path pat sel = PathExpr.is_matching_path pat sel.path

  let includes ~subsel sel = PathExpr.includes ~subexpr:subsel.path sel.path

  let intersects sel1 sel2 = PathExpr.intersect sel1.path sel2.path

  let remaining_after_match path sel =
    match PathExpr.remaining_after_match path sel.path with
    | Some remain -> Some { sel with path = remain }
    | None -> None

end


module Value = struct 
  [%%cenum
  type encoding = 
    | RAW          [@id  0x00]
    (* | Custom_Encoding       [@id  0x01] *)
    | STRING       [@id  0x02]
    | PROPERTIES   [@id  0x03]
    | JSON         [@id  0x04]
    | SQL          [@id  0x05]
    | INT          [@id  0x06]
    | FLOAT        [@id  0x07]

  [@@uint8_t]]

  type sql_row = string list
  type sql_column_names = string list

  type t  = 
    | RawValue of (string option * bytes)
    | StringValue of string
    | PropertiesValue of properties
    | JSonValue of string
    | SqlValue of (sql_row * sql_column_names option)
    | IntValue of Int64.t
    | FloatValue of Float.t

  let update ~delta _ = ignore delta; Apero.Result.fail `UnsupportedOperation

  let encoding = function 
    | RawValue _ -> RAW
    | StringValue _ -> STRING
    | PropertiesValue _ -> PROPERTIES
    | JSonValue _ -> JSON
    | SqlValue _ -> SQL
    | IntValue _ -> INT
    | FloatValue _ -> FLOAT

  let sql_val_sep = ',' (* Char.chr 31 *) (* US - unit separator *)
  let sql_val_sep_str = String.make 1 sql_val_sep
  let sql_row_sep = Char.chr 30 (* RS - record separator *)
  let sql_row_sep_str = String.make 1 sql_row_sep
  

  let sql_to_string = function
    | (row, None) -> String.concat sql_val_sep_str row
    | (row, Some col) -> (String.concat sql_val_sep_str row)^sql_row_sep_str^(String.concat sql_val_sep_str col)

  let sql_of_string s = 
    let string_to_list s = String.split_on_char sql_val_sep s |> List.map String.trim in
    match String.split_on_char sql_row_sep s with
    | row::[] -> string_to_list row , None
    | row::col::[] -> string_to_list row , Some (String.split_on_char sql_val_sep col)
    | _ -> raise @@ YException (`UnsupportedTranscoding (`Msg ("String to SQL of  "^s)))

  let to_raw_encoding v =
    let encoding_descr = encoding_to_string @@ encoding v in
    match v with
    | RawValue _ as v -> Apero.Result.ok @@ v
    | StringValue s -> Apero.Result.ok @@ RawValue (Some encoding_descr, Bytes.of_string @@ s)
    | PropertiesValue p -> Apero.Result.ok @@ RawValue (Some encoding_descr, Bytes.of_string @@ Properties.to_string p)
    | JSonValue s -> Apero.Result.ok @@ RawValue (Some encoding_descr, Bytes.of_string @@ s)
    | SqlValue v  -> Apero.Result.ok @@ RawValue (Some encoding_descr, Bytes.of_string @@ sql_to_string v)
    | IntValue i -> Apero.Result.ok @@ RawValue (Some encoding_descr, Bytes.of_string @@ Int64.to_string i)
    | FloatValue f -> Apero.Result.ok @@ RawValue (Some encoding_descr, Bytes.of_string @@ Float.to_string f)

  let to_string_encoding = function 
    | RawValue (_, r)  -> Apero.Result.ok @@ StringValue (Bytes.to_string r)  (* @TODO: base64 conversion and encoding description in string ? *)
    | StringValue _ as v  -> Apero.Result.ok @@ v
    | PropertiesValue p -> Apero.Result.ok @@ StringValue (Properties.to_string p)
    | JSonValue s -> Apero.Result.ok @@ StringValue s
    | SqlValue v -> Apero.Result.ok @@ StringValue (sql_to_string v)
    | IntValue i -> Apero.Result.ok @@ StringValue (Int64.to_string i)
    | FloatValue f -> Apero.Result.ok @@ StringValue (Float.to_string f)

  let properties_from_json json =
    let open Yojson.Basic in
    match from_string json with
    | `Assoc l -> l
      |> List.map (fun (k, j) -> match j with
        | `String v -> (k,v)
        | _ -> raise @@ YException (`UnsupportedTranscoding (`Msg ("Json to Properties of  "^json))))
      |> Properties.of_list
    | _ -> raise @@ YException (`UnsupportedTranscoding (`Msg ("Json to Properties of  "^json)))

  let properties_from_sql (row, col) =
    match col with
    | Some keys -> List.combine keys row |> Properties.of_list
    | None -> raise @@ YException (`UnsupportedTranscoding (`Msg ("SQL without columns to Properties of  "^(sql_to_string (row, col)))))


  let to_properties_encoding = function
    | RawValue (_,r)  -> Apero.Result.ok @@ PropertiesValue (Bytes.to_string r |> Properties.of_string)  (* @TODO: base64 conversion and encoding description as property ? *)
    | StringValue s  -> Apero.Result.ok @@ PropertiesValue (Properties.of_string s)
    | PropertiesValue _ as v -> Apero.Result.ok v
    | JSonValue s -> Apero.Result.ok @@ PropertiesValue (properties_from_json s)
    | SqlValue v -> Apero.Result.ok @@ PropertiesValue (properties_from_sql v)
    | IntValue i -> Apero.Result.ok @@ PropertiesValue (Properties.add "int-value" (Int64.to_string i) Properties.empty)
    | FloatValue f -> Apero.Result.ok @@ PropertiesValue (Properties.add "float-value" (Float.to_string f) Properties.empty)


  let json_from_sql (row, col) =
    let open Yojson.Basic in
    let kv_list = match col with
    | None -> List.mapi (fun i v -> "'col_"^(string_of_int i) , `String v ) row
    | Some col -> List.map2 (fun k v -> k , `String v) col row
    in
    to_string (`Assoc kv_list)

  let json_from_properties (p:properties) =
    `Assoc (Properties.fold (fun k v l -> (k, `String v)::l) p [])

  let to_json_encoding = 
    let open Yojson.Basic in
    function
    | RawValue (_, r)  -> Apero.Result.ok @@ JSonValue (Bytes.to_string r)  (* @TODO: base64 conversion and and encoding description as json attribute ? *)
    | StringValue s  -> Apero.Result.ok @@ JSonValue s
    | PropertiesValue p -> Apero.Result.ok @@ JSonValue (to_string @@ json_from_properties p)
    | JSonValue _ as v -> Apero.Result.ok @@ v
    | SqlValue v -> Apero.Result.ok @@ StringValue (json_from_sql v)
    | IntValue i -> Apero.Result.ok @@ JSonValue (Int64.to_string i) 
    | FloatValue f -> Apero.Result.ok @@ JSonValue (Float.to_string f)

  (* @TODO: use Error instead of Exception *)
  let sql_from_json json =
    let open Yojson.Basic in
    match from_string json with
    | `Assoc l -> List.split l |> fun (col, row) -> (List.map (fun json -> to_string json) row), Some col
    | _ -> raise @@ YException (`UnsupportedTranscoding (`Msg ("Json to SQL of  "^json)))

  let sql_from_properties (p:properties) =
    Properties.bindings p |> List.split |>
    fun (keys, values) -> (values, Some keys)

  let to_sql_encoding = function
    | RawValue (_, r) -> Apero.Result.ok @@ SqlValue (sql_of_string (Bytes.to_string r))  (* @TODO: base64 conversion and and encoding description as sql column ? *)
    | StringValue s  -> Apero.Result.ok @@ SqlValue (sql_of_string s)
    | PropertiesValue p -> Apero.Result.ok @@ SqlValue (sql_from_properties p)
    | JSonValue s -> Apero.Result.ok @@ SqlValue (sql_from_json s)
    | SqlValue _ as v -> Apero.Result.ok @@ v
    | IntValue i -> Apero.Result.ok @@ SqlValue (sql_of_string @@ Int64.to_string i)
    | FloatValue f -> Apero.Result.ok @@ SqlValue (sql_of_string @@ Float.to_string f)    

  let to_int_encoding = function
    | RawValue (_, bs) -> Apero.Result.ok @@ IntValue (Int64.of_string (Bytes.to_string bs))
    | StringValue s -> Apero.Result.ok @@ IntValue (Int64.of_string s)
    | PropertiesValue ps -> Apero.Result.ok @@ IntValue (Int64.of_string (Properties.find "int-value" ps))
    | JSonValue j -> Apero.Result.ok @@ IntValue (Int64.of_string j)
    | SqlValue s -> Apero.Result.ok @@ IntValue (Int64.of_string (sql_to_string s))
    | IntValue i -> Apero.Result.ok @@ IntValue i
    | FloatValue f -> Apero.Result.ok @@ IntValue (Int64.of_int @@ Float.to_int f)

(* | IntValue i -> Apero.Result.ok @@ PropertiesValue (Properties.add "int-value" (Int64.to_string i) Properties.empty)
    | FloatValue f -> Apero.Result.ok @@ PropertiesValue (Properties.add "float-value" (Float.to_string f) Properties.empty) *)

  let to_float_encoding = function
    | RawValue (_, bs) -> Apero.Result.ok @@ FloatValue (Float.of_string (Bytes.to_string bs))
    | StringValue s -> Apero.Result.ok @@ FloatValue (Float.of_string s)
    | PropertiesValue ps -> Apero.Result.ok @@ FloatValue (Float.of_string (Properties.find "float-value" ps))    
    | JSonValue j -> Apero.Result.ok @@ FloatValue (Float.of_string j)
    | SqlValue s -> Apero.Result.ok @@ FloatValue (Float.of_string (sql_to_string s))    
    | IntValue i -> Apero.Result.ok @@ FloatValue (Int64.to_float i)
    | FloatValue f -> Apero.Result.ok @@ FloatValue f    

  
  let transcode v = function   
    | RAW -> to_raw_encoding v
    | STRING -> to_string_encoding v
    | PROPERTIES -> to_properties_encoding v
    | JSON -> to_json_encoding v
    | SQL -> to_sql_encoding v
    | INT -> to_int_encoding v 
    | FLOAT -> to_float_encoding v
    
    

  let of_string s e = transcode (StringValue s)  e
  let to_string  = function 
    | RawValue (descr,r) ->
      let d = Option.get_or_default (Option.map descr (fun d -> d^": ")) "" in
      d^(Bytes.to_string r)   (* @TODO: base64 conversion *)
    | StringValue s -> s
    | PropertiesValue p -> Properties.to_string p
    | JSonValue j -> j 
    | SqlValue s -> sql_to_string s
    | IntValue i -> Int64.to_string i
    |FloatValue f -> Float.to_string f

end

include Zenoh_time

module TimedValue = struct
  type t = { time:Timestamp.t; value:Value.t }

  let update ~delta tv =
    let open Result.Infix in
    Value.update tv.value ~delta:delta.value >>> fun v -> { time=delta.time; value=v }

  let preceeds ~first ~second = Timestamp.compare first.time second.time < 0
  (** [preceeds first second] returns true if timestamp of [first] < timestamp of [second] *)

end

type change =
  | Put of TimedValue.t
  | Update of TimedValue.t
  | Remove of Timestamp.t
