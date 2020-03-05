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


type state = {wr_time:float; count:int; rt_times:float list}
let state = MVar_lwt.create_empty () 

let (promise, resolver) = Lwt.task ()

let rec put_n n ws base_path value = 
    if n > 1 then
        begin
            let%lwt _ = Zenoh.Workspace.put ~//base_path value ws in
            put_n (n-1) ws base_path value 
        end
    else Zenoh.Workspace.put ~//base_path value ws

let create_data n =
    let rec r_create_data n s = 
        if n > 0 then
            r_create_data (n-1) (s ^ (string_of_int @@ n mod 10))
        else s
    in r_create_data n ""
 

let run addr port samples size = 
  Lwt_main.run 
  (
    let%lwt _ = MVar_lwt.put state {wr_time=Unix.gettimeofday(); count=0; rt_times=[];} in
    let locator = Printf.sprintf "tcp/%s:%s" addr port in 
    let%lwt y = Zenoh.login locator Properties.empty in 
    let%lwt ws = Zenoh.workspace ~//"/" y in 
    let value = Value.StringValue (create_data size) in

    let obs _ state = 
        let now = Unix.gettimeofday() in 
        (match state.count + 1 < samples with 
        | true -> Lwt.ignore_result @@ Zenoh.Workspace.put ~//"/test/lat/ping" value ws 
        | false -> Lwt.wakeup_later resolver ());
        Lwt.return (Lwt.return_unit, {wr_time=now; count=state.count + 1; rt_times=(now -. state.wr_time) :: state.rt_times}) in

    let%lwt _ = Zenoh.Workspace.subscribe ~listener:(fun ds -> MVar_lwt.guarded state (obs ds))  ~/*"/test/lat/pong"  ws in
    let%lwt _ = put_n samples ws "/test/lat/ping" value in
    let%lwt _ = promise in
    let%lwt state = MVar_lwt.take state in
    state.rt_times |> List.rev |> List.iter (fun time -> Printf.printf "%i\n" (int_of_float (time *. 1000000.0))); 
    Printf.printf "%!";
    Lwt.return_unit
  )

let () =
    let _ = Term.(eval (const run $ addr $port $ samples $ size, Term.info "ylat_ping")) in  ()