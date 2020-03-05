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
open Ztypes
open Message
open Locator
open Lwt

module VleMap = Map.Make(Vle)

type sublistener = string -> (Abuf.t * data_info) list -> unit Lwt.t
type queryreply = 
  | StorageData of {stoid:Abuf.t; rsn:int; resname:string; data:Abuf.t; info:data_info}
  | StorageFinal of {stoid:Abuf.t; rsn:int}
  | EvalData of {stoid:Abuf.t; rsn:int; resname:string; data:Abuf.t; info:data_info}
  | EvalFinal of {stoid:Abuf.t; rsn:int}
  | ReplyFinal 
type reply_handler = queryreply -> unit Lwt.t
type query_handler = string -> string -> (string * Abuf.t * data_info) list Lwt.t

type insub = {subid:int; resid:Vle.t; listener:sublistener}

type insto = {stoid:int; resname:PathExpr.t; listener:sublistener; qhandler:query_handler}

type ineval = {evalid:int; resname:PathExpr.t; qhandler:query_handler}

type resource = {rid: Vle.t; name: PathExpr.t; matches: Vle.t list; subs: insub list; stos : insto list; evals : ineval list;}

type query = {qid: Vle.t; listener:reply_handler;}

let scout_port = 7447 
let mcast_scout_addr = Unix.ADDR_INET (Unix.inet_addr_of_string "239.255.0.1", scout_port)
let max_scout_msg_size = 1024
let local_scout_period = 0.3
let local_scout_tries = 2

let with_match res mrid = 
  {res with matches = mrid :: List.filter (fun r -> r != mrid) res.matches}

(* let remove_match res mrid = 
  {res with matches = List.filter (fun r -> r != mrid) res.matches} *)

let with_sub sub res = 
  {res with subs = sub :: List.filter (fun s -> s != sub) res.subs}

let remove_sub subid res = 
  {res with subs = List.filter (fun s -> s.subid != subid) res.subs}

let with_sto sto res = 
  {res with stos = sto :: List.filter (fun s -> s != sto) res.stos}

let remove_sto stoid res = 
  {res with stos = List.filter (fun s -> s.stoid != stoid) res.stos}

let with_eval eval res = 
  {res with evals = eval :: List.filter (fun e -> e != eval) res.evals}

let remove_eval evalid res = 
  {res with evals = List.filter (fun e -> e.evalid != evalid) res.evals}
  

type state = {
  next_sn : Vle.t;
  next_pubsub_id : int;
  next_res_id : Vle.t;
  next_qry_id : Vle.t;
  resmap : resource VleMap.t;
  qrymap : query VleMap.t;
}

let create_state = {next_sn=1L; next_pubsub_id=0; next_res_id=0L; next_qry_id=0L; resmap=VleMap.empty; qrymap=VleMap.empty}

let get_next_sn state = (state.next_sn, {state with next_sn=Vle.add state.next_sn 1L})
let get_next_entity_id state = (state.next_pubsub_id, {state with next_pubsub_id=(state.next_pubsub_id + 1)})
let get_next_res_id state = (state.next_res_id, {state with next_res_id=Vle.add state.next_res_id 1L})
let get_next_qry_id state = (state.next_qry_id, {state with next_qry_id=Vle.add state.next_qry_id 1L})

type session = | Sock of Lwt_unix.file_descr | Stream of (Frame.Frame.t Lwt_stream.t * Frame.Frame.t Lwt_stream.bounded_push)

let lbuf = Abuf.create_bigstring 16
let wbuf = Abuf.create_bigstring ~grow:8192 (64*1024)
let rbuf = Abuf.create_bigstring ~grow:8192 (64*1024)

let send_message sock msg =
  match sock with 
  | Sock sock -> 
    Abuf.clear wbuf;
    Abuf.clear lbuf;
    Mcodec.encode_msg msg wbuf;
    let len = Abuf.readable_bytes wbuf in
    fast_encode_vle (Vle.of_int len) lbuf;
    Net.write_all sock (Abuf.wrap [lbuf; wbuf]) 
    >>= fun _ -> Lwt.return_unit
  | Stream (_, push) -> 
    push#push @@ Frame.Frame.create [msg]

let read_messages = function
  | Sock sock -> 
    let%lwt len = Net.read_vle sock >>= fun v -> Vle.to_int v |> Lwt.return in
    let%lwt _ = Logs_lwt.debug (fun m -> m ">>> Received message of %d bytes" len) in
    Abuf.clear rbuf;
    let%lwt _ = Net.read_all sock rbuf len in
    let%lwt _ =  Logs_lwt.debug (fun m -> m "tx-received: %s "  (Abuf.to_string rbuf)) in
    let rec decode_msgs buf msgs = 
      match Abuf.readable_bytes buf with 
      | 0 -> msgs
      | _ -> try 
                let msg = Mcodec.decode_msg buf in
                decode_msgs buf (msg::msgs)
             with e -> match msgs with
             | [] -> raise e
             | msgs ->
                Logs.err (fun m -> m "Exception decoding message: %s " (Printexc.to_string e));
                msgs
    in
    Lwt.return @@ List.rev @@ decode_msgs rbuf []
  | Stream (stream, _) -> 
    Lwt_stream.get stream >>= (Option.get %> Lwt.return) >>= (Frame.Frame.to_list %> Lwt.return)

let close = function
  | Sock sock -> Lwt_unix.close sock 
  | Stream _ -> Lwt.return_unit

let copy buf = 
  Abuf.get_buf ~at:(Abuf.r_pos buf) (Abuf.readable_bytes buf) buf

type t = {
  sock : session;
  peer_pid : string option;
  state : state Guard.t;
}

type sub = {z:t; id:int; resid:Vle.t;}
type pub = {z:t; id:int; resid:Vle.t; reliable:bool}
type storage = {z:t; id:int; resid:Vle.t;}
type eval = {z:t; id:int; resid:Vle.t;}

type submode = SubscriptionMode.t


let pid  = 
  Abuf.create_bigstring 32 |> fun buf -> 
  Abuf.write_bytes (Bytes.unsafe_of_string ((Uuidm.to_bytes @@ Uuidm.v5 (Uuidm.create `V4) (string_of_int @@ Unix.getpid ())))) buf; 
  buf

let lease = 0L
let version = Char.chr 0x01

let clean_query z qryid = 
  let%lwt state = Guard.acquire z.state in
  let qrymap = VleMap.remove qryid state.qrymap in 
  let state = {state with qrymap} in
  Lwt.return @@ Guard.release z.state state

let match_resource rmap mres = 
  VleMap.fold (fun _ res x -> 
    let (rmap, mres) = x in
    match PathExpr.intersect mres.name res.name with 
    | true -> 
      let mres = with_match mres res.rid in 
      let rmap = VleMap.add res.rid (with_match res mres.rid) rmap in 
      (rmap, mres)
    | false -> x) rmap (rmap, mres)

let add_resource resname state = 
  let (rid, state) = get_next_res_id state in 
  let res =  {rid; name=resname; matches=[rid]; subs=[]; stos=[]; evals=[]} in
  let (resmap, res) = match_resource state.resmap res in
  let resmap = VleMap.add rid res resmap in 
  (res, {state with resmap})


(* let make_hello = Message.Hello (Hello.create (Vle.of_char ScoutFlags.scoutBroker) Locators.empty []) *)
let make_open username password = 
  let props = [] in 
  let props = match username with  
  | Some name -> ZProperty.User.make name :: props
  | None -> props in 
  let props = match password with  
  | Some word -> ZProperty.Password.make word :: props
  | None -> props in 
  Message.Open (Open.create version pid lease Locators.empty props)
(* let make_accept opid = Message.Accept (Accept.create opid pid lease []) *)

let invoke_listeners res resname payloads = 
  let%lwt _ = Lwt_list.iter_s (fun (sub:insub) -> 
    Lwt.catch (fun () -> sub.listener resname (List.map (fun p -> (copy (Payload.data p)), (Payload.header p)) payloads)) 
              (fun e -> Logs_lwt.info (fun m -> m "Subscriber listener raised exception %s : \n%s" (Printexc.to_string e) (Printexc.get_backtrace ())))
    |> Lwt.ignore_result; Lwt.return_unit
  ) res.subs in
  Lwt_list.iter_s (fun (sto:insto) -> 
    Lwt.catch (fun () -> sto.listener resname (List.map (fun p -> (copy (Payload.data p)), (Payload.header p)) payloads)) 
              (fun e -> Logs_lwt.info (fun m -> m "Storage listener raised exception %s : \n%s" (Printexc.to_string e) (Printexc.get_backtrace ())))
    |> Lwt.ignore_result; Lwt.return_unit
  ) res.stos

let process_stream_data z rid payloads =
  let state = Guard.get z.state in
  let%lwt _ = match VleMap.find_opt rid state.resmap with
  | Some res -> 
    Lwt_list.iter_s (fun resid -> 
      match VleMap.find_opt resid state.resmap with
      | Some matching_res -> invoke_listeners matching_res (PathExpr.to_string res.name) payloads
      | None -> Lwt.return_unit 
    ) res.matches
  | None -> Lwt.return_unit in
  return_true

let process_write_data z resname payloads = 
  let state = Guard.get z.state in
  let%lwt _ = Lwt_list.iter_s (fun (_, res) -> 
    match PathExpr.intersect res.name (PathExpr.of_string resname) with 
    | true -> invoke_listeners res resname payloads
    | false -> return_unit) (VleMap.bindings state.resmap) in
    return_true

let process_query z resname predicate process_replies = 
  let state = Guard.get z.state in
  let path = PathExpr.of_string resname in
  VleMap.bindings state.resmap 
  |> List.filter (fun (_, res) -> PathExpr.intersect res.name path)
  |> Lwt_list.iter_s (fun (_, res) -> 
        let%lwt _ = res.evals |> Lwt_list.iter_s (fun (eval:ineval) -> 
          Lwt.catch(fun () -> eval.qhandler resname predicate) 
                    (fun e -> Logs_lwt.info (fun m -> m "Eval query handler raised exception %s" (Printexc.to_string e)) >>= fun () -> Lwt.return [])
                    (* TODO propagate query failures *)
          >>= process_replies Reply.Eval
        ) in
        res.stos |> Lwt_list.iter_s (fun (sto:insto) -> 
          Lwt.catch(fun () -> sto.qhandler resname predicate) 
                    (fun e -> Logs_lwt.info (fun m -> m "Storage query handler raised exception %s" (Printexc.to_string e)) >>= fun () -> Lwt.return [])
                    (* TODO propagate query failures *)
          >>= process_replies Reply.Storage
        )
      )

let process_incoming_message msg resolver t = 
  let open Lwt.Infix in
  match msg with
  | Message.Accept amsg -> Lwt.wakeup_later resolver {t with peer_pid=Some (Abuf.hexdump @@ Message.Accept.apid amsg)}; return_true
  | Message.BatchedStreamData dmsg -> process_stream_data t (BatchedStreamData.id dmsg) (BatchedStreamData.payload dmsg)
  | Message.StreamData dmsg -> process_stream_data t (StreamData.id dmsg) [StreamData.payload dmsg]
  | Message.CompactData dmsg -> process_stream_data t (CompactData.id dmsg) [CompactData.payload dmsg]
  | Message.WriteData dmsg -> process_write_data t (WriteData.resource dmsg) [WriteData.payload dmsg]
  | Message.Query qmsg -> 
      (process_query t (Query.resource qmsg) (Query.predicate qmsg) (fun source replies -> 
        replies |> Lwt_list.iteri_s (fun rsn (resname, data, ctx) -> 
          let rep_value = (Some (pid, Vle.of_int rsn, resname, Payload.create ~header:ctx data)) in
          send_message t.sock (Message.Reply(Reply.create (Query.pid qmsg) (Query.qid qmsg) source rep_value)))
        >>= fun () -> 
          let rep_value = (Some (pid, Vle.of_int (List.length replies), "", Payload.from_buffer true (Abuf.create 0))) in
          send_message t.sock (Message.Reply(Reply.create (Query.pid qmsg) (Query.qid qmsg) source rep_value)))
      >>= fun () -> send_message t.sock (Message.Reply(Reply.create (Query.pid qmsg) (Query.qid qmsg) Storage None)))
      |> Lwt.ignore_result; Lwt.return_true
  | Message.Reply rmsg -> 
    (match String.equal (Abuf.hexdump (Reply.qpid rmsg)) (Abuf.hexdump pid) with 
    | false -> return_true
    | true -> 
      (* Don't use Guard.get. We want to make sure the request has been registered. *)
      let%lwt state = Guard.acquire t.state in Guard.release t.state state;
      match VleMap.find_opt (Reply.qid rmsg) state.qrymap with 
      | None -> return_true 
      | Some query -> 
        (match (Message.Reply.value rmsg) with 
        | None -> Lwt.catch(fun () -> query.listener ReplyFinal) 
                           (fun e -> Logs_lwt.info (fun m -> m "Reply handler raised exception %s" (Printexc.to_string e)))
                  |> Lwt.ignore_result;
                  clean_query t (Reply.qid rmsg)
        | Some (stoid, rsn, resname, payload) -> 
          let payload_buffer = Payload.buffer payload in
          (match (Reply.source rmsg, Abuf.readable_bytes payload_buffer) with 
          | Storage, 0 -> Lwt.catch(fun () -> query.listener (StorageFinal({stoid; rsn=(Vle.to_int rsn)})))
                            (fun e -> Logs_lwt.info (fun m -> m "Reply handler raised exception %s" (Printexc.to_string e)))
                          |> Lwt.ignore_result; Lwt.return_unit
          | Eval, 0 ->  Lwt.catch(fun () -> query.listener (EvalFinal({stoid; rsn=(Vle.to_int rsn)})))
                          (fun e -> Logs_lwt.info (fun m -> m "Reply handler raised exception %s" (Printexc.to_string e)))
                        |> Lwt.ignore_result; Lwt.return_unit
          | Storage, _ -> Lwt.catch(fun () -> query.listener (StorageData({stoid; rsn=(Vle.to_int rsn); resname; data=Payload.data payload; info=Payload.header payload})))
                            (fun e -> Logs_lwt.info (fun m -> m "Reply handler raised exception %s" (Printexc.to_string e)))
                          |> Lwt.ignore_result; Lwt.return_unit
          | Eval, _ ->  Lwt.catch(fun () -> query.listener (EvalData({stoid; rsn=(Vle.to_int rsn); resname; data=Payload.data payload; info=Payload.header payload})))
                          (fun e -> Logs_lwt.info (fun m -> m "Reply handler raised exception %s" (Printexc.to_string e)))
                        |> Lwt.ignore_result; Lwt.return_unit
          )) >>= fun _ -> return_true )
  | msg ->
    Logs.debug (fun m -> m "\n[received: %s]\n>> " (Message.to_string msg));  
    return_true

let rec run_decode_loop resolver t = 
  let%lwt msgs = read_messages t.sock in
  let%lwt () = Lwt_list.iter_s (fun msg -> 
    process_incoming_message msg resolver t >>= fun _ -> Lwt.return_unit) msgs in
  run_decode_loop resolver t
  
let safe_run_decode_loop resolver t =  
  try%lwt
    run_decode_loop resolver t
  with
  | x ->
    let%lwt _ = Logs_lwt.warn (fun m -> m "Exception in decode loop : %s\n%s" (Printexc.to_string x) (Printexc.get_backtrace ()) ) in
    try%lwt
      let%lwt _ = close t.sock in
      fail @@ Exception (`ClosedSession (`Msg (Printexc.to_string x)))
    with
    | _ -> 
      fail @@ Exception (`ClosedSession (`Msg (Printexc.to_string x)))

let handle_scout_data ahbuf locators n tries nb =       
      let _ = Logs.debug (fun m -> m "Scouting loop -- received %d bytes\n" nb)  in 
      Abuf.set_w_pos nb ahbuf ; 
      let msg = Mcodec.decode_msg ahbuf in 
      let ls = match msg with 
      | Message.Hello hello ->  
        let _ = Logs_lwt.debug (fun m -> m "received hello message\n")  in 
        Hello.locators hello        
      | _ -> 
        let _ = Logs_lwt.debug (fun m -> m "received unexpected  message\n") 
        in Locators.empty
      in     
      let locs = List.append (Locators.to_list ls)  locators in 
      match locs with 
      | [] when n < tries -> Lwt.return @@ `ContinueScouting locs 
      | _ -> Lwt.return @@ `DoneScouting locs

let scouting socket sbuf_ctx tries scout_addr scout_period = 
  let hbuf = Bytes.create max_scout_msg_size in 
  let ahbuf = Abuf.from_bytes hbuf in     
  let rec scout_loop n locators =    
    if n = 0 then Lwt.return locators
    else 
      begin 
        let (sbuf, offset, len) = sbuf_ctx in 
      let _ = Logs_lwt.debug (fun m -> m "Scouting loop...\n")  in 
      let  _ = Lwt_unix.sendto socket sbuf offset len [] scout_addr in 
    
      let handle_timeout _ =
        let _ = Logs.debug (fun m -> m "Scouting timed-out\n") in  
        Lwt.return @@ `ContinueScouting locators
      in 
      match%lwt 
        (Lwt.try_bind 
          (fun () -> (Lwt_unix.with_timeout scout_period (fun () -> Lwt_unix.recv socket hbuf 0 max_scout_msg_size [])))
          (handle_scout_data ahbuf locators n tries)
          handle_timeout) with 
      | `ContinueScouting locs -> scout_loop (n-1) locs
      | `DoneScouting locs -> Lwt.return locs
    end
  in scout_loop tries [] 

let select_iface iface =   
  match iface with 
  | "auto" ->
    (let ifs = Aunix.inet_addrs_up_nolo () in 
    List.iteri (fun i addr -> Printf.printf "%d - %s\n" i (Unix.string_of_inet_addr addr)) ifs ;  
    Unix.string_of_inet_addr  
    @@ match ifs with 
    | [] -> Unix.inet_addr_loopback
            (* If no interface are active we can only be reached through loopback *)
    | h::_ -> h)
  | _ -> iface

let zscout ?(iface="auto") ?(mask=Message.ScoutFlags.scoutBroker) ?(tries=3) ?(period=1.0) () = 
  let addr = Unix.inet_addr_of_string @@ select_iface iface in 
  let remote_scout_period = period in 
  let s = Message.Scout(Scout.create mask []) in   
  let sbuf = Bytes.create max_scout_msg_size in   
  let _ = Logs.debug (fun m -> m "Wrapping bytes in Abyte\n")  in 
  let asbuf = Abuf.from_bytes sbuf in   
  let _ = Logs_lwt.debug (fun m -> m "Encoding Scout message\n")  in 
  Abuf.clear asbuf;
  Mcodec.encode_msg_element s asbuf ;
  let _ = Logs_lwt.debug (fun m -> m "Encoded Scout message\n")  in 
  (* let ifaddr = Unix.inet_addr_of_string iface in  *)
  (* let saddr = Unix.ADDR_INET (ifaddr, 0) in  *)
  let saddr = Unix.ADDR_INET (addr, 0) in 
  let socket = Lwt_unix.socket Unix.PF_INET Unix.SOCK_DGRAM 0 in 
  let _ = Lwt_unix.bind socket saddr in 
  let sbuf_ctx = (sbuf, 0, Abuf.w_pos asbuf) in 

  (* Check if a broker is available on the current node *)
  let local_scout_address = Unix.ADDR_INET(Unix.inet_addr_loopback , scout_port) in 
  match%lwt scouting socket sbuf_ctx local_scout_tries local_scout_address local_scout_period with 
  | [] -> 
    (* If we can't scout a local broker than look around *)
    let%lwt locs = scouting socket sbuf_ctx tries mcast_scout_addr remote_scout_period in 
    Lwt.return @@ Locators.of_list locs 
  | ls -> 
    Printf.printf "Found non-empty locator list...\n";
    Lwt.return @@ Locators.of_list ls
  
  
    
let zopen ?username ?password peer = 
  let%lwt local_sex = Zenoh_local_router.open_local_session () in
  match local_sex with 
  | Some local_sex ->
    let sock = Stream(local_sex) in
    let (promise, resolver) = Lwt.task () in
    let _ = safe_run_decode_loop resolver {sock; state=Guard.create create_state; peer_pid=None;} in 
    let _ = send_message sock (make_open None None) in
    promise 
  | None ->
    let open Lwt_unix in
    let sock = socket PF_INET SOCK_STREAM 0 in
    setsockopt sock SO_REUSEADDR true;
    setsockopt sock TCP_NODELAY true;
    setsockopt sock SO_KEEPALIVE true;
    let saddr = Scanf.sscanf peer "%[^/]/%[^:]:%d" (fun _ ip port -> 
      ADDR_INET (Unix.inet_addr_of_string ip, port)) in
    let name_info = Unix.getnameinfo saddr [NI_NUMERICHOST; NI_NUMERICSERV] in
    let _ = Logs_lwt.debug (fun m -> m "peer : tcp/%s:%s" name_info.ni_hostname name_info.ni_service) in
    let (promise, resolver) = Lwt.task () in
    let con = connect sock saddr in 
    let sock = Sock(sock) in
    let _ = con >>= fun _ -> safe_run_decode_loop resolver {sock; state=Guard.create create_state; peer_pid=None;} in
    let _ = con >>= fun _ -> send_message sock (make_open username password) in
    con >>= fun _ -> promise

let zclose z =
  (* TODO: implement clean closure *)
  ignore z;
  Lwt.return_unit

let info z =
  let peer = 
  match z.sock with 
  | Sock sock -> 
    (match Unix.getpeername @@ Lwt_unix.unix_file_descr sock with
    | ADDR_UNIX a -> "unix:"^a
    | ADDR_INET (a,p) -> Printf.sprintf "%s:%d" (Unix.string_of_inet_addr a) p)
  | Stream _ -> "local"
  in
  let props = Apero.Properties.of_list ["peer", peer; "pid",(Abuf.hexdump pid)]  in 
  match z.peer_pid with 
  | None -> props 
  | Some bpid -> Apero.Properties.add "peer_pid" bpid props

let publish z resname = 
  let resname = PathExpr.of_string resname in
  let%lwt state = Guard.acquire z.state in
  let (res, state) = add_resource resname state in
  let (pubid, state) = get_next_entity_id state in

  let (sn, state) = get_next_sn state in
  let _ = send_message z.sock (Message.Declare(Declare.create (true, false) sn [
    ResourceDecl(ResourceDecl.create res.rid (PathExpr.to_string res.name) []);
    PublisherDecl(PublisherDecl.create res.rid [])
  ])) in 
  Guard.release z.state state;
  Lwt.return {z; id=pubid; resid=res.rid; reliable=false}


let write z resname ?timestamp ?kind ?encoding buf = 
  let payload = (Payload.create ~header:{empty_data_info with ts=timestamp; encoding; kind;} buf) in
  let _ = process_write_data z resname [payload] in
  let%lwt state = Guard.acquire z.state in
  let (sn, state) = get_next_sn state in
  let%lwt _ = send_message z.sock (Message.WriteData(WriteData.create (false, false) sn resname payload)) in
  Lwt.return @@ Guard.release z.state state

let stream (pub:pub) ?timestamp ?kind ?encoding buf = 
  let payload = (Payload.create ~header:{empty_data_info with ts=timestamp; encoding; kind;} buf) in
  let _ = process_stream_data pub.z pub.resid [payload] in
  let%lwt state = Guard.acquire pub.z.state in
  let (sn, state) = get_next_sn state in
  let%lwt _ = send_message pub.z.sock (Message.StreamData(StreamData.create (false, pub.reliable) sn pub.resid payload)) in
  Lwt.return @@ Guard.release pub.z.state state

let lstream (pub:pub) (bufs: Abuf.t list) = 
  let payloads = List.map (fun b -> Payload.create b) bufs in
  let _ = process_stream_data pub.z pub.resid payloads in
  let%lwt state = Guard.acquire pub.z.state in
  let (sn, state) = get_next_sn state in
  let%lwt _ = send_message pub.z.sock (Message.BatchedStreamData(BatchedStreamData.create (false, pub.reliable) sn pub.resid payloads)) in
  Lwt.return @@ Guard.release pub.z.state state


let unpublish z (pub:pub) = 
  let%lwt state = Guard.acquire z.state in
  let (sn, state) = get_next_sn state in
  let%lwt _ = send_message z.sock (Message.Declare(Declare.create (true, false) sn [
    ForgetPublisherDecl(ForgetPublisherDecl.create pub.resid)
  ])) in 
  Lwt.return @@ Guard.release z.state state


let push_mode = SubscriptionMode.push_mode
let pull_mode = SubscriptionMode.pull_mode


let subscribe z ?(mode=push_mode)  resname listener = 
  let resname = PathExpr.of_string resname in
  let%lwt state = Guard.acquire z.state in
  let (res, state) = add_resource resname state in
  let (subid, state) = get_next_entity_id state in
  let insub = {subid; resid=res.rid; listener} in
  let res = with_sub insub res in
  let resmap = VleMap.add res.rid res state.resmap in 
  let state = {state with resmap} in

  let (sn, state) = get_next_sn state in
  let _ = send_message z.sock (Message.Declare(Declare.create (true, false) sn [
    ResourceDecl(ResourceDecl.create res.rid (PathExpr.to_string res.name) []);
    SubscriberDecl(SubscriberDecl.create res.rid mode [])
  ])) in 
  Guard.release z.state state ;
  Lwt.return ({z=z; id=subid; resid=res.rid}:sub)


let pull (sub:sub) = 
  let%lwt state = Guard.acquire sub.z.state in
  let (sn, state) = get_next_sn state in 
  let%lwt _ = send_message sub.z.sock (Message.Pull(Pull.create (true, true) sn sub.resid None)) in 
  Lwt.return @@ Guard.release sub.z.state state


let unsubscribe z (sub:sub) = 
  let%lwt state = Guard.acquire z.state in
  let state = match VleMap.find_opt sub.resid state.resmap with 
  | None -> state 
  | Some res -> 
    let res = remove_sub sub.id res in 
    let resmap = VleMap.add res.rid res state.resmap in 
    {state with resmap} in
  let (sn, state) = get_next_sn state in
  let%lwt _ = send_message z.sock (Message.Declare(Declare.create (true, false) sn [
    ForgetSubscriberDecl(ForgetSubscriberDecl.create sub.resid)
  ])) in 
  Lwt.return @@ Guard.release z.state state 


let store z resname listener qhandler = 
  let resname = PathExpr.of_string resname in
  let%lwt state = Guard.acquire z.state in
  let (res, state) = add_resource resname state in
  let (stoid, state) = get_next_entity_id state in
  let insto = {stoid; resname; listener; qhandler} in
  let res = with_sto insto res in
  let resmap = VleMap.add res.rid res state.resmap in 
  let state = {state with resmap} in

  let (sn, state) = get_next_sn state in
  let _ = send_message z.sock (Message.Declare(Declare.create (true, false) sn [
    ResourceDecl(ResourceDecl.create res.rid (PathExpr.to_string res.name) []);
    StorageDecl(StorageDecl.create res.rid [])
  ])) in 
  Guard.release z.state state ;
  Lwt.return ({z=z; id=stoid; resid=res.rid} : storage)

let evaluate z resname qhandler = 
  let resname = PathExpr.of_string resname in
  let%lwt state = Guard.acquire z.state in
  let (res, state) = add_resource resname state in
  let (evalid, state) = get_next_entity_id state in
  let ineval = {evalid; resname; qhandler} in
  let res = with_eval ineval res in
  let resmap = VleMap.add res.rid res state.resmap in 
  let state = {state with resmap} in

  let (sn, state) = get_next_sn state in
  let _ = send_message z.sock (Message.Declare(Declare.create (true, false) sn [
    ResourceDecl(ResourceDecl.create res.rid (PathExpr.to_string res.name) []);
    EvalDecl(EvalDecl.create res.rid [])
  ])) in 
  Guard.release z.state state ;
  Lwt.return ({z=z; id=evalid; resid=res.rid} : eval)

let query z ?(dest_storages=Best_match) ?(dest_evals=Best_match) resname predicate listener = 
  let%lwt _ = process_query z resname predicate (fun source replies -> 
    match source with 
    | Storage -> 
      let%lwt _ = Lwt_list.iteri_s (fun rsn (resname, data, info) -> 
        listener (StorageData({stoid=pid; rsn; resname; data=copy data; info}))) replies in
      listener (StorageFinal({stoid=pid; rsn=(List.length replies)}))
    | Eval -> 
      let%lwt _ = Lwt_list.iteri_s (fun rsn (resname, data, info) -> 
        listener (EvalData({stoid=pid; rsn; resname; data=copy data; info}))) replies in
      listener (EvalFinal({stoid=pid; rsn=(List.length replies)})))
  in
  let%lwt state = Guard.acquire z.state in
  let (qryid, state) = get_next_qry_id state in
  let qrymap = VleMap.add qryid {qid=qryid; listener} state.qrymap in 
  let props = [ZProperty.DestStorages.make dest_storages; ZProperty.DestEvals.make dest_evals] in
  let state = {state with qrymap} in
  let%lwt _ = send_message z.sock (Message.Query(Query.create pid qryid resname predicate props)) in
  Lwt.return @@ Guard.release z.state state


let squery z ?(dest_storages=Best_match) ?(dest_evals=Best_match) resname predicate = 
  let stream, push = Lwt_stream.create () in 
  let reply_handler = function
    | ReplyFinal -> push @@ Some ReplyFinal; push None; Lwt.return_unit
    | qreply -> push @@ Some qreply; Lwt.return_unit
   in
  let _ = (query z resname predicate reply_handler ~dest_storages ~dest_evals) in 
  stream

module RepliesMap = Map.Make(String)
type lquery_context = {resolver: (string * Abuf.t * data_info) list Lwt.u; mutable map: (Abuf.t * data_info) list RepliesMap.t}

let lquery z ?(dest_storages=Best_match) ?(dest_evals=Best_match) ?(consolidation=LatestValue) resname predicate =
  let promise,resolver = Lwt.wait () in 
  let ctx = {resolver; map = RepliesMap.empty} in
  let add_reply resname data info =
    match RepliesMap.find_opt resname ctx.map , consolidation with
    | Some replies , LatestValue ->
      let _, info' = List.hd replies in
      (match info.ts, info'.ts with
      | _, None -> ctx.map <- RepliesMap.add resname [(data,info)] ctx.map
      | Some ts, Some ts' when Timestamp.compare ts ts' > 0 ->
        ctx.map <- RepliesMap.add resname [(data,info)] ctx.map
      | _ , _ -> ())
    | Some replies , KeepAll -> ctx.map <- RepliesMap.add resname ((data,info)::replies) ctx.map
    | None , _ -> ctx.map <- RepliesMap.add resname [(data,info)] ctx.map
  in
  let reply_handler = function
    | StorageData {stoid=_; rsn=_; resname; data; info}
    | EvalData {stoid=_; rsn=_; resname; data; info} -> add_reply resname data info; Lwt.return_unit
    | StorageFinal {stoid=_;rsn=_}
    | EvalFinal {stoid=_;rsn=_} -> Lwt.return_unit
    | ReplyFinal ->
      let result = RepliesMap.fold (fun resname replies result ->
          (List.map (fun (buf, info) -> (resname, buf, info)) replies ) @ result
        ) ctx.map []
      in
      Lwt.wakeup_later ctx.resolver result;
      Lwt.return_unit
  in
  let _ = (query z resname predicate reply_handler ~dest_storages ~dest_evals) in 
  promise


let unstore z (sto:storage) = 
  let%lwt state = Guard.acquire z.state in
  let state = match VleMap.find_opt sto.resid state.resmap with 
  | None -> state 
  | Some res -> 
    let res = remove_sto sto.id res in 
    let resmap = VleMap.add res.rid res state.resmap in 
    {state with resmap} in
  let (sn, state) = get_next_sn state in
  let%lwt _ = send_message z.sock (Message.Declare(Declare.create (true, false) sn [
    ForgetStorageDecl(ForgetStorageDecl.create sto.resid)
  ])) in 
  Lwt.return @@ Guard.release z.state state 


let unevaluate z (eval:eval) = 
  let%lwt state = Guard.acquire z.state in
  let state = match VleMap.find_opt eval.resid state.resmap with 
  | None -> state 
  | Some res -> 
    let res = remove_eval eval.id res in 
    let resmap = VleMap.add res.rid res state.resmap in 
    {state with resmap} in
  let (sn, state) = get_next_sn state in
  let%lwt _ = send_message z.sock (Message.Declare(Declare.create (true, false) sn [
    ForgetEvalDecl(ForgetEvalDecl.create eval.resid)
  ])) in 
  Lwt.return @@ Guard.release z.state state 