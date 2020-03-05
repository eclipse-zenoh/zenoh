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
  | 1 | 2 -> "/demo/example/zenoh-net-ocaml-write"
  | _ -> Sys.argv.(2)

let value = match Array.length Sys.argv with 
  | 1 | 2 | 3 -> "Write from OCaml!"
  | _ -> Sys.argv.(3)

let run =
  Printf.printf "Connecting to %s...\n%!" locator;
  let%lwt z = zopen locator in 

  Printf.printf "Writing Data ('%s': '%s')...\n%!" uri value;
  let%lwt _ = write z uri (Abuf.from_bytes @@ Bytes.of_string value) in
  
  zclose z


let () = 
  Lwt_main.run @@ run
