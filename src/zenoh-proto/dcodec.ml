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
open Message
open Pcodec
open Block


let encode_properties = function
  | [] -> fun _ -> ()
  | ps -> (encode_seq encode_property) ps 

let decode_properties h buf =
  if Flags.(hasFlag h pFlag) then ((decode_seq decode_property) buf)
  else []


let make_res_decl rid resource ps = Declaration.ResourceDecl (ResourceDecl.create rid resource ps)

let decode_res_decl header =
  read3_spec 
    (Logs.debug (fun m -> m "Reading ResourceDeclaration"))
    fast_decode_vle
    decode_string
    (decode_properties header)
    make_res_decl
    
let encode_res_decl d buf=
  let open ResourceDecl in
  Logs.debug (fun m -> m "Writing ResourceDeclaration") ;
  Abuf.write_byte (header d) buf;
  fast_encode_vle (rid d) buf;
  encode_string (resource d) buf;
  encode_properties (properties d) buf
  
let make_pub_decl rid ps = Declaration.PublisherDecl (PublisherDecl.create rid ps)

let decode_pub_decl header = 
  read2_spec 
  (Logs.debug (fun m -> m "Reading PubDeclaration"))
  fast_decode_vle
  (decode_properties header)
  make_pub_decl

let encode_pub_decl d buf =
  let open PublisherDecl in
  Logs.debug (fun m -> m "Writing PubDeclaration") ;
  let id = (rid d) in
  Logs.debug (fun m -> m  "Writing PubDeclaration for rid = %Ld" id) ;
  Abuf.write_byte (header d) buf;
  fast_encode_vle id buf;
  encode_properties (properties d) buf

let make_temporal_properties origin period duration = TemporalProperty.create origin period duration

let decode_temporal_properties =
  read3_spec
    (Logs.debug (fun m -> m "Reading TemporalProperties"))
    fast_decode_vle
    fast_decode_vle
    fast_decode_vle
    make_temporal_properties
  
let encode_temporal_properties stp buf =
  let open TemporalProperty in
  match stp with
  | None -> ()
  | Some tp ->
    Logs.debug (fun m -> m "Writing Temporal") ;
    fast_encode_vle (origin tp) buf;
    fast_encode_vle (period tp) buf;
    fast_encode_vle (duration tp) buf
    
let decode_sub_mode buf =
  Logs.debug (fun m -> m "Reading SubMode") ;
  Abuf.read_byte buf |> function 
  | id when Flags.mid id = SubscriptionModeId.pushModeId ->
    SubscriptionMode.PushMode
  | id when Flags.mid id = SubscriptionModeId.pullModeId ->
    SubscriptionMode.PullMode
  | id when Flags.mid id = SubscriptionModeId.periodicPushModeId ->
    decode_temporal_properties buf |> fun tp -> 
      SubscriptionMode.PeriodicPushMode tp
  | id when Flags.mid id = SubscriptionModeId.periodicPullModeId ->
    decode_temporal_properties buf |> fun tp -> 
      SubscriptionMode.PeriodicPullMode tp
  | _ -> raise @@ Exception(`UnknownSubMode)
  
let encode_sub_mode m buf =
  let open SubscriptionMode in
  Logs.debug (fun m -> m "Writing SubMode") ;  
  Abuf.write_byte (id m) buf;
  encode_temporal_properties (temporal_properties m) buf

let make_sub_decl rid mode ps = Declaration.SubscriberDecl (SubscriberDecl.create rid mode ps)

let decode_sub_decl header =
  read3_spec
    (Logs.debug (fun m -> m "Reading SubDeclaration"))
    fast_decode_vle
    decode_sub_mode
    (decode_properties header)
    make_sub_decl
  
let encode_sub_decl d buf =
  let open SubscriberDecl in
  Logs.debug (fun m -> m "Writing SubDeclaration") ;
  let id = (rid d) in
  Logs.debug (fun m -> m "Writing SubDeclaration for rid = %Ld" id) ;
  Abuf.write_byte (header d) buf;
  fast_encode_vle id buf;
  encode_sub_mode (mode d) buf;
  encode_properties (properties d) buf


let make_selection_decl h sid query ps = 
  Declaration.SelectionDecl (SelectionDecl.create sid query ps (Flags.(hasFlag h gFlag)))

let decode_selection_decl header = 
  read3_spec
    (Logs.debug (fun m -> m "Reading SelectionDeclaration"))
    fast_decode_vle  
    decode_string
    (decode_properties header)
    (make_selection_decl header)
    
let encode_selection_decl d buf =
  let open SelectionDecl in
  Logs.debug (fun m -> m "Writing SelectionDeclaration");
  Abuf.write_byte (header d) buf;
  fast_encode_vle (sid d) buf;
  encode_string (query d) buf;
  encode_properties (properties d) buf
  
let make_binding_decl h oldid newid = 
  Declaration.BindingDecl (BindingDecl.create oldid newid (Flags.(hasFlag h gFlag)))

let decode_binding_decl header =  
  read2_spec
    (Logs.debug (fun m -> m "Reading BindingDeclaration"))
    fast_decode_vle
    fast_decode_vle
    (make_binding_decl header)

let encode_bindind_decl d buf =
  let open BindingDecl in
  Logs.debug (fun m -> m "Writing BindingDeclaration") ;
  Abuf.write_byte (header d) buf;
  fast_encode_vle (old_id d) buf;
  fast_encode_vle (new_id d) buf

let make_commit_decl commit_id = (Declaration.CommitDecl (CommitDecl.create commit_id))

let decode_commit_decl = 
  read1_spec 
    (Logs.debug (fun m -> m "Reading Commit Declaration"))
    Abuf.read_byte
    make_commit_decl
  
let encode_commit_decl cd buf =
  let open CommitDecl in  
  Logs.debug (fun m -> m "Writing Commit Declaration");
  Abuf.write_byte (header cd) buf;
  Abuf.write_byte (commit_id cd) buf
  
let decode_result_decl buf =  
  Logs.debug (fun m -> m "Reading Result Declaration");
  Abuf.read_byte buf |> fun commit_id -> 
    Abuf.read_byte buf |> function 
      | status when status = char_of_int 0 ->
        Declaration.ResultDecl  (ResultDecl.create commit_id status None)
      | status ->
        fast_decode_vle buf |> fun v ->
          Declaration.ResultDecl (ResultDecl.create commit_id status (Some v))

let encode_result_decl rd buf =
  let open ResultDecl in  
  Logs.debug (fun m -> m "Writing Result Declaration") ;
  Abuf.write_byte (header rd) buf;
  Abuf.write_byte (commit_id rd) buf;
  Abuf.write_byte (status rd) buf;
  match (id rd) with 
  | None -> ()
  | Some v -> fast_encode_vle v buf


let decode_forget_res_decl buf =
  Logs.debug (fun m -> m "Reading ForgetResource Declaration");
  fast_decode_vle buf |> fun rid ->
    Declaration.ForgetResourceDecl (ForgetResourceDecl.create rid)
  
let encode_forget_res_decl frd buf =
  let open ForgetResourceDecl in
  Logs.debug (fun m -> m "Writing ForgetResource Declaration");
  Abuf.write_byte (header frd) buf;
  fast_encode_vle (rid frd) buf


let decode_forget_pub_decl buf =
  Logs.debug (fun m -> m "Reading ForgetPublisher Declaration");
  fast_decode_vle buf |> fun id -> 
    Declaration.ForgetPublisherDecl (ForgetPublisherDecl.create id)
  
let encode_forget_pub_decl fpd buf =
  let open ForgetPublisherDecl in
  Logs.debug (fun m -> m "Writing ForgetPublisher Declaration");
  Abuf.write_byte (header fpd) buf;
  fast_encode_vle (id fpd) buf


let decode_forget_sub_decl buf =
  Logs.debug (fun m -> m "Reading ForgetSubscriber Declaration");
  fast_decode_vle buf |> fun id -> 
    Declaration.ForgetSubscriberDecl (ForgetSubscriberDecl.create id)
  
let encode_forget_sub_decl fsd buf =
  let open ForgetSubscriberDecl in
  Logs.debug (fun m -> m "Writing ForgetSubscriber Declaration");
  Abuf.write_byte (header fsd) buf;
  fast_encode_vle (id fsd) buf


let decode_forget_sel_decl buf =
  Logs.debug (fun m -> m "Reading ForgetSelection Declaration") ;
  fast_decode_vle buf |> fun sid -> 
    Declaration.ForgetSelectionDecl (ForgetSelectionDecl.create sid)

let encode_forget_sel_decl fsd buf =
  let open ForgetSelectionDecl in
  Logs.debug (fun m -> m "Writing ForgetSelection Declaration" );   
  Abuf.write_byte (header fsd) buf;
  fast_encode_vle (sid fsd) buf

let make_storage_decl rid ps = Declaration.StorageDecl (StorageDecl.create rid ps)

let decode_storage_decl header = 
  read2_spec 
  (Logs.debug (fun m -> m "Reading StorageDeclaration"))
  fast_decode_vle
  (decode_properties header)
  make_storage_decl

let encode_storage_decl d buf =
  let open StorageDecl in
  Logs.debug (fun m -> m "Writing StorageDeclaration") ;
  let id = (rid d) in
  Logs.debug (fun m -> m  "Writing StorageDeclaration for rid = %Ld" id) ;
  Abuf.write_byte (header d) buf;
  fast_encode_vle id buf;
  encode_properties (properties d) buf


let decode_forget_storage_decl buf =
  Logs.debug (fun m -> m "Reading ForgetStorage Declaration");
  fast_decode_vle buf |> fun id -> 
    Declaration.ForgetStorageDecl (ForgetStorageDecl.create id)
  
let encode_forget_storage_decl fsd buf =
  let open ForgetStorageDecl in
  Logs.debug (fun m -> m "Writing ForgetStorage Declaration");
  Abuf.write_byte (header fsd) buf;
  fast_encode_vle (id fsd) buf

let make_eval_decl rid ps = Declaration.EvalDecl (EvalDecl.create rid ps)

let decode_eval_decl header = 
  read2_spec 
  (Logs.debug (fun m -> m "Reading EvalDeclaration"))
  fast_decode_vle
  (decode_properties header)
  make_eval_decl

let encode_eval_decl d buf =
  let open EvalDecl in
  Logs.debug (fun m -> m "Writing EvalDeclaration") ;
  let id = (rid d) in
  Logs.debug (fun m -> m  "Writing EvalDeclaration for rid = %Ld" id) ;
  Abuf.write_byte (header d) buf;
  fast_encode_vle id buf;
  encode_properties (properties d) buf


let decode_forget_eval_decl buf =
  Logs.debug (fun m -> m "Reading ForgetEval Declaration");
  fast_decode_vle buf |> fun id -> 
    Declaration.ForgetEvalDecl (ForgetEvalDecl.create id)
  
let encode_forget_eval_decl fsd buf =
  let open ForgetEvalDecl in
  Logs.debug (fun m -> m "Writing ForgetEval Declaration");
  Abuf.write_byte (header fsd) buf;
  fast_encode_vle (id fsd) buf