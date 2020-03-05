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
open Ztypes
open NetService
open R_name
open Engine_state

let final_reply q = Message.Reply.create (Message.Query.pid q) (Message.Query.qid q) Storage None

let forward_query_to_txsex pe q txsex =
  let open Lwt.Infix in 
  (Mcodec.ztcp_safe_write_frame_pooled txsex @@ Frame.Frame.create [Query(q)]) pe.buffer_pool >>= fun _ -> Lwt.return_unit

let forward_query_to_session pe q sid =
  match SIDMap.find_opt sid pe.smap with
  | None -> let%lwt _ = Logs_lwt.debug (fun m -> m  "Unable to forward query to unknown session %s" (Id.to_string sid)) in Lwt.return_unit
  | Some s ->
    let%lwt _ = Logs_lwt.debug (fun m -> m  "Forwarding query to session %s" (Id.to_string s.sid)) in
    forward_query_to_txsex pe q s.tx_sex

let forward_reply_to_session pe r sid =
  match SIDMap.find_opt sid pe.smap with
  | None -> let%lwt _ = Logs_lwt.debug (fun m -> m  "Unable to forward reply to unknown session %s" (Id.to_string sid)) in Lwt.return_unit
  | Some s ->
    let%lwt _ = Logs_lwt.debug (fun m -> m  "Forwarding reply to session %s" (Id.to_string s.sid)) in
    let open Lwt.Infix in 
    (Mcodec.ztcp_safe_write_frame_pooled s.tx_sex @@ Frame.Frame.create [Reply(r)]) pe.buffer_pool >>= fun _ -> Lwt.return_unit

let forward_replies_to_session pe rs sid =
  match SIDMap.find_opt sid pe.smap with
  | None -> let%lwt _ = Logs_lwt.debug (fun m -> m  "Unable to forward reply to unknown session %s" (Id.to_string sid)) in Lwt.return_unit
  | Some s ->
    let%lwt _ = Logs_lwt.debug (fun m -> m  "Forwarding %i replies to session %s" (List.length rs)(Id.to_string s.sid)) in
    let open Lwt.Infix in 
    List.map (fun r -> 
      (Mcodec.ztcp_safe_write_frame_pooled s.tx_sex @@ Frame.Frame.create [Reply(r)]) pe.buffer_pool >>= fun _ -> Lwt.return_unit
    ) rs |> Lwt.join
    
let forward_query_to pe q = List.map (fun s -> forward_query_to_session pe q s)

let forward_admin_query pe sid q = 
  let open Lwt.Infix in 
  let faces = SIDMap.bindings pe.smap 
    |> List.filter (fun (id, s) -> id <> sid && (id < Id.zero || Session.is_broker s))
    |> List.split |> fst
  in 
  Lwt.join @@ forward_query_to pe q faces >>= fun _ -> Lwt.return faces

type dest_kind = | Storages | Evals
type coverness = | Covering | Matching

let forward_user_query pe sid q = 
  let open Resource in 
  let open Lwt.Infix in 
  let destStorages = match ZProperty.DestStorages.find_opt (Message.Query.properties q) with 
  | None -> Best_match
  | Some prop -> ZProperty.DestStorages.dest prop in
  let destEvals = match ZProperty.DestEvals.find_opt (Message.Query.properties q) with 
  | None -> Best_match
  | Some prop -> ZProperty.DestEvals.dest prop in 

  let sub_get_faces coverness dest_kind = 
    ResMap.fold (fun _ res accu -> match res.name with 
        | ID _ -> accu
        | Path path -> 
          match coverness with 
          | Covering ->
            (match PathExpr.includes ~subexpr:(PathExpr.of_string @@ Message.Query.resource q) path with 
              | false -> accu 
              | true -> 
                List.fold_left (fun accu m -> 
                    match m.session != sid, dest_kind, m.sto, m.eval with
                    | true, Storages, Some _, _ -> (m.session, Option.get m.sto) :: accu
                    | true, Evals, _, Some _ -> (m.session, Option.get m.eval) :: accu
                    | _-> accu
                  ) accu res.mappings)
          | Matching -> 
            match ResName.name_match (ResName.Path(PathExpr.of_string @@ Message.Query.resource q)) res.name with 
            | false -> accu 
            | true -> 
                List.fold_left (fun accu m -> 
                    match m.session != sid, dest_kind, m.sto, m.eval with
                    | true, Storages, Some _, _ -> (m.session, Option.get m.sto) :: accu
                    | true, Evals, _, Some _ -> (m.session, Option.get m.eval) :: accu
                    | _-> accu
                  ) accu res.mappings) pe.rmap [] in

  let get_faces dest kind = match dest with 
    | All -> sub_get_faces Matching kind |> List.split |> fst
    | Complete _ -> sub_get_faces Covering kind |> List.split |> fst
    (* TODO : manage quorum *)
    | Best_match -> 
      (match sub_get_faces Covering kind with
        | [] -> sub_get_faces Matching kind |> List.split |> fst
        | faces -> 
          let (nearest_face, _) = List.fold_left (fun (accu, accudist) (s, sdist) -> 
              match sdist < accudist with 
              | true -> (s, sdist)
              | false -> (accu, accudist)) (List.hd faces) faces in 
          [nearest_face]) 
    | No -> [] in
  let storage_faces = get_faces destStorages Storages in
  let eval_faces = get_faces destEvals Evals in
  let faces = List.append storage_faces eval_faces in 
  let faces = List.sort_uniq (fun s1 s2 -> match s1 == s2 with | true -> 0 | false -> 1 ) faces in

  forward_query_to pe q faces |> Lwt.join >>= fun () -> Lwt.return faces

let store_query pe srcFace fwdFaces q =
  let open Query in
  let qmap = QIDMap.add (Message.Query.pid q, Message.Query.qid q) {srcFace; fwdFaces} pe.qmap in 
  {pe with qmap}

let find_query pe q = QIDMap.find_opt (Message.Query.pid q, Message.Query.qid q) pe.qmap

let local_replies = Admin.replies

let process_admin_query pe session q = 
  let open Session in 
  let open Lwt.Infix in
  match find_query pe q with 
  | None -> (
    let%lwt local_replies = local_replies pe q in
    forward_replies_to_session pe local_replies session.sid >>= fun _ ->
    forward_admin_query pe session.sid q >>= function
    | [] -> forward_reply_to_session pe (final_reply q) session.sid >>= fun _ -> Lwt.return pe
    | ss -> Lwt.return @@ store_query pe session.sid ss q )
  | Some _ -> forward_reply_to_session pe (final_reply q) session.sid >>= fun _ -> Lwt.return pe

let process_user_query pe session q = 
  let open Session in 
  let open Lwt.Infix in
  match find_query pe q with 
  | None -> (
    let%lwt local_replies = local_replies pe q in
    forward_replies_to_session pe local_replies session.sid >>= fun _ ->
    forward_user_query pe session.sid q >>= function
    | [] -> forward_reply_to_session pe (final_reply q) session.sid >>= fun _ -> Lwt.return pe
    | ss -> Lwt.return @@ store_query pe session.sid ss q )
  | Some _ -> 
    let%lwt _ = Logs_lwt.debug (fun m -> m "Ignore duplicate query.") in
    Lwt.return pe

let process_query engine tsex q = 
  Guard.guarded engine @@ fun pe -> 
  let open Session in 
  let sid = Session.txid tsex in
  let session = SIDMap.find_opt sid pe.smap in 
  let%lwt pe = match session with 
    | None -> let%lwt _ = Logs_lwt.warn (fun m -> m "Received Query on unknown session %s: Ignore it!" (Id.to_string sid)) in Lwt.return pe
    | Some session -> 
      let%lwt _ = Logs_lwt.debug (fun m -> 
          let nid = match List.find_opt (fun (peer:Spn_trees_mgr.peer) -> 
              Session.txid peer.tsex = session.sid) pe.trees.peers with 
          | Some peer -> peer.pid
          | None -> "UNKNOWN" in
          m "Handling Query Message. nid[%s] sid[%s] pid[%s] qid[%d] res[%s]" 
            nid (Id.to_string session.sid) (Abuf.hexdump (Message.Query.pid q)) (Int64.to_int (Message.Query.qid q)) (Message.Query.resource q)) in
      match Admin.is_admin q with 
        | true -> process_admin_query pe session q
        | false -> process_user_query pe session q
  in
  Guard.return [] pe

let process_reply engine tsex r = 
  let open Lwt.Infix in
  Guard.guarded engine @@ fun pe -> 
  let qid = Message.Reply.(qpid r, qid r) in 
  let%lwt pe = match QIDMap.find_opt qid pe.qmap with 
    | None -> Logs_lwt.debug (fun m -> m  "Received reply for unknown query. Ingore it!") >>= fun _ -> Lwt.return pe
    | Some qs -> 
      let%lwt _ = Logs_lwt.debug (fun m -> 
          let nid = match List.find_opt (fun (peer:Spn_trees_mgr.peer) -> 
              Session.txid peer.tsex = (Session.txid tsex)) pe.trees.peers with 
          | Some peer -> peer.pid
          | None -> "UNKNOWN" in
          let h = Message.Block.header r in
          let kind, resource = match Message.Flags.
            (hasFlag h fFlag, hasFlag h eFlag, Message.Reply.resource r) with 
          | true, false, None -> "StorageFinal", ""
          | true, false, Some "" -> "StorageFinal", ""
          | true, false, Some res -> "StorageData", res
          | true, true, None -> "EvalFinal", ""
          | true, true, Some "" -> "EvalFinal", ""
          | true, true, Some res -> "EvalData", res
          | false, _, _ -> "ReplyFinal", "" in
          m "Handling Reply Message. nid[%s] sid[%s] qpid[%s] qid[%d] %s %s" 
            nid (Id.to_string (Session.txid tsex)) 
            (Abuf.hexdump (Message.Reply.qpid r)) (Int64.to_int (Message.Reply.qid r))
            kind resource) in
      (match Message.Reply.value r with 
       | Some _ -> forward_reply_to_session pe r qs.srcFace >>= fun _ -> Lwt.return pe
       | None -> 
         let fwdFaces = List.filter (fun face -> 
             match SIDMap.find_opt face pe.smap with 
             | Some s -> s.tx_sex != tsex
             | None -> true) qs.fwdFaces in 
         let%lwt qmap = match fwdFaces with 
           | [] -> forward_reply_to_session pe r qs.srcFace >>= fun _ -> Lwt.return @@ QIDMap.remove qid pe.qmap
           | _ -> Lwt.return @@ QIDMap.add qid {qs with fwdFaces} pe.qmap in 
         Lwt.return {pe with qmap} ) in
  Guard.return [] pe
