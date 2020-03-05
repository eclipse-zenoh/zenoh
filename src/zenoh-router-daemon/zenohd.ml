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
open Cmdliner

let scout_port = 7447 
let scout_mcast_addr = Unix.inet_addr_of_string "239.255.0.1"
let scout_addr = Unix.ADDR_INET (scout_mcast_addr, scout_port)
let max_scout_msg_size = 1024


let reporter ppf =
  let report _ level ~over k msgf =
    let k _ = over (); k () in
    let with_stamp h _ k ppf fmt =
      Format.kfprintf k ppf ("[%f]%a @[" ^^ fmt ^^ "@]@.")
        (Unix.gettimeofday ()) Logs.pp_header (level, h)
    in
    msgf @@ fun ?header ?tags fmt -> with_stamp header tags k ppf fmt
  in
  { Logs.report = report }

let setup_log style_renderer level =
  Fmt_tty.setup_std_outputs ?style_renderer ();
  Logs.set_level level;
  Logs.set_reporter (reporter (Format.std_formatter));
  ()

let make_hello_buf locator = 
  let open Message in 
  let open Locator in 
  let open Mcodec in 
  let hello = Hello.create ScoutFlags.scoutBroker (Locators.singleton locator) [] in 
  let buf = Bytes.create max_scout_msg_size in 
  let abuf = Abuf.from_bytes buf in  
  let _ = Logs.debug (fun m -> m "Buffer Capacity %d" (Abuf.capacity abuf)) in 
  Abuf.clear abuf; 
  encode_hello hello abuf ;
  (buf, 0, Abuf.w_pos abuf)

let handle_msg socket msg dest hello_buf =  
  let open Message in 
  let open Lwt.Infix in 
  match msg with         
  | Scout scout  
      when (Int64.logand (Scout.mask scout) ScoutFlags.scoutBroker)!= Int64.of_int 0 ->     
    let (buf, offset, len) = hello_buf in 
    Lwt_bytes.sendto socket (Lwt_bytes.of_bytes buf) offset len [] dest >>= fun _ -> Lwt.return_unit
                    
  | Scout _ -> Lwt.return_unit               
  | _ -> let _ = Logs.warn (fun m -> m "Received unexpected Message on scouting locator") in Lwt.return_unit


let rec scout_loop socket hello_buf = 
  let open Lwt.Infix in 
  let buf = Bytes.create max_scout_msg_size in 
  let abuf = Abuf.from_bytes buf in 
  Abuf.clear abuf ;
  let _ = Logs.debug (fun m -> m "Scouting..." ) in
  let%lwt (n, src) = Lwt_unix.recvfrom socket buf 0 max_scout_msg_size  []  in 
  let _ = Logs.debug (fun m -> m "Received Message %d bytes" n ) in
  (match n with 
  | 0 ->  Lwt.return_unit
  | _ ->     
    let () = Abuf.set_r_pos 0 abuf in
    let () = Abuf.set_w_pos n abuf in
    (Lwt.try_bind 
      (fun () -> Lwt.return @@ Mcodec.decode_msg abuf) 
      (fun msg -> handle_msg socket msg src hello_buf)
      (fun _ -> Lwt.return_unit))) >>= fun _ -> scout_loop socket hello_buf


let run_scouting iface locator = 
  try
    let socket = Lwt_unix.socket Unix.PF_INET Unix.SOCK_DGRAM 0 in 
    Lwt_unix.setsockopt socket Unix.SO_REUSEADDR true;
    let saddr = Unix.ADDR_INET (Unix.inet_addr_any, scout_port) in 
    let%lwt () = Lwt_unix.bind socket saddr in 
    let _ = Logs.info (fun m -> m "Joining MCast group") in  
    let  () = Lwt_unix.mcast_add_membership socket ~ifname:iface scout_mcast_addr in 
    let hello_buf = make_hello_buf locator in 
    scout_loop socket hello_buf
  with e ->
    Logs.warn (fun m -> m "Problem running scouting : %s" (Printexc.to_string e));
    Lwt.return_unit
      
let auto_select_iface () =   
  let ifs = Aunix.inet_addrs_up_nolo () in 
  List.iteri (fun i addr -> Printf.printf "%d - %s\n" i (Unix.string_of_inet_addr addr)) ifs ;  
  Unix.string_of_inet_addr  
  @@ match ifs with 
  | [] -> Unix.inet_addr_loopback
          (* If no interface are active we can only be reached through loopback *)
  | h::_ -> h
  
let run_disco disco = 
  let addr = match disco with 
    | "auto" ->  auto_select_iface ()     
    | _ -> disco in 
  let open Locator in 
  match Locator.of_string ("tcp/" ^ addr ^ ":" ^ (string_of_int(scout_port))) with
  | Some locator ->       
    let _ = Logs.info (fun m -> m "Running scouting on interface %s\n" disco) in 
    let iface = Unix.inet_addr_of_string addr in         
    run_scouting iface locator
  | _ -> let _ = Logs.warn (fun m -> m "Invalid scouting interface %s" disco) in Lwt.return_unit
  

let run plugins_args plugins_opt_ignored tcpport peers strength usersfile plugins bufn timestamp style_renderer level disco =
  ignore plugins_opt_ignored;
  setup_log style_renderer level;
  Lwt_main.run @@ Lwt.join [Zrouter.run tcpport peers strength usersfile plugins plugins_args bufn timestamp; run_disco disco]

let extract_plugins_args () =
  let open Zplugins in
  let regex = Str.regexp "^--\\(.*\\)\\.\\(.*\\)$" in
  Array.fold_left (fun (plugin_args, argv) arg ->
    if (Str.string_match regex arg 0) then (
      let plugin = Str.matched_group 1 arg in
      let parg = "--"^(Str.matched_group 2 arg) in
      let plugin_args' = match PluginsArgs.find_opt plugin plugin_args with
        | None -> PluginsArgs.add plugin [parg] plugin_args
        | Some pargs -> PluginsArgs.add plugin (parg::pargs) plugin_args
      in
      (plugin_args',  argv)
    )
    else (plugin_args,  argv@[arg])
  ) (PluginsArgs.empty, []) Sys.argv

let () =
  Printexc.record_backtrace true;  
  (* Lwt_engine.set (new Lwt_engine.libev ()); *)
  let env = Arg.env_var "ZENOD_VERBOSITY" in
  (* NOTE: the plugin_opt below is declared only for the description in --help.
     the effective plugin options are extracted from argv (in plugin_args) before passing to Term.eval,
     since Cmdliner cannot understand such pattern of options.
  *)
  let plugins_opt = Arg.(value & opt string "" & info ["<plugin_name>.<plugin_option>"] ~docv:"<option_value>"
    ~doc:"Pass to the plugin with name '<plugin_name>' the option --<plugin_option>=<option_value>.
          Example of usage: --zenoh-storages.storage=/demo/example/**  --zenoh-http.httpport=8080")
  in
  let (plugins_args, argv) = extract_plugins_args () in
  let _ = Term.(eval ~argv:(Array.of_list argv) (const (run plugins_args) $ plugins_opt $ Zrouter.tcpport $ Zrouter.peers $ Zrouter.strength $ Zrouter.users $ Zrouter.plugins $ Zrouter.bufn $ Zrouter.timestamp $ Fmt_cli.style_renderer () $ Logs_cli.level~env () $ Zrouter.disco, Term.info "zenohd")) in  ()

