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
let port = Arg.(value & opt string "7447" & info ["p"; "port"] ~docv:"PORT" ~doc:"port")
let samples = Arg.(value & opt int 100000 & info ["n"; "samples"] ~docv:"SAMPLES" ~doc:"number of samples")
let size = Arg.(value & opt int 1024 & info ["s"; "size"] ~docv:"SIZE" ~doc:"payload size")
let register_path = Arg.(value & opt bool true & info ["r"; "register_path"] ~docv:"true|false" ~doc:"Use RegisteredPath or Workspage put operation")

let rec put_registered_path n rpath value =
    if n > 1 then
        begin
            let%lwt _ = Zenoh.RegisteredPath.put value rpath in
            put_registered_path (n-1) rpath value
        end
    else Zenoh.RegisteredPath.put value rpath

let rec put_path n ws path value =
    if n > 1 then
        begin
            let%lwt _ = Zenoh.Workspace.put path value ws in
            put_path (n-1) ws path value
        end
    else Zenoh.Workspace.put path value ws

let create_data n =
    let rec r_create_data n s =
        if n > 0 then
            r_create_data (n-1) (s ^ (string_of_int @@ n mod 10))
        else s
    in  r_create_data n ""

let run addr port samples size register_path =
  Lwt_main.run 
  (
    let locator = Printf.sprintf "tcp/%s:%s" addr port in 
    let%lwt y = Zenoh.login locator Properties.empty in 
    let%lwt ws = Zenoh.workspace ~//"/" y  in
    let path = ~//"/ythrp/sample" in 
    let value = Value.StringValue (create_data size) in 
    let start = Unix.gettimeofday () in 
    let%lwt rpath = Zenoh.Workspace.register_path path ws in
    let%lwt () =
        if register_path
        then put_registered_path samples rpath value
        else put_path samples ws path value
    in
    let stop = Unix.gettimeofday () in 
    let delta = stop -. start in 
    let%lwt  _ = Lwt_io.printf "Sent %i samples in %fsec \n" samples delta in
    let%lwt  _ = Lwt_io.printf "Throughput: %f msg/sec\n" ((float_of_int samples) /. delta) in
    let%lwt  _ = Lwt_io.printf "Average: %f\n" (( delta) /. float_of_int samples) in
    Lwt.return_unit
  )

let () =
    let _ = Term.(eval (const run $ addr $port $ samples $ size $register_path, Term.info "ythr_put")) in  ()
