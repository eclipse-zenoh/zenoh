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
  | 1 | 2 -> "/demo/example/zenoh-net-ocaml-eval*"
  | _ -> Sys.argv.(2)

let qhandler resname predicate = 
  Printf.printf ">> [Query handler] Handling '%s?%s'\n%!" resname predicate;
  let data = Abuf.from_bytes @@ Bytes.of_string "Eval from OCaml!" in
  let info = Ztypes.empty_data_info in
  Lwt.return @@ (uri, data, info) :: []

let run = 
  Printf.printf "Connecting to %s...\n%!" locator;
  let%lwt z = zopen locator in 

  Printf.printf "Declaring Eval on '%s'...\n%!" uri;
  let%lwt eval = evaluate z uri qhandler in

  let rec loop = fun () ->
    match%lwt Lwt_io.read_char Lwt_io.stdin with
    | 'q' -> Lwt.return_unit
    | _ -> loop ()
  in
  let%lwt _ = loop () in

  let%lwt _ = unevaluate z eval in 
  zclose z


let () = 
  Lwt_main.run @@ run