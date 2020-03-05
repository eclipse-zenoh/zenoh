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


let _ = 
  let argv = Sys.argv in
  Lwt_main.run 
  (
    let locator = if (Array.length argv > 1) then Array.get argv 1 else "tcp/127.0.0.1:7447" in
    let selector = if (Array.length argv > 2) then Array.get argv 2 else "/demo/example/**" in

    let%lwt () = Lwt_io.printf "Login to %s\n" locator in
    let%lwt y = Zenoh.login locator Properties.empty in

    let%lwt () = Lwt_io.printf "Use Workspace on '/'\n" in
    let%lwt w = Zenoh.workspace ~//"/" y in


    let%lwt () = Lwt_io.printf "Subscribe on %s\n" selector in
    let%lwt subid = Workspace.subscribe ~/*selector w
      ~listener:begin
        Lwt_list.iter_p begin
          fun (path, change) ->
            match change with
            | Put tv -> Lwt_io.printf ">> [Subscription listener] Received PUT on '%s': '%s')\n" (Path.to_string path) (Value.to_string tv.value)
            | Update tv -> Lwt_io.printf ">> [Subscription listener] Received UPDATE on '%s': '%s')\n" (Path.to_string path) (Value.to_string tv.value)
            | Remove _ -> Lwt_io.printf ">> [Subscription listener] Received REMOVE on '%s')\n" (Path.to_string path)
        end
      end
    in
    
    let%lwt () = Lwt_io.printf "Enter 'q' to quit...\n%!" in
    let rec loop () = match%lwt Lwt_io.read_char Lwt_io.stdin with
      | 'q' -> Lwt.return_unit
      | _ -> loop()
    in
    let%lwt () = loop () in

    let%lwt () = Workspace.unsubscribe subid w in
    Zenoh.logout y
  )
