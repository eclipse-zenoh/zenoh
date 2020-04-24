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

type db_info = {
    name: string
  ; query_url: Uri.t
  ; write_url: Uri.t
}

type on_dispose = DropDB | DropAllSeries | DoNothing

type storage_info = {
  selector : Selector.t
; keys_prefix : Path.t     (* prefix of the selector that is not included in the keys stored in the table *)
; keys_prefix_length : int
; props : properties
; db : db_info
; on_dispose : on_dispose
}

type serie = {
    name: string
  ; columns: string list
  ; values: Yojson.Safe.t list list option [@default None]
}
[@@deriving yojson { exn = true }]

type statement = {
    statement_id: int
  ; series: serie list   [@default []]
  ; error: string option [@default None]
}
[@@deriving yojson { exn = true }]

type results = {
  results: statement list
}
[@@deriving yojson { exn = true }]

type json_error = {
  error: string
}
[@@deriving yojson { exn = true }]



let on_dispose_from_properties props =
  match Properties.get "on_dispose" props with
  | Some text ->
    let t = String.uppercase_ascii text in
    if t = "DROP" then DropDB
    else if t = "DROPDB" then DropDB
    else if t = "DROPALLSERIES" then DropAllSeries
    else (Logs.err (fun m -> m "[Inflx]: unsupported property: %s=%s - ignore it" "on_dispose" text); DoNothing)
  | None -> DoNothing

let measurement_from_path storage_info path =
  Astring.with_range ~first:storage_info.keys_prefix_length (Path.to_string path)
