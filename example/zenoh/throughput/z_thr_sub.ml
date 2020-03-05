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
open Zenoh_ocaml
open Zenoh.Infix
open Cmdliner

let addr = Arg.(value & opt string "127.0.0.1" & info ["a"; "addr"] ~docv:"ADDRESS" ~doc:"address")
let port = Arg.(value & opt string "7447" & info ["p"; "port"] ~docv:"PORT" ~doc:"port")

type state = {mutable starttime: float; mutable count: int; n: int}

let state =  {starttime=0.0; count=0; n = 50000}


let obs _ = 
    (match state.count with 
      | 0 ->         
        state.starttime <- Unix.gettimeofday ();
        state.count <- state.count +1 
      | i when i = state.n -> 
        let delta = Unix.gettimeofday () -. state.starttime in 
        let thr = ((float_of_int) state.n) /. delta in  
        print_endline (string_of_float thr);
        state.count <- 0
      | _ ->                  
        state.count <- state.count +1)
      ; Lwt.return_unit

let run addr port =
  Lwt_main.run 
  (
    let locator = Printf.sprintf "tcp/%s:%s" addr port in 
    let%lwt y = Zenoh.login locator Properties.empty in 
    let%lwt ws = Zenoh.workspace ~//"/" y  in
    let path = "/ythrp/sample" in  
    let%lwt _ = Zenoh.Workspace.subscribe ~listener:obs ~/*path ws in
    let%lwt _ = Lwt_unix.sleep 3000.0 in 
    Lwt.return_unit
  )

let () =
    let _ = Term.(eval (const run $ addr $port, Term.info "ythr_sub")) in  ()
