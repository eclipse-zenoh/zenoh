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

let handler = 
  let abuf_to_string buf = Bytes.to_string @@ Abuf.read_bytes (Abuf.readable_bytes buf) buf in
  function
  | StorageData rep ->
    let str = abuf_to_string rep.data in
    Printf.printf ">> [Reply handler] received -Storage Data- ('%s': '%s')\n%!" rep.resname str;
    Lwt.return_unit
  | EvalData rep ->
    let str = abuf_to_string rep.data in
    Printf.printf ">> [Reply handler] received -Eval Data-    ('%s': '%s')\n%!" rep.resname str;
    Lwt.return_unit
  | StorageFinal _ -> 
    Printf.printf ">> [Reply handler] received -Storage Final-\n%!";
    Lwt.return_unit
  | EvalFinal _ -> 
    Printf.printf ">> [Reply handler] received -Eval Final-\n%!";
    Lwt.return_unit
  | ReplyFinal -> 
    Printf.printf ">> [Reply handler] received -Reply Final-\n%!";
    Lwt.return_unit

let locator = match Array.length Sys.argv with 
  | 1 -> "tcp/127.0.0.1:7447"
  | _ -> Sys.argv.(1)

let uri = match Array.length Sys.argv with 
  | 1 | 2 -> "/demo/example/**"
  | _ -> Sys.argv.(2)

let run = 
  Printf.printf "Connecting to %s...\n%!" locator;
  let%lwt z = zopen locator in 

  Printf.printf "Sending Query '%s'...\n%!" uri;
  let%lwt () = query z uri "" handler in

  let%lwt _ = Lwt_unix.sleep 1.0 in

  zclose z


let () = 
  Lwt_main.run @@ run