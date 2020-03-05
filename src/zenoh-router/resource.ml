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
open NetService
open R_name

type mapping = {
    id : Vle.t;
    session : Id.t;
    pub : bool;
    sub : bool option;
    sto : int option;
    eval : int option;
    matched_pub : bool;
    matched_sub : bool;
}

type t = {
    name : ResName.t;
    mappings : mapping list;
    matches : ResName.t list;
    local_id : Vle.t;
}

let create_mapping id session = 
    {
        id;
        session;
        pub = false;
        sub = None;
        sto = None;
        eval = None;
        matched_pub = false;
        matched_sub = false;
    }

let report_mapping m = 
    Printf.sprintf "SID:%2s RID:%2d PUB:%-4s SUB:%-4s MPUB:%-4s MSUB:%-4s" 
    (Id.to_string m.session) (Vle.to_int m.id)
    (match m.pub with true -> "YES" | false -> "NO")
    (match m.sub with None -> "NO" | Some true -> "PULL" | Some false -> "PUSH")
    (match m.matched_pub with true -> "YES" | false -> "NO")
    (match m.matched_sub with true -> "YES" | false -> "NO")

let report res = 
    Printf.sprintf "Resource name %s\n  mappings:\n" (ResName.to_string res.name) |> fun s ->
    List.fold_left (fun s m -> s ^ "    " ^ report_mapping m ^ "\n") s res.mappings ^ 
    "  matches:\n" |> fun s -> List.fold_left (fun s mr -> s ^ "    " ^ (ResName.to_string mr) ^ "\n") s res.matches

let with_mapping res mapping = 
    {res with mappings = mapping :: List.filter (fun m -> not (Id.equal m.session mapping.session)) res.mappings}

let update_mapping_opt res sid updater = 
    let mapping = List.find_opt (fun m -> m.session = sid) res.mappings in 
    match updater mapping with 
    | Some mapping -> with_mapping res mapping
    | None -> res

let update_mapping res sid updater = 
    update_mapping_opt res sid (fun ores -> Some (updater ores))

let remove_mapping res sid = 
    {res with mappings = List.filter (fun m -> not (Id.equal m.session sid)) res.mappings}

let with_match res mname = 
    {res with matches = mname :: List.filter (fun r -> r != mname) res.matches}

let remove_match res mname = 
    {res with matches = List.filter (fun r -> r != mname) res.matches}

let res_match res1 res2 = ResName.name_match res1.name res2.name

let get_matching l res = List.filter (fun r -> res_match r res) l