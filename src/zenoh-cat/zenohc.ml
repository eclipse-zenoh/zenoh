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
open Lwt
open Lwt.Infix
open Zenoh_proto
open Apero

(* let () =
  (Lwt_log.append_rule "*" Lwt_log.Debug) *)

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


(* Command line interface *)

open Cmdliner

let dbuf = Abuf.create_bigstring 1024

let pid  = 
  Abuf.create_bigstring 32 |> fun buf -> 
  Abuf.write_bytes (Bytes.unsafe_of_string ((Uuidm.to_bytes @@ Uuidm.v5 (Uuidm.create `V4) (string_of_int @@ Unix.getpid ())))) buf; 
  buf

let lease = 0L
let version = Char.chr 0x01


let default_conduit = ZConduit.make 0

module Command = struct
  type t = Cmd of string | CmdIArgs of string * int list | CmdSArgs of string * string list | NoCmd

  let of_string s =
    match (String.split_on_char ' ' s |> List.filter (fun x -> x != "") |> List.map (fun s -> String.trim s)) with
    | [] ->  NoCmd
    | [a] -> Cmd a
    | h::tl when h = "dsub" -> CmdSArgs (h, tl)
    | h::tl when h = "pub" -> CmdSArgs (h, tl)
    | h::tl when h = "pubn" -> CmdSArgs (h, tl)
    | h::tl when h = "write" -> CmdSArgs (h, tl)
    | h::tl when h = "writen" -> CmdSArgs (h, tl)
    | h::tl when h = "dres" -> CmdSArgs (h, tl)
    | h::tl when h = "query" -> CmdSArgs (h, tl)
    | a::tl  -> CmdIArgs (a, tl |> (List.map (int_of_string)))

end

let lbuf = Abuf.create_bigstring 16
let wbuf = Abuf.create_bigstring 65537
let rbuf = Abuf.create_bigstring 65537

let get_args () =
  if Array.length Sys.argv < 3 then
    begin
      let ipaddr = "127.0.0.1" in
      let port = 7447 in
      print_endline (Printf.sprintf "[Connecting to broker at %s:%d -- to use other address run as: zenohc <ipaddr> <port>]" ipaddr port)
    ; (ipaddr, port)
    end
  else (Array.get Sys.argv 1, int_of_string @@ Array.get Sys.argv 2)

let send_message sock (msg: Message.t) =
  Abuf.clear wbuf;
  Abuf.clear lbuf;
  Mcodec.encode_msg msg wbuf;
  let len = Abuf.readable_bytes wbuf in
  fast_encode_vle (Vle.of_int len) lbuf;
  let%lwt _ = Net.write_all sock (Abuf.wrap [lbuf; wbuf]) in
  Lwt.return 1
  

let send_scout sock =
  let%lwt _ = Logs_lwt.debug (fun m -> m "send_scout\n") in
  let msg = Message.Scout (Scout.create (Vle.of_int 1) []) in send_message sock msg

let send_open  sock =
  let%lwt _ = Logs_lwt.debug (fun m -> m "send_open\n") in 
  let msg = Message.Open (Open.create version pid lease Locators.empty [])
  in send_message sock msg

let send_close sock =
  let%lwt _ = Logs_lwt.debug (fun m -> m "send_close\n") in
  let msg = Message.Close (Close.create pid (Char.chr 1)) in send_message sock msg

let send_declare_res sock id uri = 
  let res_id = id in
  let decls = Declarations.singleton @@ ResourceDecl (ResourceDecl.create res_id uri [])  in
  let msg = Message.Declare (Declare.create (true, true) (ZConduit.next_rsn default_conduit) decls)
  in send_message sock msg

let send_declare_pub sock id =
  let pub_id = Vle.of_int id in
  let decls = Declarations.singleton @@ PublisherDecl (PublisherDecl.create pub_id [])  in
  let msg = Message.Declare (Declare.create (true, true) (ZConduit.next_rsn default_conduit) decls)
  in send_message sock msg

let send_declare_sub sock id mode =
  let sub_id = Vle.of_int id in
  let mode = if mode = "pull" then SubscriptionMode.pull_mode else SubscriptionMode.push_mode in
  let decls = Declarations.singleton @@ SubscriberDecl (SubscriberDecl.create sub_id mode [])  in
  let msg = Message.Declare (Declare.create (true, true) (ZConduit.next_rsn default_conduit) decls)
  in send_message sock msg

let send_declare_sto sock id =
  let res_id = Vle.of_int id in
  let decls = Declarations.singleton @@ StorageDecl (StorageDecl.create res_id [])  in
  let msg = Message.Declare (Declare.create (true, true) (ZConduit.next_rsn default_conduit) decls)
  in send_message sock msg

let send_compact_data sock rid data =
  let sn = ZConduit.next_usn default_conduit in 
  Abuf.clear dbuf;
  encode_string data dbuf;
  let msg = Message.CompactData (CompactData.create (false,false) sn rid None (Payload.create dbuf)) in
  send_message sock msg
  
let rec send_compact_data_n sock rid n p data = 
  let%lwt _ = Logs_lwt.debug (fun m -> m "[Sending periodic sample...]") in
  if n = 0 then Lwt.return 0
  else 
    begin
      let%lwt _ = (send_compact_data sock rid data) in
      let%lwt _ =  Lwt_unix.sleep p in
      send_compact_data_n sock rid (n-1) p data
    end

let send_write_data sock rname data =
  let sn = ZConduit.next_usn default_conduit in 
  Abuf.clear dbuf;
  encode_string data dbuf;
  let msg = Message.WriteData (WriteData.create (false,false) sn rname (Payload.create dbuf)) in
  send_message sock msg
  
let rec send_write_data_n sock rname n p data = 
  let%lwt _ = Logs_lwt.debug (fun m -> m "[Sending periodic sample...]") in
  if n = 0 then Lwt.return 0
  else 
    begin
      let%lwt _ = (send_write_data sock rname data) in
      let%lwt _ =  Lwt_unix.sleep p in
      send_write_data_n sock rname (n-1) p data
    end

let send_query sock qid rname predicate =
  let msg = Message.Query (Query.create pid qid rname predicate []) in
  send_message sock msg

let send_pull sock rid =
  let sn = ZConduit.next_usn default_conduit in   
  let msg = Message.Pull (Pull.create (true, false) sn (Vle.of_int rid) None) in
  send_message sock msg

let produce_message sock cmd =
  match cmd with
  | Command.Cmd msg -> (
    match msg with
    | "scout" -> send_scout sock
    | "close" -> send_close sock
    | "open" -> send_open sock
    | _ ->
      let%lwt _ = Lwt_io.printf "[Error: The message <%s> is unkown]\n" msg in
      return 0)

  | Command.CmdIArgs (msg, xs) -> (
      match msg with
      | "dpub" -> send_declare_pub sock (List.hd xs)
      | "dsto" -> send_declare_sto sock (List.hd xs)
      | "pull" -> send_pull sock (List.hd xs)
      | _ ->
        let%lwt _ = Lwt_io.printf "[Error: The message <%s> is unkown]\n" msg in
        return 0)

  | Command.CmdSArgs (msg, xs) -> (
      match msg with
      | "dsub" ->
        let id = int_of_string (List.hd xs) in
        let mode = match xs with 
        | _::s::_ -> s
        | _ -> "" in
        send_declare_sub sock id mode
      | "pub" ->
        let rid = Vle.of_string (List.hd xs) in
        let data = List.hd @@ List.tl @@ xs in
        send_compact_data sock rid data
      | "pubn" ->
        let%lwt _ = Logs_lwt.debug (fun m -> m "pubn....") in
        let rid = Vle.of_string (List.hd xs) in
        let n = int_of_string (List.nth xs 1) in
        let p = float_of_string (List.nth xs 2) in
        let data = (List.nth xs 3) in
        let%lwt _ = Logs_lwt.debug (fun m -> m "Staring pub loop with %d %f %s" n p data) in
        send_compact_data_n sock rid n p data
      | "write" ->
        let rname = List.hd xs in
        let data = List.hd @@ List.tl @@ xs in
        send_write_data sock rname data
      | "writen" ->
        let%lwt _ = Logs_lwt.debug (fun m -> m "writen....") in
        let rname = List.hd xs in
        let n = int_of_string (List.nth xs 1) in
        let p = float_of_string (List.nth xs 2) in
        let data = (List.nth xs 3) in
        let%lwt _ = Logs_lwt.debug (fun m -> m "Staring write loop with %d %f %s" n p data) in
        send_write_data_n sock rname n p data
      | "dres" ->
        let%lwt _ = Logs_lwt.debug (fun m -> m "dres....") in
        let rid = Vle.of_string (List.hd xs) in
        let uri = List.nth xs 1 in
        send_declare_res sock rid uri
      | "query" ->
        let%lwt _ = Logs_lwt.debug (fun m -> m "query....") in
        let qid = Vle.of_string (List.hd xs) in
        let rname = List.nth xs 1 in
        let predicate = List.nth xs 2 in
        send_query sock qid rname predicate 
      | _ ->
        let%lwt _ = Lwt_io.printf "[Error: The message <%s> is unkown]\n" msg in
        return 0)

  | Command.NoCmd -> return 0


let rec run_encode_loop sock =
  let%lwt _ = Logs_lwt.debug (fun m -> m "[Starting run_encode_loop]\n") in 
  let%lwt _ = Lwt_io.printf ">> "  in
  let%lwt msg = Lwt_io.read_line Lwt_io.stdin in
  let%lwt _ = produce_message sock (Command.of_string msg) in
  run_encode_loop sock


let process_incoming_message = function
  | Message.CompactData dmsg ->
    let rid = CompactData.id dmsg in
    let buf = CompactData.payload dmsg |> Payload.data in
    let data = decode_string buf in
    Logs.info (fun m -> m "\n[received compact data rid: %Ld payload: %s]\n>>" rid data);
    return_true
  | Message.WriteData dmsg ->
    let res = WriteData.resource dmsg in
    let buf = WriteData.payload dmsg |> Payload.data in
    let data = decode_string buf in
    Logs.info (fun m -> m "\n[received write data res: %s payload: %s]\n>>" res data);
    return_true
  | Message.StreamData dmsg ->
    let res = StreamData.id dmsg in
    let buf = StreamData.payload dmsg |> Payload.data in
    let data = decode_string buf in
    Logs.info (fun m -> m "\n[received stream data res: %Ld payload: %s]\n>>" res data);
    return_true
  | msg ->
      Logs.debug (fun m -> m "\n[received: %s]\n>> " (Message.to_string msg));  
      return_true


let rec run_decode_loop sock =  
  try%lwt
    let%lwt _ = Logs_lwt.debug (fun m -> m "[Starting run_decode_loop]\n") in 
    let%lwt len = Net.read_vle sock >>= fun v -> Vle.to_int v |> Lwt.return in
    let%lwt _ = Logs_lwt.debug (fun m -> m ">>> Received message of %d bytes" len) in
    Abuf.clear rbuf; 
    let%lwt _ = Net.read_all sock rbuf len in
    let%lwt _ =  Logs_lwt.debug (fun m -> m "tx-received: %s "  (Abuf.to_string rbuf)) in
    let msg = Mcodec.decode_msg rbuf in
    let%lwt _ = process_incoming_message msg in
    run_decode_loop sock
  with
  | _ ->
    let%lwt _ = Logs_lwt.debug (fun m -> m "Connection close by peer") in
    try%lwt
      let%lwt _ = Lwt_unix.close sock in
      fail @@ Exception (`ClosedSession (`Msg "Connection  closed by peer"))
    with
    | _ ->  
      let%lwt _ = Logs_lwt.debug (fun m -> m "[Session Closed]\n") in
      fail @@ Exception (`ClosedSession `NoMsg)
    
let peer = Arg.(value & opt string "tcp/127.0.0.1:7447" & info ["p"; "peer"] ~docv:"PEER" ~doc:"peer")

let to_string peers = 
  peers
  |> List.map (fun p -> Locator.to_string p) 
  |> String.concat "," 

let run peer style_renderer level = 
  setup_log style_renderer level; 
  let open Lwt_unix in
  let sock = socket PF_INET SOCK_STREAM 0 in
  let _ = setsockopt sock SO_REUSEADDR true in
  let _ = setsockopt sock TCP_NODELAY true in
  let saddr = Scanf.sscanf peer "%[^/]/%[^:]:%d" (fun _ ip port -> 
    ADDR_INET (Unix.inet_addr_of_string ip, port)) in
  let name_info = Unix.getnameinfo saddr [NI_NUMERICHOST; NI_NUMERICSERV] in
  let _ = Logs_lwt.debug (fun m -> m "peer : tcp/%s:%s" name_info.ni_hostname name_info.ni_service) in
  let _ = connect sock  saddr in
  Lwt_main.run @@ Lwt.join [run_encode_loop sock; run_decode_loop sock >>= fun _ -> return_unit]

let () =
  Printexc.record_backtrace true;
  let env = Arg.env_var "ZENOD_VERBOSITY" in
  let _ = Term.(eval (const run $ peer $ Fmt_cli.style_renderer () $ Logs_cli.level ~env (), Term.info "client")) in  ()
  
