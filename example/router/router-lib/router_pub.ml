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

let run tcpport peers strength usersfile plugins bufn timestamp = 
  Lwt_main.run (
    let _ = Zrouter.run tcpport peers strength usersfile plugins Zplugins.PluginsArgs.empty bufn timestamp in
    let%lwt z = zopen "" in 
    let%lwt pub = publish z "/home2" in 
    let rec publish i = 
      let buf = Abuf.create 1024 in
      encode_string ("HOME2_MSG" ^ Stdlib.string_of_int i) buf;
      let%lwt _ = stream pub buf in
      let%lwt _ = Lwt_unix.sleep 1.0 in
      publish (i + 1)
    in
    publish 0
  )

let () = 
  let open Cmdliner in
  let _ = Term.(eval (const run $ 
                      Zrouter.tcpport $ 
                      Zrouter.peers $ 
                      Zrouter.strength $ 
                      Zrouter.users $
                      Zrouter.plugins $ 
                      Zrouter.bufn $
                      Zrouter.timestamp, Term.info "router_pub")) 
  in ()
