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
open Lwt.Infix
open Cmdliner

let addr = Arg.(value & opt string "127.0.0.1" & info ["a"; "addr"] ~docv:"ADDRESS" ~doc:"address")
let port = Arg.(value & opt string "7447" & info ["p"; "port"] ~docv:"PORT" ~doc:"port")
let samples = Arg.(value & opt int 10000 & info ["n"; "samples"] ~docv:"SAMPLES" ~doc:"number of samples")
let size = Arg.(value & opt int 1024 & info ["s"; "size"] ~docv:"SIZE" ~doc:"payload size")
let exec_type = Arg.(value & opt string "s" & info ["t"; "exec_type"] ~docv:"EXEC_TYPE" ~doc:"s:serial or p:parallel")

(* performs gets in parallel*)
let get_n_p n selector ws  = 
    let rec get_n_p n selector ws  = 
        if n > 1 then
            begin
                let p = Zenoh.Workspace.get selector ws >|= ignore in 
                let ps = get_n_p (n-1) selector ws in 
                p :: ps
            end 
        else [Zenoh.Workspace.get selector ws >|= ignore] in 
    Lwt.join @@ get_n_p n selector ws

(* performs gets in serial*)
let rec get_n_s n selector ws  = 
    if n > 1 then
        begin
            let%lwt _ = Zenoh.Workspace.get selector ws in 
            get_n_s (n-1) selector ws
        end 
    else Zenoh.Workspace.get selector ws >|= ignore


let create_data n =
    let rec r_create_data n s = 
        if n > 0 then
            r_create_data (n-1) (s ^ (string_of_int @@ n mod 10))
        else s
    in  r_create_data n ""

let run addr port samples size exec_type =
  Lwt_main.run 
  (
    let locator = Printf.sprintf "tcp/%s:%s" addr port in  
    let%lwt y = Zenoh.login locator Properties.empty in 
    let%lwt ws = Zenoh.workspace ~//"/" y  in
    let value = Value.StringValue (create_data size) in 
    let _ = Zenoh.Workspace.put ~//"test/thr/get" value ws in 

    let selector = ~/*"/test/thr/get" in 
    let start = Unix.gettimeofday () in 
    let%lwt _ = (match exec_type with
                    | "p" | "P" -> get_n_p samples selector ws
                    | _ -> get_n_s samples selector ws) in
    let stop = Unix.gettimeofday () in 
    let delta = stop -. start in 
    let%lwt  _ = Lwt_io.printf "Sent %i samples in %fsec \n" samples delta in
    let%lwt  _ = Lwt_io.printf "Throughput: %f msg/sec\n" ((float_of_int samples) /. delta) in
    let%lwt  _ = Lwt_io.printf "Average: %f\n" (( delta) /. float_of_int samples) in
    Lwt.return_unit
  )

let () =
    let _ = Term.(eval (const run $ addr $port $ samples $ size $exec_type, Term.info "ythrg")) in  ()

