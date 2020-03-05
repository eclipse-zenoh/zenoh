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
open Zenoh
open Zenoh.Infix
open Cmdliner

let addr = Arg.(value & opt string "127.0.0.1" & info ["a"; "addr"] ~docv:"ADDRESS" ~doc:"address")
let port = Arg.(value & opt string "7887" & info ["p"; "port"] ~docv:"PORT" ~doc:"port")
let samples = Arg.(value & opt int 10000 & info ["n"; "samples"] ~docv:"SAMPLES" ~doc:"number of samples")
let size = Arg.(value & opt int 1024 & info ["s"; "size"] ~docv:"SIZE" ~doc:"payload size")
    
(* performs gets in serial*)
let rec get_n_s n selector ws result  = 
    if n > 1 then
        begin
            let start = Unix.gettimeofday () in    
            let%lwt _ = Zenoh.Workspace.get selector ws in 
            let stop = Unix.gettimeofday () in 
            let delta = stop -. start in 
            let lt = int_of_float (delta *. 1000000.0) in
            Array.set result (n-1) lt;
            get_n_s (n-1) selector ws result
        end 
    else Zenoh.Workspace.get selector ws

let create_data n =
    let rec r_create_data n s = 
        if n > 0 then
            r_create_data (n-1) (s ^ (string_of_int @@ n mod 10))
        else s
    in r_create_data n ""


let run addr port samples size =
  Lwt_main.run 
  (
    let result = Array.make samples 0 in
    let locator = Printf.sprintf "tcp/%s:%s" addr port in  
    let%lwt y = Zenoh.login locator Properties.empty in 
    let%lwt ws = Zenoh.workspace ~//"/" y in 
    let value = Value.StringValue (create_data size) in
    let _ = Zenoh.Workspace.put ~//"test/lat/get" value ws in
    let selector = ~/*"/test/thr/get" in 
    let%lwt _ = get_n_s samples selector ws result in
    let _ = Array.iter (Printf.printf "%i\n") result in
    Lwt.return_unit
  )

let () =
    let _ = Term.(eval (const run $ addr $port $ samples $ size, Term.info "ylatg")) in  ()
