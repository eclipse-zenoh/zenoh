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
    (* If not specified as 2nd argument, use a relative path (to the workspace below): "zenoh-ocaml-put" *)
    let path = if (Array.length argv > 2) then Array.get argv 2 else "zenoh-ocaml-put" in
    let value = if (Array.length argv > 3) then Array.get argv 3 else "Put from zenoh OCaml!" in

    let%lwt () = Lwt_io.printf "Login to %s\n" locator in
    let%lwt y = Zenoh.login locator Properties.empty in

    let%lwt () = Lwt_io.printf "Use Workspace on '/demo/example'\n" in
    let%lwt w = Zenoh.workspace ~//"/demo/example" y in

    let%lwt () = Lwt_io.printf "Put on %s : %s\n" path value in
    let%lwt () = Workspace.put ~//path ~$value w in

    Zenoh.logout y
  )
