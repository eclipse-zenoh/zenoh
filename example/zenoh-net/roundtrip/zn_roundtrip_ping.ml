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
open Cmdliner

let peers = Arg.(value & opt string "tcp/127.0.0.1:7447" & info ["p"; "peers"] ~docv:"PEERS" ~doc:"peers")
let nb = Arg.(value & opt int 10000 & info ["n"; "nb"] ~docv:"NB" ~doc:"nb roundtrip iteration")
let size = Arg.(value & opt int 8 & info ["s"; "size"] ~docv:"SIZE" ~doc:"payload size")

type state = {wr_time:float; count:int; rt_times:float list}
let state = MVar_lwt.create_empty ()

let (promise, resolver) = Lwt.task ()

let run peers nb size = 
  Lwt_main.run 
  (
    let%lwt _ = MVar_lwt.put state {wr_time=Unix.gettimeofday(); count=0; rt_times=[];} in

    let%lwt z = zopen peers in
    let%lwt pub = publish z "/roundtrip/ping" in

    let listener _ _ state = 
      let now = Unix.gettimeofday() in 
      (match state.count + 1 < nb with 
      | true -> Lwt.ignore_result @@ stream pub (Abuf.create size)
      | false -> Lwt.wakeup_later resolver ());
      Lwt.return (Lwt.return_unit, {wr_time=now; count=state.count + 1; rt_times=(now -. state.wr_time) :: state.rt_times}) in

    let%lwt _ = subscribe z "/roundtrip/pong" (fun r ds -> MVar_lwt.guarded state (listener r ds)) in

    Unix.sleep 2; (* Avoid "declare & shoot" issue. TODO : remove when fixed *)

    Lwt.ignore_result @@ stream pub (Abuf.create size);

    let%lwt _ = promise in
    let%lwt state = MVar_lwt.take state in
    state.rt_times |> List.rev |> List.iter (fun time -> Printf.printf "%i\n" (int_of_float (time *. 1000000.0))); 
    Printf.printf "%!";
    Lwt.return_unit
  )

let () = 
  Printexc.record_backtrace true;
  Lwt_engine.set (new Lwt_engine.libev ()) ;
  let _ = Term.(eval (const run $ peers $ nb $ size, Term.info "roundtrip_ping")) in  ()
