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
    let p = if (Array.length argv > 2) then Array.get argv 2 else "/demo/example/zenoh-ocaml-eval" in

    let%lwt () = Lwt_io.printf "Login to %s\n" locator in
    let%lwt y = Zenoh.login locator Properties.empty in

    let%lwt () = Lwt_io.printf "Use Workspace on '/'\n" in
    let%lwt w = Zenoh.workspace ~//"/" y in


    let%lwt () = Lwt_io.printf "Register eval %s\n" p in
    let%lwt () = Workspace.register_eval ~//p
      begin
        fun path properties -> begin
          (* In this Eval function, we choosed to get the name to be returned in the StringValue in 3 possible ways,
             depending the properties specified in the selector. For example, with the following selectors:
               - "/demo/example/zenoh-java-eval" : no properties are set, a default value is used for the name
               - "/demo/example/zenoh-java-eval?(name=Bob)" : "Bob" is used for the name
               - "/demo/example/zenoh-java-eval?(name=/demo/example/name)" :
                 the Eval function does a GET on "/demo/example/name" an uses the 1st result for the name *)

          let%lwt () = Lwt_io.printf ">> Processing eval for path %s with properties: %s\n" (Path.to_string path) (Properties.to_string properties) in
          let name = Properties.get_or_default "name" ~default:"Zenoh OCaml!" properties in

          let%lwt name' = if Astring.is_prefix ~affix:"/" name then
            (
              let%lwt () = Lwt_io.printf "   >> Get name to use from zenoh at path: %s\n" name in
              match%lwt Workspace.get ~/*name w with
              | (_, value)::_ -> Lwt.return (Value.to_string value)
              | [] -> Lwt.return name
            )
            else Lwt.return name
          in

          let%lwt () = Lwt_io.printf "   >> Returning string: \"Eval from %s\"\n" name' in
          Lwt.return ~$("Eval from "^name')
        end
      end
      w
    in
    
    let%lwt () = Lwt_io.printf "Enter 'q' to quit...\n%!" in
    let rec loop () = match%lwt Lwt_io.read_char Lwt_io.stdin with
      | 'q' -> Lwt.return_unit
      | _ -> loop()
    in
    let%lwt () = loop () in

    let%lwt () = Workspace.unregister_eval ~//p w in
    Zenoh.logout y
  )
