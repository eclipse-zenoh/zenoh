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
open Apero.Infix
open Zenoh_ocaml
open Zenoh
open Zenoh.Infix
open Cmdliner

let addr = Arg.(value & opt string "127.0.0.1" & info ["a"; "addr"] ~docv:"ADDRESS" ~doc:"address")
let port = Arg.(value & opt string "7887" & info ["p"; "port"] ~docv:"PORT" ~doc:"port")

let rec infinitewait () = 
    let%lwt _ = Lwt_unix.sleep 1000.0 in 
    infinitewait ()

let run addr port =
  Lwt_main.run 
  (
    let locator = Printf.sprintf "tcp/%s:%s" addr port in 
    let%lwt y = Zenoh.login locator Properties.empty in 
    let%lwt ws = Zenoh.workspace ~//"/" y  in
    let%lwt _ = Zenoh.Workspace.subscribe ~/*"/test/lat/ping" ws 
      ~listener:(List.split %> snd %> Lwt_list.iter_s (function
                 | Put tv -> Zenoh.Workspace.put ~//"/test/lat/pong" tv.value ws
                 | _ -> Lwt.return_unit
                 ))
    in

    infinitewait ()
  )

let () =
    let _ = Term.(eval (const run $ addr $port, Term.info "ylat_pong")) in  ()