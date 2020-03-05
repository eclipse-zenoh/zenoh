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
open Lwt.Infix
open Apero.Infix
open Apero
open Channel
open NetService
open R_name
open Engine_state
open Discovery

let store_data pe sid resname payload = 
  match SIDMap.find_opt sid pe.smap with
  | None -> pe
  | Some s -> 
    let pending_pull = ResMap.add resname payload s.pending_pull in
    let s = {s with pending_pull} in
    let smap = SIDMap.add sid s pe.smap in
    {pe with smap}

let forward_data_to_mapping pe srcresname dstres dstmapsession dstmapid reliable payload =
  let open Resource in 
  let open ResName in 
  match SIDMap.find_opt dstmapsession pe.smap with
  | None -> Lwt.return_unit
  | Some s ->
    let%lwt _ = Logs_lwt.debug (fun m -> m  "Forwarding data to session %s" (Id.to_string s.sid)) in
    let oc = Session.out_channel s in
    let fsn = if reliable then OutChannel.next_rsn oc else  OutChannel.next_usn oc in
    let msgs = match srcresname with 
      | ID id -> [Message.CompactData(Message.CompactData.create (true, reliable) fsn id None payload)]
      | Path uri -> match srcresname = dstres.name with 
        | true -> [Message.StreamData(Message.StreamData.create (true, reliable) fsn dstmapid payload)]
        | false -> [Message.WriteData(Message.WriteData.create (true, reliable) fsn (PathExpr.to_string uri) payload)] 
    in
    Session.add_out_msg s.stats;
    (Mcodec.ztcp_safe_write_frame_pooled s.tx_sex @@ Frame.Frame.create msgs) pe.buffer_pool >>= fun _ -> Lwt.return_unit

let forward_batched_data_to_mapping pe srcresname dstres dstmapsession dstmapid reliable payloads =
  let open Resource in 
  let open ResName in 
  match SIDMap.find_opt dstmapsession pe.smap with
  | None -> Lwt.return_unit
  | Some s ->
    let%lwt _ = Logs_lwt.debug (fun m -> m  "Forwarding batched data to session %s" (Id.to_string s.sid)) in
    let oc = Session.out_channel s in
    let fsn = if reliable then OutChannel.next_rsn oc else  OutChannel.next_usn oc in
    let msgs = match srcresname with 
      | ID id -> [Message.BatchedStreamData(Message.BatchedStreamData.create (true, reliable) fsn id payloads)]
      | Path uri -> match srcresname = dstres.name with 
        | true -> [Message.BatchedStreamData(Message.BatchedStreamData.create (true, reliable) fsn dstmapid payloads)]
        | false -> List.map (fun p -> Message.WriteData(Message.WriteData.create (true, reliable) fsn (PathExpr.to_string uri) p)) payloads
    in
    Session.add_out_msg s.stats;
    (Mcodec.ztcp_safe_write_frame_pooled s.tx_sex @@ Frame.Frame.create msgs) pe.buffer_pool >>= fun _ -> Lwt.return_unit

let forward_data pe sid srcres reliable payload = 
  let open Resource in
  let dstsexs = List.fold_left (fun dstsexs name -> 
      match ResMap.find_opt name pe.rmap with 
      | None -> dstsexs
      | Some r -> 
        List.fold_left (fun dstsexs m ->
          match (m.sub != None || m.sto != None) && m.session != sid with 
          | true -> 
            let entry = SIDMap.find_opt m.session dstsexs in
            let newentry = match entry, (m.sub = Some false || m.sto != None) with 
            | None, true -> (r, m, true)
            | None, false -> (r, m, false)
            | Some (exr, exm, expush), push -> 
              match srcres.name = r.name, expush, push with 
              | true, false, false -> (r, m, false)
              | true, _, _ -> (r, m, true)
              | false, false, false -> (exr, exm, false)
              | _ -> (exr, exm, true)
            in 
            SIDMap.add m.session newentry dstsexs
          | false -> dstsexs
        ) dstsexs r.mappings 
    ) SIDMap.empty srcres.matches in 
  let pe, ps = SIDMap.fold (fun _ (r, m, push) (pe, ps) -> 
    match push with 
    | true -> pe, forward_data_to_mapping pe srcres.name r m.session m.id reliable payload :: ps
    | false -> store_data pe m.session srcres.name payload, ps
  ) dstsexs (pe, []) in
  let%lwt _ = Lwt.join ps in 
  Lwt.return pe

let forward_batched_data pe sid srcres reliable payloads = 
  let open Resource in
  let dstsexs = List.fold_left (fun dstsexs name -> 
      match ResMap.find_opt name pe.rmap with 
      | None -> dstsexs
      | Some r -> 
        List.fold_left (fun dstsexs m ->
          match (m.sub != None || m.sto != None) && m.session != sid with 
          | true -> 
            let entry = SIDMap.find_opt m.session dstsexs in
            let newentry = match entry, (m.sub = Some false || m.sto != None) with 
            | None, true -> (r, m, true)
            | None, false -> (r, m, false)
            | Some (exr, exm, expush), push -> 
              match srcres.name = r.name, expush, push with 
              | true, false, false -> (r, m, false)
              | true, _, _ -> (r, m, true)
              | false, false, false -> (exr, exm, false)
              | _ -> (exr, exm, true)
            in 
            SIDMap.add m.session newentry dstsexs
          | false -> dstsexs
        ) dstsexs r.mappings 
    ) SIDMap.empty srcres.matches in 
  let pe, ps = SIDMap.fold (fun _ (r, m, push) (pe, ps) -> 
    match push with 
    | true -> pe, forward_batched_data_to_mapping pe srcres.name r m.session m.id reliable payloads :: ps
    | false -> let last = List.nth payloads ((List.length payloads) - 1) in store_data pe m.session srcres.name last, ps
  ) dstsexs (pe, []) in
  let%lwt _ = Lwt.join ps in 
  Lwt.return pe

let forward_oneshot_data pe sid srcresname reliable payload = 
  let open Resource in 
  let dstsexs =  ResMap.fold (fun _ r dstsexs -> 
    match ResName.name_match srcresname r.name with 
    | false -> dstsexs
    | true -> 
      List.fold_left (fun dstsexs m ->
        match (m.sub != None || m.sto != None) && m.session != sid with 
        | true -> 
          let entry = SIDMap.find_opt m.session dstsexs in
          let newentry = match entry, (m.sub = Some false || m.sto != None) with 
          | None, true -> (r, m, true)
          | None, false -> (r, m, false)
          | Some (exr, exm, expush), push -> 
            match srcresname = r.name, expush, push with 
            | true, false, false -> (r, m, false)
            | true, _, _ -> (r, m, true)
            | false, false, false -> (exr, exm, false)
            | _ -> (exr, exm, true)
          in 
          SIDMap.add m.session newentry dstsexs
        | false -> dstsexs
      ) dstsexs r.mappings 
    ) pe.rmap SIDMap.empty in 
  let pe, ps = SIDMap.fold (fun _ (r, m, push) (pe, ps) -> 
    match push with 
    | true -> pe, forward_data_to_mapping pe srcresname r m.session m.id reliable payload :: ps
    | false -> store_data pe m.session srcresname payload, ps
  ) dstsexs (pe, []) in
  let%lwt _ = Lwt.join ps in 
  Lwt.return pe

let stamp_payload pe p = 
  match pe.timestamp with 
  | false -> Lwt.return p 
  | true -> match (Payload.header p).ts with 
    | Some _ -> Lwt.return p 
    | None -> (Ztypes.HLC.new_timestamp pe.hlc) >>= (Payload.with_timestamp p %> Lwt.return)

let process_user_compactdata (pe:engine_state) session msg =      
  let open Session in
  let open Resource in
  let rid = Message.CompactData.id msg in
  let name = match VleMap.find_opt rid session.rmap with 
    | None -> (match ResMap.bindings pe.rmap |> List.find_opt (fun (_, res) -> res.local_id = rid) with 
        | None -> ResName.ID(rid)
        | Some (_, res) -> res.name)
    | Some name -> name in 
  match ResMap.find_opt name pe.rmap with 
  | None -> let%lwt _ = Logs_lwt.warn (fun m -> m "Received CompactData for unknown resource %s on session %s: Ignore it!" 
                                                (ResName.to_string name) (Id.to_string session.sid)) in Lwt.return (pe, [])
  | Some res -> 
    let%lwt _ = Logs_lwt.debug (fun m -> 
        let nid = match List.find_opt (fun (peer:Spn_trees_mgr.peer) -> 
            Session.txid peer.tsex = session.sid) pe.trees.peers with 
        | Some peer -> peer.pid
        | None -> "UNKNOWN" in
        m "Handling CompactData Message. nid[%s] sid[%s] rid[%Ld] res[%s]"
          nid (Id.to_string session.sid) rid (match res.name with Path u -> PathExpr.to_string u | ID _ -> "UNNAMED")) in
    let%lwt payload = stamp_payload pe (Message.CompactData.payload msg) in
    let%lwt pe = forward_data pe session.sid res (Message.Reliable.reliable msg) payload in
    Lwt.return (pe, [])

let process_user_streamdata (pe:engine_state) session msg =      
  let open Session in
  let open Resource in
  let rid = Message.StreamData.id msg in
  let name = match VleMap.find_opt rid session.rmap with 
    | None -> (match ResMap.bindings pe.rmap |> List.find_opt (fun (_, res) -> res.local_id = rid) with 
        | None -> ResName.ID(rid)
        | Some (_, res) -> res.name)
    | Some name -> name in 
  match ResMap.find_opt name pe.rmap with 
  | None -> let%lwt _ = Logs_lwt.warn (fun m -> m "Received CompactData for unknown resource %s on session %s: Ignore it!" 
                                                (ResName.to_string name) (Id.to_string session.sid)) in Lwt.return (pe, [])
  | Some res -> 
    let%lwt _ = Logs_lwt.debug (fun m -> 
        let nid = match List.find_opt (fun (peer:Spn_trees_mgr.peer) -> 
            Session.txid peer.tsex = session.sid) pe.trees.peers with 
        | Some peer -> peer.pid
        | None -> "UNKNOWN" in
        m "Handling StreamData Message. nid[%s] sid[%s] rid[%Ld] res[%s]"
          nid (Id.to_string session.sid) rid (match res.name with Path u -> PathExpr.to_string u | ID _ -> "UNNAMED")) in
    let%lwt payload = stamp_payload pe (Message.StreamData.payload msg) in
    let%lwt pe = forward_data pe session.sid res (Message.Reliable.reliable msg) payload in
    Lwt.return (pe, [])

let process_user_batched_streamdata (pe:engine_state) session msg =      
  let open Session in
  let open Resource in
  let rid = Message.BatchedStreamData.id msg in
  let name = match VleMap.find_opt rid session.rmap with 
    | None -> (match ResMap.bindings pe.rmap |> List.find_opt (fun (_, res) -> res.local_id = rid) with 
        | None -> ResName.ID(rid)
        | Some (_, res) -> res.name)
    | Some name -> name in 
  let bufs = Message.BatchedStreamData.payload msg in 
  match ResMap.find_opt name pe.rmap with 
  | None -> let%lwt _ = Logs_lwt.warn (fun m -> m "Received BatchedData for unknown resource %s on session %s: Ignore it!" 
                                                (ResName.to_string name) (Id.to_string session.sid)) in Lwt.return (pe, [])
  | Some res -> 
    let%lwt _ = Logs_lwt.debug (fun m -> 
        let nid = match List.find_opt (fun (peer:Spn_trees_mgr.peer) -> 
            Session.txid peer.tsex = session.sid) pe.trees.peers with 
        | Some peer -> peer.pid
        | None -> "UNKNOWN" in
        m "Handling BatchedData Message. nid[%s] sid[%s] rid[%Ld] res[%s]"
          nid (Id.to_string session.sid) rid (match res.name with Path u -> PathExpr.to_string u | ID _ -> "UNNAMED")) in
    let%lwt stamped_bufs = Lwt_list.map_s (stamp_payload pe) bufs in
    let%lwt pe = forward_batched_data pe session.sid res (Message.Reliable.reliable msg) stamped_bufs in
    Lwt.return (pe, [])

let process_user_writedata pe session msg =      
  let open Session in 
  let%lwt _ = Logs_lwt.debug (fun m -> 
      let nid = match List.find_opt (fun (peer:Spn_trees_mgr.peer) -> 
          Session.txid peer.tsex = session.sid) pe.trees.peers with 
      | Some peer -> peer.pid
      | None -> "UNKNOWN" in
      m "Handling WriteData Message. nid[%s] sid[%s] res[%s] [%s]" 
        nid (Id.to_string session.sid) (Message.WriteData.resource msg) (Abuf.hexdump (Payload.buffer (Message.WriteData.payload msg)))) in
  let name = ResName.Path(PathExpr.of_string @@ Message.WriteData.resource msg) in
  let%lwt payload = stamp_payload pe (Message.WriteData.payload msg) in
  match ResMap.find_opt name pe.rmap with 
  | None -> 
    let%lwt pe = forward_oneshot_data pe session.sid name (Message.Reliable.reliable msg) payload in
    Lwt.return (pe, [])
  | Some res -> 
    let%lwt pe = forward_data pe session.sid res (Message.Reliable.reliable msg) payload in
    Lwt.return (pe, [])


let process_pull engine tsex msg =
  Guard.guarded engine @@ fun pe ->
  let sid = Session.txid tsex in 
  let session = SIDMap.find_opt sid pe.smap in 
    let%lwt pe = match session with 
  | None -> let%lwt _ = Logs_lwt.warn (fun m -> m "Received Pull on unknown session %s: Ignore it!" 
                                            (Id.to_string sid)) in Lwt.return pe
  | Some session -> 
    let rid = Message.Pull.id msg in
    let name = match VleMap.find_opt rid session.rmap with 
      | None -> ResName.ID(rid)
      | Some name -> name in 
    match ResMap.find_opt name pe.rmap with 
    | None -> let%lwt _ = Logs_lwt.warn (fun m -> m "Received Pull for unknown resource %s on session %s: Ignore it!" 
                                              (ResName.to_string name) (Id.to_string session.sid)) in Lwt.return pe
    | Some res -> 
      let%lwt _ = Logs_lwt.debug (fun m -> m "Handling Pull Message for resource: [%s:%Ld] (%s)" 
                                     (Id.to_string session.sid) rid (match res.name with Path u -> PathExpr.to_string u | ID _ -> "UNNAMED")) in
        let%lwt pending_pull = ResMap.bindings session.pending_pull |> Lwt_list.fold_left_s (fun pending (name, payload) -> 
          match ResName.name_match res.name name with 
          | true -> let%lwt _ = forward_data_to_mapping pe name res sid rid true payload in Lwt.return pending
          | false -> Lwt.return @@ ResMap.add name payload pending
        ) ResMap.empty in 
        let session = {session with pending_pull} in 
        let smap = SIDMap.add session.sid session pe.smap in 
        Lwt.return {pe with smap}
    in
    Guard.return [] pe

let rspace msg =       
  let open Message in 
  List.fold_left (fun res marker -> 
      match marker with 
      | RSpace rs -> RSpace.id rs 
      | _ -> res) 0L (markers msg)

let process_broker_data pe session msg = 
  let open Session in      
  let%lwt _ = Logs_lwt.debug (fun m -> m "Received tree state on %s\n" (Id.to_string session.sid)) in
  let pl = Message.CompactData.payload msg |> Payload.data in
  let b = Abuf.read_bytes (Abuf.readable_bytes pl) pl in
  let node = Marshal.from_bytes b 0 in
  let pe = {pe with trees = Spn_trees_mgr.update pe.trees node} in
  let%lwt _ = Logs_lwt.debug (fun m -> m "Spanning trees status :\n%s" (Spn_trees_mgr.report pe.trees)) in
  forward_all_decls pe;
  Lwt.return (pe, []) 

let process_compact_data engine tsex msg =
  Guard.guarded engine @@ fun pe ->
  let%lwt (pe, ms) = 
    let sid = Session.txid tsex in 
    let session = SIDMap.find_opt sid pe.smap in 
    match session with 
    | None -> let%lwt _ = Logs_lwt.warn (fun m -> m "Received CompactData on unknown session %s: Ignore it!" 
                                            (Id.to_string sid)) in Lwt.return (pe, [])
    | Some session -> 
      match rspace (Message.CompactData(msg)) with 
      | 1L -> process_broker_data pe session msg
      | _ -> process_user_compactdata pe session msg
  in Guard.return ms pe

let process_batched_stream_data engine tsex msg =
  Guard.guarded engine @@ fun pe ->
  let%lwt (pe, ms) = 
    let sid = Session.txid tsex in 
    let session = SIDMap.find_opt sid pe.smap in 
    match session with 
    | None -> let%lwt _ = Logs_lwt.warn (fun m -> m "BatchedReceived StreamData on unknown session %s: Ignore it!" 
                                            (Id.to_string sid)) in Lwt.return (pe, [])
    | Some session -> 
      match rspace (Message.BatchedStreamData(msg)) with 
      (* TODO: Should decide if broker will ever send  batched data *)
      | 1L -> Lwt.return (pe, []) 
      | _ -> process_user_batched_streamdata pe session msg
  in Guard.return ms pe

let process_write_data engine tsex msg =
  Guard.guarded engine @@ fun pe ->
  let%lwt (pe, ms) = 
    let sid = Session.txid tsex in
    let session = SIDMap.find_opt sid pe.smap in 
    match session with 
    | None -> let%lwt _ = Logs_lwt.warn (fun m -> m "Received WriteData on unknown session %s: Ignore it!" 
                                            (Id.to_string sid)) in Lwt.return (pe, [])
    | Some session -> 
      match rspace (Message.WriteData(msg)) with 
      | 1L -> Lwt.return (pe, []) 
      | _ -> process_user_writedata pe session msg
  in Guard.return ms pe

let process_stream_data engine tsex msg =
  Guard.guarded engine @@ fun pe ->
  let%lwt (pe, ms) = 
    let sid = Session.txid tsex in 
    let session = SIDMap.find_opt sid pe.smap in 
    match session with 
    | None -> let%lwt _ = Logs_lwt.warn (fun m -> m "Received StreamData on unknown session %s: Ignore it!" 
                                            (Id.to_string sid)) in Lwt.return (pe, [])
    | Some session -> 
      match rspace (Message.StreamData(msg)) with 
      | 1L -> Lwt.return (pe, []) (* Should never happen *)
      | _ -> process_user_streamdata pe session msg
  in Guard.return ms pe
