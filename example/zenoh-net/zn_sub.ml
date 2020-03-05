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
open Apero.Infix

let listener src =
  let abuf_to_string buf = Abuf.read_bytes (Abuf.readable_bytes buf) buf in
  List.iter (fst %> abuf_to_string %> Bytes.to_string %> Printf.printf ">> [Subscription listener] Received ('%s': '%s')\n%!" src) 
  %> Lwt.return

let locator = match Array.length Sys.argv with 
  | 1 -> "tcp/127.0.0.1:7447"
  | _ -> Sys.argv.(1)

let uri = match Array.length Sys.argv with 
  | 1 | 2 -> "/demo/example/**"
  | _ -> Sys.argv.(2)

let run = 
  Printf.printf "Connecting to %s...\n%!" locator;
  let%lwt z = zopen locator in 

  Printf.printf "Declaring Subscriber on '%s'...\n%!" uri;
  let%lwt sub = subscribe z uri listener in

  let rec loop = fun () ->
    match%lwt Lwt_io.read_char Lwt_io.stdin with
    | 'q' -> Lwt.return_unit
    | _ -> loop ()
  in
  let%lwt _ = loop () in

  let%lwt _ = unsubscribe z sub in 
  zclose z


let () = 
  Lwt_main.run @@ run