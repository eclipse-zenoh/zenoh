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
open Zenoh_net
open Apero

module StrMap = Map.Make(String)

let stored = Lwt_mvar.create (StrMap.empty)

let listener src datas = 
  let abuf_to_string buf = Abuf.read_bytes (Abuf.readable_bytes buf) buf in
  let%lwt storage = Lwt_mvar.take stored in
  let storage = List.fold_left (
    fun storage data -> (data |> fst |> abuf_to_string |> Bytes.to_string |> Printf.printf ">> [Storage listener] Received ('%20s' : '%s')\n%!" src);
                         Abuf.reset_r_pos (fst data);
                         StrMap.add src data storage) 
    storage datas in 
  Lwt_mvar.put stored storage

let qhandler resname predicate = 
  Printf.printf ">> [Query handler   ] Handling '%s?%s'\n%!" resname predicate;
  let%lwt storage = Lwt_mvar.take stored in
  let%lwt _ = Lwt_mvar.put stored storage in
  storage
  |> StrMap.filter (fun storesname _ -> PathExpr.intersect (PathExpr.of_string storesname) (PathExpr.of_string resname)) 
  |> StrMap.bindings 
  |> List.map (fun (k, (data, info)) -> (k, data, info))
  |> Lwt.return

let locator = match Array.length Sys.argv with 
  | 1 -> "tcp/127.0.0.1:7447"
  | _ -> Sys.argv.(1)

let uri = match Array.length Sys.argv with 
  | 1 | 2 -> "/demo/example/**"
  | _ -> Sys.argv.(2)


let run = 
  Printf.printf "Connecting to %s...\n%!" locator;
  let%lwt z = zopen locator in 

  Printf.printf "Declaring Storage on '%s'...\n%!" uri;
  let%lwt sto = store z uri listener qhandler in

  let rec loop = fun () ->
    match%lwt Lwt_io.read_char Lwt_io.stdin with
    | 'q' -> Lwt.return_unit
    | _ -> loop ()
  in
  let%lwt _ = loop () in

  let%lwt _ = unstore z sto in 
  zclose z


let () = 
  Lwt_main.run @@ run