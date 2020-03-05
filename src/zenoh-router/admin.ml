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
open Engine_state
open Message
open Apero

let hostname = Unix.gethostname ()

let admin_prefix = "/@/"

let is_admin q = 
  String.length (Message.Query.resource q) >= String.length admin_prefix &&
  String.equal admin_prefix (String.sub (Message.Query.resource q) 0 (String.length admin_prefix))

let broker_json pe = 
  let locators = pe.locators |> Locator.Locators.to_list |> List.map (fun l -> `String(Locator.Locator.to_string l)) in
  `Assoc [ 
      ("pid",      `String (Abuf.hexdump pe.pid));
      ("locators", `List locators);
      ("lease",    `Int (Vle.to_int pe.lease));
    ]

let full_broker_json pe = 
  let locators = pe.locators |> Locator.Locators.to_list |> List.map (fun l -> `String(Locator.Locator.to_string l)) in
  let sessions = pe.smap |> SIDMap.bindings |> List.map (fun (_, v) -> Session.to_yojson v) in
  `Assoc [ 
      ("pid",       `String (Abuf.hexdump pe.pid));
      ("hostname",  `String hostname);
      ("locators",  `List locators);
      ("lease",     `Int (Vle.to_int pe.lease));
      ("trees",    (Spn_trees_mgr.to_yojson pe.trees));
      ("sessions",  `List sessions);
    ]


let broker_path_str pe = String.concat "" [admin_prefix; "router/"; Abuf.hexdump pe.pid]
let broker_path pe = Path.of_string @@ broker_path_str pe
(* let router_path pe = Path.of_string (String.concat "" [broker_path_str pe; "/routing"])
let resource_path pe res_name = Path.of_string @@ String.concat "" [broker_path_str pe; R_name.ResName.to_string res_name]  *)

let json_replies pe q = 
  match is_admin q with 
  | false -> []
  | true -> 
  let qexpr = PathExpr.of_string (Message.Query.resource q) in 
  List.concat [
    (* match PathExpr.is_matching_path (broker_path pe) qexpr with 
    | true -> [((broker_path pe), (broker_json pe))]
    | false -> []
    ;
    match PathExpr.is_matching_path (router_path pe) qexpr with 
    | true -> [((router_path pe), (ZRouter.to_yojson pe.router))]
    | false -> []
    ; *)
    match PathExpr.is_matching_path (broker_path pe) qexpr with 
    | true -> [((broker_path pe), (full_broker_json pe))]
    | false -> []
  ]

let replies pe q = 
  let open Ztypes in
  let%lwt ts = (match pe.timestamp with 
    | true -> HLC.new_timestamp pe.hlc >>= fun ts -> Lwt.return (Some ts)
    | false -> Lwt.return None) in
  Lwt.return @@ List.mapi (fun idx (p, j) -> 
    let data = Abuf.create ~grow:65536 1024 in 
    Abuf.write_bytes (Bytes.unsafe_of_string (Yojson.Safe.to_string j)) data;
    let info = {srcid=None; srcsn=None; bkrid=None; bkrsn=None; ts; encoding=Some 4L (* JSON *); kind=None} in
    let pl = Payload.create ~header:info data in
    Reply.create (Query.pid q) (Query.qid q) Reply.Eval (Some (pe.pid, Vle.of_int (idx + 1), Path.to_string p, pl))
  ) (json_replies pe q)
