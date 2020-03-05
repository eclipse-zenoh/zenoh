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
open Channel
open NetService
open R_name

let framing_buf_len = 16

type local_sex = {
  sid : NetService.Id.t; 
  stream : Frame.Frame.t Lwt_stream.t;
  push : Frame.Frame.t Lwt_stream.bounded_push
}

type tx_sex = | TxSex of TxSession.t | Local of local_sex


let txid = function | TxSex tx -> TxSession.id tx | Local lx -> lx.sid

type stats = {
  mutable out_msgs : int;
  mutable out_msgs_tp : int;
  mutable out_msg_tp_time : float;
  mutable out_msgs_tp_build : int;
}

let create_stats () = 
  {
    out_msgs = 0;
    out_msgs_tp = 0;
    out_msg_tp_time = 0.0;
    out_msgs_tp_build = 0;
  }

let update_stats s = 
  let now = Unix.gettimeofday () in 
  match s.out_msg_tp_time == 0.0 with 
  | true -> s.out_msg_tp_time <- now
  | false -> 
    match now > s.out_msg_tp_time +. 1.0 with 
    | true -> 
      s.out_msgs_tp <- s.out_msgs_tp_build; 
      s.out_msg_tp_time <- s.out_msg_tp_time +. 1.0;
      s.out_msgs_tp_build <- 0
    | false -> ()

let stats_to_yojson s = 
  update_stats s;
  `Assoc  [ ("out_msgs", `Int s.out_msgs) ; ("out_msgs_tp", `Int s.out_msgs_tp) ]  

let add_out_msg s = 
  s.out_msgs <- s.out_msgs + 1;
  update_stats s;
  s.out_msgs_tp_build <- s.out_msgs_tp_build + 1

type t = {    
  tx_sex : tx_sex;      
  ic : InChannel.t;
  oc : OutChannel.t;
  rmap : ResName.t VleMap.t;
  mask : Vle.t;
  sid : Id.t;
  pending_pull : Payload.t ResMap.t;
  stats : stats;
} 

let to_yojson t = 
  `Assoc  [ 
    ("sid", `String (Id.to_string t.sid)); 
    ("mask", `Int (Vle.to_int t.mask)); 
    ("stats", stats_to_yojson t.stats);
  ]

let create tx_sex mask =
  let ic = InChannel.create Int64.(shift_left 1L 16) in
  let oc = OutChannel.create Int64.(shift_left 1L 16) in        
  {      
    tx_sex;
    ic;
    oc;
    rmap = VleMap.empty; 
    mask = mask;
    sid = txid tx_sex;
    pending_pull = ResMap.empty; 
    stats = create_stats ();
  }

let in_channel s = s.ic
let out_channel s = s.oc
let tx_sex s = s.tx_sex
let id s = txid s.tx_sex
let is_broker s = Message.ScoutFlags.hasFlag s.mask Message.ScoutFlags.scoutBroker