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
open Cmdliner

let peers = Arg.(value & opt string "tcp/127.0.0.1:7447" & info ["p"; "peers"] ~docv:"PEERS" ~doc:"peers")

type state = {mutable starttime:float; mutable count:int;}

let state =  {starttime=0.0; count=0}

let listener _ _ = 
    let now = Unix.gettimeofday() in 
    Lwt.return @@ match state.starttime with 
    | 0.0 -> state.starttime <- now; state.count <- 1
    | time when now < (time +. 1.0) -> state.count <- state.count + 1
    | _ -> 
      Printf.printf "%d\n%!" (state.count + 1)
      ; state.starttime <-now
      ; state.count <- 0 
    

let run peers = 
  Lwt_main.run 
  (
    let%lwt z = zopen peers in 
    let%lwt _ = subscribe z "/home1" listener in 
    let%lwt _ = Lwt_unix.sleep 3000.0 in 
    Lwt.return_unit
  )

let () = 
  let _ = Term.(eval (const run $ peers, Term.info "throughput_sub")) in  ()