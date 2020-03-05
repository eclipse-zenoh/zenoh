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
open Apero_net
open Message
open Dcodec
open Frame
open Block
open Reliable
open Session
open Lwt.Infix
open Apero.Infix

type element =
  | Message of Message.t
  | Marker of marker

let decode_payload h = Apero.Infix.(decode_buf %> Payload.from_buffer h)

let make_scout mask ps = Message (Scout (Scout.create mask ps))

let decode_scout header = 
  read2_spec
    (Logs.debug (fun m -> m "Reading Scout"))
    fast_decode_vle 
    (Dcodec.decode_properties header) 
    make_scout

let encode_scout scout buf =
  let open Scout in
  Logs.debug (fun m -> m "Writring Scout\n") ;
  Abuf.write_byte (header scout) buf;
  fast_encode_vle (mask scout) buf;
  encode_properties (properties scout) buf


let make_hello mask ls ps = Message (Hello (Hello.create mask ls ps))

let decode_hello header =
  read3_spec
    (Logs.debug (fun m -> m "Reading Hello"))    
    fast_decode_vle
    decode_locators     
    (decode_properties header)    
    make_hello

let encode_hello hello buf =
  let open Hello in
  Logs.debug (fun m -> m "Writing Hello") ;
  Abuf.write_byte (header hello) buf;
  fast_encode_vle (mask hello) buf;
  encode_locators (locators hello) buf;
  encode_properties  (properties hello) buf


let make_open version pid lease locs ps = Message (Open (Open.create version pid lease locs ps))

let decode_open header =      
  (read5_spec 
     (Logs.debug (fun m -> m "Reading Open"))
     Abuf.read_byte
     decode_buf
     fast_decode_vle
     decode_locators
     (decode_properties header)
     make_open)

let encode_open msg buf =
  let open Open in
  Logs.debug (fun m -> m "Writing Open") ;
  Abuf.write_byte (header msg) buf;
  Abuf.write_byte (version msg) buf;
  encode_buf (pid msg) buf;
  fast_encode_vle (lease msg) buf;
  encode_locators (locators msg) buf;
  Dcodec.encode_properties (properties msg) buf


let make_accept opid apid lease ps = Message (Accept (Accept.create opid apid lease ps))

let decode_accept header =
  read4_spec 
    (Logs.debug (fun m -> m"Reading Accept"))
    decode_buf
    decode_buf
    fast_decode_vle 
    (decode_properties header)
    make_accept  

let encode_accept accept buf =
  let open Accept in
  Logs.debug (fun m -> m "Writing Accept") ;
  Abuf.write_byte (header accept) buf;
  encode_buf (opid accept) buf;
  encode_buf (apid accept) buf;
  fast_encode_vle (lease accept) buf;
  Dcodec.encode_properties (properties accept) buf


let make_close pid reason = Message (Close (Close.create pid reason))

let decode_close _ = 
  read2_spec
    (Logs.debug (fun m -> m "Reading Close"))
    decode_buf
    Abuf.read_byte
    make_close

let encode_close close buf =
  let open Close in
  Logs.debug (fun m -> m "Writing Close") ;
  Abuf.write_byte (header close) buf;
  encode_buf (pid close) buf;
  Abuf.write_byte (reason close) buf  


let decode_declaration buf =  
  Abuf.read_byte buf |> fun header ->
  match Flags.mid header with
  | r when r = DeclarationId.resourceDeclId -> decode_res_decl header buf
  | p when p = DeclarationId.publisherDeclId -> decode_pub_decl header buf 
  | s when s = DeclarationId.subscriberDeclId -> decode_sub_decl header buf
  | s when s = DeclarationId.selectionDeclId -> decode_selection_decl header buf
  | b when b = DeclarationId.bindingDeclId -> decode_binding_decl header buf 
  | c when c = DeclarationId.commitDeclId -> decode_commit_decl buf 
  | r when r = DeclarationId.resultDeclId -> decode_result_decl  buf 
  | r when r = DeclarationId.forgetResourceDeclId -> decode_forget_res_decl buf
  | r when r = DeclarationId.forgetPublisherDeclId -> decode_forget_pub_decl  buf
  | r when r = DeclarationId.forgetSubscriberDeclId -> decode_forget_sub_decl  buf 
  | r when r = DeclarationId.forgetSelectionDeclId -> decode_forget_sel_decl buf 
  | s when s = DeclarationId.storageDeclId -> decode_storage_decl header buf
  | f when f = DeclarationId.forgetStorageDeclId -> decode_forget_storage_decl buf
  | s when s = DeclarationId.evalDeclId -> decode_eval_decl header buf
  | f when f = DeclarationId.forgetEvalDeclId -> decode_forget_eval_decl buf
  | _ -> raise @@ Exception(`NotImplemented)

let encode_declaration (d: Declaration.t) buf=
  match d with
  | ResourceDecl rd -> encode_res_decl rd buf
  | PublisherDecl pd -> encode_pub_decl pd buf 
  | SubscriberDecl sd -> encode_sub_decl sd buf 
  | SelectionDecl sd -> encode_selection_decl sd buf
  | BindingDecl bd -> encode_bindind_decl bd buf 
  | CommitDecl cd -> encode_commit_decl cd buf 
  | ResultDecl rd -> encode_result_decl rd buf
  | ForgetResourceDecl frd -> encode_forget_res_decl  frd buf
  | ForgetPublisherDecl fpd -> encode_forget_pub_decl fpd buf 
  | ForgetSubscriberDecl fsd -> encode_forget_sub_decl fsd buf 
  | ForgetSelectionDecl fsd -> encode_forget_sel_decl fsd buf 
  | StorageDecl sd -> encode_storage_decl sd buf 
  | ForgetStorageDecl fsd -> encode_forget_storage_decl fsd buf 
  | EvalDecl ed -> encode_eval_decl ed buf 
  | ForgetEvalDecl fed -> encode_forget_eval_decl fed buf 


let decode_declarations buf = 
  let rec loop  n ds = 
    if n = 0 then ds
    else 
      decode_declaration buf |> fun d ->
        loop (n-1) (d::ds)
  in 
  fast_decode_vle buf |> fun len ->
    loop (Vle.to_int len) []

let encode_declarations ds buf =
  fast_encode_vle (Vle.of_int @@ List.length ds) buf;
  List.iter (fun d -> encode_declaration d buf) ds


let make_declare h sn ds = 
  Message (Declare (Declare.create ((Flags.hasFlag h Flags.sFlag), (Flags.hasFlag h Flags.cFlag)) sn ds))

let decode_declare header =
  read2_spec 
    (Logs.debug (fun m -> m "Reading Declare message"))
    fast_decode_vle
    decode_declarations
    (make_declare header)

let encode_declare decl buf=
  let open Declare in  
  Logs.debug (fun m -> m "Writing Declare message");
  Abuf.write_byte (header decl) buf;
  fast_encode_vle (sn decl) buf;
  encode_declarations (declarations decl) buf


let make_write_data h sn resource payload = 
  let (s, r) = ((Flags.hasFlag h Flags.sFlag), (Flags.hasFlag h Flags.rFlag)) in
  Message (WriteData (WriteData.create (s, r) sn resource payload))

let decode_write_data header buf =
  Logs.debug (fun m -> m "Reading WriteData");
  let sn = fast_decode_vle buf in 
  let resource = decode_string buf in 
  let payload = decode_payload true buf in 
  make_write_data header sn resource payload

let encode_write_data m buf =
  let open WriteData in
  Logs.debug (fun m -> m "Writing WriteData");
  Abuf.write_byte (header m) buf;
  fast_encode_vle (sn m) buf;
  encode_string (resource m) buf;
  encode_buf (Payload.buffer @@ payload m) buf

let decode_prid h buf = 
  if Flags.(hasFlag h aFlag) 
  then fast_decode_vle buf |> fun v -> Some v
  else None

let encode_prid = function
  | None -> fun _ -> ()
  | Some v -> fast_encode_vle v

let make_compact_data h sn id prid payload = 
  let (s, r) = ((Flags.hasFlag h Flags.sFlag), (Flags.hasFlag h Flags.rFlag)) in
  Message (CompactData (CompactData.create (s, r) sn id prid payload))

let decode_compact_data header =
  read4_spec 
    (Logs.debug (fun m -> m "Reading CompactData"))
    decode_vle
    decode_vle
    (decode_prid header)
    (decode_payload false)
    (make_compact_data header)

let encode_compact_data m buf =
  let open CompactData in
  Logs.debug (fun m -> m "Writing CompactData");
  Abuf.write_byte (header m) buf;
  encode_vle (sn m) buf;
  encode_vle (id m) buf;
  encode_prid (prid m) buf;
  encode_buf (Payload.data @@ payload m) buf

let make_stream_data h sn id payload = 
  let (s, r) = ((Flags.hasFlag h Flags.sFlag), (Flags.hasFlag h Flags.rFlag)) in
  Message (StreamData (StreamData.create (s, r) sn id payload))

let decode_stream_data header buf =
  Logs.debug (fun m -> m "Reading StreamData");
  let sn = fast_decode_vle buf in 
  let id = fast_decode_vle buf in 
  let p = decode_payload true buf in 
  make_stream_data header sn id p   

let encode_stream_data m buf =
  let open StreamData in  
  Logs.debug (fun m -> m "Writing StreamData");
  Abuf.write_byte (header m) buf;
  fast_encode_vle (sn m) buf;
  fast_encode_vle (id m) buf;
  encode_buf (Payload.buffer @@ payload m) buf


let decode_batched_stream_data header buf =  
  let sn = fast_decode_vle buf in 
  let id = fast_decode_vle buf in 
  let payload = decode_seq (decode_payload true) buf in
  let flags = ((Flags.hasFlag header Flags.sFlag), (Flags.hasFlag header Flags.rFlag)) in
  Message (BatchedStreamData (BatchedStreamData.create flags sn id payload))

let encode_batched_stream_data m buf =  
  let open BatchedStreamData in 
  let open Apero.Infix in
  Abuf.write_byte (header m) buf;
  fast_encode_vle (sn m) buf;
  fast_encode_vle (id m) buf;    
  encode_seq (Payload.buffer %> encode_buf) (payload m) buf
  

let decode_synch_count h buf = 
  if Flags.(hasFlag h uFlag) 
  then fast_decode_vle buf |> fun v -> Some v
  else None


let make_synch h sn c= 
  let  (s, r) = Flags.(hasFlag h sFlag, hasFlag h rFlag) in
  Message (Synch (Synch.create (s,r) sn c))

let decode_synch header =
  read2_spec 
    (Logs.debug (fun m -> m "Reading Synch"))
    fast_decode_vle
    (decode_synch_count header)
    (make_synch header)

let encode_synch m buf =
  let open Synch in  
  Logs.debug (fun m -> m "Writing Synch");
  Abuf.write_byte (header m) buf;
  fast_encode_vle (sn m) buf;
  match count m  with
  | None -> ()
  | Some c -> fast_encode_vle c buf


let make_ack sn m = Message (AckNack (AckNack.create sn m))

let decode_acknack_mask h buf =
  if Flags.(hasFlag h mFlag) 
  then fast_decode_vle buf |> fun m -> Some m
  else None

let decode_ack_nack header =
  read2_spec 
    (Logs.debug (fun m -> m "Reading AckNack"))
    fast_decode_vle
    (decode_acknack_mask header)
    make_ack

let encode_ack_nack m buf =
  let open AckNack in
  Logs.debug (fun m -> m "Writing AckNack");
  Abuf.write_byte (header m) buf;
  fast_encode_vle (sn m) buf;
  match mask m with
  | None -> ()
  | Some v -> fast_encode_vle v buf

let decode_keep_alive _ buf =
  Logs.debug (fun m -> m "Reading KeepAlive");
  decode_buf buf |> fun pid -> Message (KeepAlive (KeepAlive.create pid))

let encode_keep_alive keep_alive buf =
  let open KeepAlive in  
  Logs.debug (fun m -> m "Writing KeepAlive");
  Abuf.write_byte (header keep_alive) buf;
  encode_buf (pid keep_alive) buf

let decode_migrate_id h buf = 
  if Flags.(hasFlag h iFlag) 
  then fast_decode_vle buf |> fun id -> Some id
  else None


let make_migrate ocid id rch_last_sn bech_last_sn =
  Message (Migrate (Migrate.create ocid id rch_last_sn bech_last_sn))

let decode_migrate header =
  read4_spec
    (Logs.debug (fun m -> m "Reading Migrate"))
    fast_decode_vle
    (decode_migrate_id header)
    fast_decode_vle 
    fast_decode_vle 
    make_migrate

let encode_migrate m buf =
  let open Migrate in  
  Logs.debug (fun m -> m "Writing Migrate");
  Abuf.write_byte (header m) buf;
  fast_encode_vle (ocid m) buf;
  (match id m with
  | None -> ()
  | Some id -> fast_encode_vle id buf);
  fast_encode_vle (rch_last_sn m) buf;
  fast_encode_vle (bech_last_sn m) buf


let decode_max_samples header buf = 
  if Flags.(hasFlag header nFlag) 
  then fast_decode_vle buf |> fun max_samples -> Some max_samples
  else None

let make_query pid qid resource predicate properties = 
  Message (Query (Query.create pid qid resource predicate properties))

let decode_query header =
  read5_spec
    (Logs.debug (fun m -> m "Reading Query"))
    decode_buf
    fast_decode_vle
    decode_string
    decode_string
    (decode_properties header)
    make_query

let encode_query m buf =
  let open Query in  
  Logs.debug (fun m -> m "Writing Query");
  Abuf.write_byte (header m) buf;
  encode_buf (pid m) buf;
  fast_encode_vle (qid m) buf;
  encode_string (resource m) buf;
  encode_string (predicate m) buf;
  Dcodec.encode_properties (properties m) buf

let make_reply source qpid qid value = 
  Message (Reply (Reply.create qpid qid source value))

let decode_reply_value header buf = 
  if Flags.(hasFlag header fFlag) 
  then
    read4_spec
      (Logs.debug (fun m -> m "  Reading Reply value"))
      decode_buf
      fast_decode_vle
      decode_string
      (decode_payload true)
      (fun stoid rsn resource payload -> Some (stoid, rsn, resource, payload)) buf
  else None

let decode_reply header buf =
  let source = match Flags.hasFlag header Flags.eFlag with | true -> Reply.Eval | false -> Reply.Storage in
  read3_spec
    (Logs.debug (fun m -> m "Reading Reply src=%s" (match source with | Reply.Eval -> "Eval" | Reply.Storage -> "Storage")))
    decode_buf
    fast_decode_vle
    (decode_reply_value header)
    (make_reply source)
    buf

let encode_reply_value v buf =
  match v with 
  | None -> ()
  | Some (stoid, rsn, resource, payload) -> 
    encode_buf stoid buf;
    fast_encode_vle rsn buf;
    encode_string resource buf;
    encode_buf (Payload.buffer payload) buf

let encode_reply m buf =
  let open Reply in  
  Logs.debug (fun m -> m "Writing Reply");
  Abuf.write_byte (header m) buf;
  encode_buf (qpid m) buf;
  fast_encode_vle (qid m) buf;
  encode_reply_value (value m) buf

let make_pull header sn id max_samples = 
  let (s, f) = ((Flags.hasFlag header Flags.sFlag), (Flags.hasFlag header Flags.fFlag)) in
  Message (Pull (Pull.create (s, f) sn id max_samples))

let decode_pull header =
  read3_spec
    (Logs.debug (fun m -> m "Reading Pull"))
    fast_decode_vle
    fast_decode_vle
    (decode_max_samples header)
    (make_pull header)

let encode_pull m buf =
  let open Pull in  
  Logs.debug (fun m -> m "Writing Pull");
  Abuf.write_byte (header m) buf;
  fast_encode_vle (sn m) buf;
  fast_encode_vle (id m) buf;
  match max_samples m with
  | None -> ()
  | Some max -> fast_encode_vle max buf

let decode_ping_pong header buf =  
  Logs.debug (fun m -> m "Reading PingPong");
  let o = Flags.hasFlag header Flags.oFlag in
  fast_decode_vle buf |> fun hash ->  
    Message (PingPong (PingPong.create ~pong:o hash))

let encode_ping_pong  m buf=
  let open PingPong in
  Logs.debug (fun m -> m "Writing PingPong");
  Abuf.write_byte (header m) buf;
  fast_encode_vle (hash m) buf

let decode_compact_id header buf = 
  (* @AC: Olivier the conduit marker should always have a cid, that should not be 
         optional. The way in which it is encoded changes, but not the fact of 
         having an id... *)
  if Flags.(hasFlag header zFlag) then
    let flags = (int_of_char (Flags.flags header)) lsr Flags.mid_len in 
    Vle.of_int @@ (flags land 0x3)
  else 
    fast_decode_vle buf

let decode_conduit header buf = 
  Logs.debug (fun m -> m "Reading Conduit") ;
  decode_compact_id header buf |> fun id -> Marker (ConduitMarker (ConduitMarker.create (Vle.add id Vle.one)))

let encode_conduit m buf = 
  let open ConduitMarker in
  Logs.debug (fun m -> m "Writing Conduit");
  Abuf.write_byte (header m) buf;
  match Flags.hasFlag (header m) Flags.zFlag with 
  | true -> ()
  | false -> fast_encode_vle (id m) buf


let decode_frag_num header buf = 
  if Flags.(hasFlag header nFlag) 
  then fast_decode_vle buf |> fun n -> Some n
  else None

let make_frag sn_base n = Marker (Frag (Frag.create sn_base n))  

let decode_frag header =   
  read2_spec 
    (Logs.debug (fun m -> m "Reading Frag"))
    fast_decode_vle
    (decode_frag_num header)
    make_frag 

let encode_frag m buf = 
  let open Frag in  
  Logs.debug (fun m ->  m "Writing Frag");
  Abuf.write_byte (header m) buf;
  fast_encode_vle (sn_base m) buf;
  match n m with 
  | None -> () 
  | Some n -> fast_encode_vle n buf

let decode_rspace header buf = 
  Logs.debug (fun m -> m "Reading ResourceSpace");
  decode_compact_id header buf |> fun id -> Marker (RSpace (RSpace.create id))

let encode_rspace m buf = 
  let open RSpace in  
  Logs.debug (fun m -> m "Writing ResourceSpace");
  Abuf.write_byte (header m) buf;
  match Flags.hasFlag (header m) Flags.zFlag with 
  | true -> () 
  | false -> fast_encode_vle (id m) buf

let decode_element buf =
  Abuf.read_byte buf |> fun header ->
    match char_of_int (Header.mid (header)) with
    | id when id = MessageId.bdataId -> (decode_batched_stream_data header buf)
    | id when id = MessageId.scoutId ->  (decode_scout header buf) 
    | id when id = MessageId.helloId ->  (decode_hello header buf)
    | id when id = MessageId.openId ->  (decode_open header buf)
    | id when id = MessageId.acceptId -> (decode_accept header buf)
    | id when id = MessageId.closeId ->  (decode_close header buf)
    | id when id = MessageId.declareId -> (decode_declare header buf)
    | id when id = MessageId.wdataId ->  (decode_write_data header buf)
    | id when id = MessageId.cdataId ->  (decode_compact_data header buf)
    | id when id = MessageId.sdataId ->  (decode_stream_data header buf)
    | id when id = MessageId.synchId -> (decode_synch header buf)
    | id when id = MessageId.ackNackId -> (decode_ack_nack header buf)
    | id when id = MessageId.keepAliveId -> (decode_keep_alive header buf)
    | id when id = MessageId.migrateId -> (decode_migrate header buf)
    | id when id = MessageId.queryId -> (decode_query header buf)
    | id when id = MessageId.replyId -> (decode_reply header buf)
    | id when id = MessageId.pullId -> (decode_pull header buf)
    | id when id = MessageId.pingPongId -> (decode_ping_pong header buf)
    | id when id = MessageId.conduitId -> (decode_conduit header buf)
    | id when id = MessageId.fragmetsId -> (decode_frag header buf)
    | id when id = MessageId.rSpaceId -> (decode_rspace header buf)
    | uid ->
      Logs.debug (fun m -> m "Received unknown message id: %d" (int_of_char uid));
      raise @@ Exception(`UnknownMessageId)


let rec decode_msg_rec buf markers = 
  decode_element buf |> fun elem -> 
    match elem with 
    | Marker m -> decode_msg_rec buf (m :: markers)
    | Message m -> Message.with_markers m markers

let decode_msg buf = decode_msg_rec buf []

let encode_marker marker =
  match marker with
  | ConduitMarker c -> encode_conduit c
  | Frag c -> encode_frag c
  | RSpace c -> encode_rspace c

let encode_msg_element msg =
  let open Message in
  match msg with
  | Scout m -> encode_scout m
  | Hello m -> encode_hello m
  | Open m -> encode_open  m
  | Accept m -> encode_accept m
  | Close m -> encode_close  m
  | Declare m -> encode_declare m
  | WriteData m -> encode_write_data m
  | CompactData m -> encode_compact_data m
  | StreamData m -> encode_stream_data m
  | BatchedStreamData m -> encode_batched_stream_data m
  | Synch m -> encode_synch  m
  | AckNack m -> encode_ack_nack m
  | KeepAlive m -> encode_keep_alive  m
  | Migrate m -> encode_migrate  m
  | Query m -> encode_query m
  | Reply m -> encode_reply m
  | Pull m -> encode_pull m
  | PingPong m -> encode_ping_pong m


let encode_msg msg buf =
  let open Message in
  let rec encode_msg_wm msg markers =
    match markers with 
    | marker :: markers -> 
      encode_marker marker buf;
      encode_msg_wm msg markers
    | [] -> encode_msg_element msg buf
  in  encode_msg_wm msg (markers msg)


let decode_frame_length = fast_decode_vle 

let encode_frame_length = fast_encode_vle 


let decode_frame buf = 
  let rec parse_messages ms =
    if Abuf.readable_bytes buf > 0 then                     
      decode_msg buf |> fun m -> parse_messages (m::ms)
    else List.rev ms
  in 
  parse_messages [] |> fun ms -> Frame.create ms

let encode_frame frame buf = List.iter (fun m -> encode_msg m buf) frame


(* let decode_frame buf =
  let rec rloop n msgs buf =
    if n = 0 then return (msgs, buf)
    else
      decode_msg buf
      >>= (fun (msg, buf) -> rloop (n-1) (msg::msgs) buf)

  in
  decode_frame_length buf 
  >>= (fun (len, buf) -> 
      rloop  (Vle.to_int len) [] buf
      >>= fun (msgs, buf)  -> return (Frame.create msgs, buf))

let encode_frame f =  
  fold_m encode_msg (Frame.to_list f) *)


let ztcp_read_frame tx_sex buf () =
  match tx_sex with 
  | TxSex tx_sex -> 
    let open Lwt.Infix in 
    let sock = TxSession.socket tx_sex in 
    let%lwt len = Net.read_vle sock >|= Vle.to_int in 
    let%lwt _ = Net.read_all sock buf len in 
    Lwt.return @@ decode_frame buf
  | Local lo_sex -> Lwt_stream.get lo_sex.stream >>= (Option.get %> Lwt.return)


let ztcp_write_frame tx_sex frame buf = 
  match tx_sex with 
  | TxSex tx_sex -> 
    let sock = TxSession.socket tx_sex in 
    Abuf.clear buf;
    let ms = Frame.to_list frame in
    List.iter (fun m -> encode_msg m buf) ms;
    let lbuf = Abuf.create 8 in 
    fast_encode_vle (Vle.of_int @@ Abuf.readable_bytes buf) lbuf;
    Net.write_all sock (Abuf.wrap [lbuf; buf])
  | Local lo_sex -> 
    lo_sex.push#push frame >>= fun () -> Lwt.return 0
    

let ztcp_safe_write_frame tx_sex frame buf =
  Lwt.catch 
    (fun () -> ztcp_write_frame tx_sex frame buf)
    (fun ex -> Logs.warn(fun m -> m "Error sending frame : %s" (Printexc.to_string ex)); Lwt.return 0)

let ztcp_write_frame_alloc tx_sex frame =
  (* We shoud compute the size and allocate accordingly *)
  let buf = Abuf.create ~grow:8192 65536 in 
  ztcp_write_frame tx_sex frame buf

let ztcp_safe_write_frame_alloc tx_sex frame =
  Lwt.catch 
    (fun () -> ztcp_write_frame_alloc tx_sex frame)
    (fun ex -> Logs.warn(fun m -> m "Error sending frame : %s" (Printexc.to_string ex)); Lwt.return 0)

let ztcp_write_frame_pooled tx_sex frame pool = Lwt_pool.use pool @@ ztcp_write_frame tx_sex frame

let ztcp_safe_write_frame_pooled tx_sex frame pool =
  Lwt.catch 
    (fun () -> ztcp_write_frame_pooled tx_sex frame pool)
    (fun ex -> Logs.warn(fun m -> m "Error sending frame : %s" (Printexc.to_string ex)); Lwt.return 0)