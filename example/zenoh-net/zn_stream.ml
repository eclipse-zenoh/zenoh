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

let locator = match Array.length Sys.argv with 
  | 1 -> "tcp/127.0.0.1:7447"
  | _ -> Sys.argv.(1)

let uri = match Array.length Sys.argv with 
  | 1 | 2 -> "/demo/example/zenoh-net-ocaml-stream"
  | _ -> Sys.argv.(2)

let value = match Array.length Sys.argv with 
  | 1 | 2 | 3 -> "Stream from OCaml!"
  | _ -> Sys.argv.(3)

let run =
  Printf.printf "Connecting to %s...\n%!" locator;
  let%lwt z = zopen locator in 

  Printf.printf "Declaring Publisher on '%s'...\n%!" uri;
  let%lwt pub = publish z uri in

  let idx = 0 in
  let rec loop = fun () ->
    let%lwt _ = Lwt_unix.sleep 1.0 in
    let s = Printf.sprintf "[%4d] %s" idx value in
    Printf.printf "Streaming Data ('%s': '%s')...\n%!" uri s;
    let%lwt _ = stream pub (Abuf.from_bytes @@ Bytes.of_string s) in
    loop ()
  in
  let%lwt _ = loop () in

  let%lwt _ = unpublish z pub in
  zclose z
  

let () = 
  Lwt_main.run @@ run
