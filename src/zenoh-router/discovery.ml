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
open Engine_state


    let next_mapping pe = 
        let next = pe.next_mapping in
        ({pe with next_mapping = Vle.add next 1L}, next)

    let res_name (s:Session.t) rid = match VleMap.find_opt rid s.rmap with 
        | Some name -> name
        | None -> ID(rid)

    let match_resource rmap mres = 
        let open Resource in
        match mres.name with 
        | Path _ -> (
            ResMap.fold (fun _ res x -> 
                let (rmap, mres) = x in
                match res_match mres res with 
                | true -> 
                    let mres = with_match mres res.name in 
                    let rmap = ResMap.add res.name (with_match res mres.name) rmap in 
                    (rmap, mres)
                | false -> x) 
            rmap (rmap, mres))
        | ID _ -> (rmap, with_match mres mres.name)

    let update_resource_opt pe name updater = 
        let optres = ResMap.find_opt name pe.rmap in 
        let optres' = updater optres in
        match optres' with 
        | Some res -> 
        let (rmap, res') = match optres with 
            | None -> match_resource pe.rmap res
            | Some _ -> (pe.rmap, res) in
        let rmap = ResMap.add res'.name res' rmap in 
            ({pe with rmap}, Some res')
        | None -> (pe, None)

    let update_resource pe name updater = 
        let (pe, optres) = update_resource_opt pe name (fun ores -> Some (updater ores)) in 
        (pe, Option.get optres)

    let update_resource_mapping_opt pe name (session:Session.t) rid updater =   
        let open ResName in    
        let (pe, local_id) = match name with 
        | Path _ -> next_mapping pe
        | ID id -> (pe, id) in
        let(pe, res) = update_resource_opt pe name 
            (fun r -> match r with 
                | Some res -> Some (Resource.update_mapping_opt res (Session.id session) updater)
                | None -> (match updater None with 
                    | Some m -> Some {name; mappings=[m]; matches=[name]; local_id}
                    | None -> None)) in
        match res with 
        | Some res -> 
            let sid = Session.id session in 
            Logs.debug (fun m -> m "Register resource '%s' mapping [sid : %s, rid : %d]" (ResName.to_string res.name) (Id.to_string sid) (Vle.to_int rid));
            let session = {session with rmap=VleMap.add rid res.name session.rmap;} in      
            let smap = SIDMap.add session.sid session pe.smap in
            ({pe with smap}, Some res)
        | None -> (pe, None)

    let update_resource_mapping pe name (session:Session.t) rid updater = 
        let (pe, optres) = update_resource_mapping_opt pe name session rid (fun ores -> Some (updater ores)) in 
        (pe, Option.get optres)

    let declare_resource pe s rd =
        let rid = Message.ResourceDecl.rid rd in 
        let uri = Message.ResourceDecl.resource rd in 
        let (pe, _) = update_resource_mapping pe (Path(PathExpr.of_string uri)) s rid 
            (fun m -> match m with 
                | Some mapping -> mapping
                | None -> Resource.create_mapping rid s.sid) in 
        Lwt.return pe

    (* ======================== PUB DECL =========================== *)

    let match_pdecl pe pr id (s:Session.t) =
        let open Resource in 
        let pm = List.find (fun m -> m.session = s.sid) pr.mappings in
        match pm.matched_pub with 
        | true -> Lwt.return (pe, [])
        | false -> 
        match ResMap.exists 
                (fun _ sr ->  res_match pr sr && List.exists 
                                (fun m -> (m.sub != None || m.sto != None) && m.session != s.sid) sr.mappings) pe.rmap with
        | false -> Lwt.return (pe, [])
        | true -> 
            let pm = {pm with matched_pub = true} in
            let pr = with_mapping pr pm in
            let rmap = ResMap.add pr.name pr pe.rmap in
            let pe = {pe with rmap} in
            Lwt.return (pe, [Message.Declaration.SubscriberDecl (Message.SubscriberDecl.create id Message.SubscriptionMode.push_mode [])])

    let register_publication pe (s:Session.t) pd =
        let rid = Message.PublisherDecl.rid pd in 
        let resname = res_name s rid in
        let (pe, res) = update_resource_mapping pe resname s rid 
            (fun m -> match m with 
                | Some m -> {m with pub=true;} 
                | None -> {(Resource.create_mapping rid s.sid) with pub = true;}) in
        Lwt.return (pe, Some res)

    let process_pdecl pe s pd =      
        let%lwt (pe, pr) = register_publication pe s pd in
        match pr with 
        | None -> Lwt.return (pe, [])
        | Some pr -> 
            let id = Message.PublisherDecl.rid pd in
            match_pdecl pe pr id s

    (* ======================== SUB DECL =========================== *)

    let forward_sdecl_to_session pe res zsex =       
        let module M = Message in
        let open Resource in 
        let oc = Session.out_channel zsex in
        let (pe, ds) = match res.name with 
        | ID id -> (
            let subdecl = M.Declaration.SubscriberDecl M.(SubscriberDecl.create id SubscriptionMode.push_mode []) in
            (pe, [subdecl]))
        | Path uri -> 
            let resdecl = M.Declaration.ResourceDecl (M.ResourceDecl.create res.local_id (PathExpr.to_string uri) []) in
            let subdecl = M.Declaration.SubscriberDecl (M.SubscriberDecl.create res.local_id M.SubscriptionMode.push_mode []) in
            let (pe, _) = update_resource_mapping pe res.name zsex res.local_id 
                (fun m -> match m with 
                    | Some mapping -> mapping
                    | None -> Resource.create_mapping res.local_id (Session.id zsex)) in 
            let rmap = VleMap.add res.local_id res.name zsex.rmap in
            let session = {zsex with rmap;} in
            let smap = SIDMap.add session.sid session pe.smap in
            ({pe with smap}, [resdecl; subdecl]) in
        let decl = M.Declare (M.Declare.create (true, true) (OutChannel.next_rsn oc) ds) in
        let open Lwt.Infix in 
        (pe, Mcodec.ztcp_safe_write_frame_pooled (Session.tx_sex zsex) (Frame.Frame.create [decl]) pe.buffer_pool>|= fun _ -> ())

    let forget_sdecl_to_session pe res zsex =       
        let module M = Message in
        let open Resource in 
        let oc = Session.out_channel zsex in
        let (pe, ds) = match res.name with 
        | ID id -> (
            let fsubdecl = M.Declaration.ForgetSubscriberDecl M.(ForgetSubscriberDecl.create id) in
            (pe, [fsubdecl]))
        | Path uri -> 
            let resdecl = M.Declaration.ResourceDecl (M.ResourceDecl.create res.local_id (PathExpr.to_string uri) []) in
            let fsubdecl = M.Declaration.ForgetSubscriberDecl M.(ForgetSubscriberDecl.create res.local_id) in
            let (pe, _) = update_resource_mapping pe res.name zsex res.local_id 
                (fun m -> match m with 
                    | Some mapping -> mapping
                    | None -> Resource.create_mapping res.local_id (Session.id zsex)) in 
            let rmap = VleMap.add res.local_id res.name zsex.rmap in
            let session = {zsex with rmap;} in
            let smap = SIDMap.add session.sid session pe.smap in
            ({pe with smap}, [resdecl; fsubdecl]) in
        let decl = M.Declare (M.Declare.create (true, true) (OutChannel.next_rsn oc) ds) in
        let open Lwt.Infix in 
        (pe, Mcodec.ztcp_safe_write_frame_pooled (Session.tx_sex zsex) (Frame.Frame.create [decl]) pe.buffer_pool>|= fun _ -> ())


    let forward_sdecl pe res trees =  
        let open Spn_trees_mgr in
        let open Resource in 
        let subs = List.filter (fun map -> map.sub != None) res.mappings in 
        let (pe, ps) = (match subs with 
        | [] -> 
            SIDMap.fold (fun _ session (pe, ps) -> 
                match Session.is_broker session with 
                | true -> let (pe, p) = forget_sdecl_to_session pe res session in (pe, p::ps)
                | false -> (pe, ps)) pe.smap (pe, [])
        | sub :: [] -> 
            (match SIDMap.find_opt sub.session pe.smap with 
            | None -> (pe, [])
            | Some subsex ->
            let tsex = Session.tx_sex subsex in
            let module TreeSet = (val trees.tree_mod : Spn_tree.Set.S) in
            let tree0 = Option.get (TreeSet.get_tree trees.tree_set 0) in
            let (pe, ps) = (match TreeSet.get_parent tree0 with 
            | None -> TreeSet.get_childs tree0 
            | Some parent -> parent :: TreeSet.get_childs tree0 )
            |> List.map (fun (node:Spn_tree.Node.t) -> 
                (List.find_opt (fun peer -> peer.pid = node.node_id) trees.peers))
            |> List.filter (fun opt -> opt != None)
            |> List.map (fun peer -> (Option.get peer).tsex)
            |> List.fold_left (fun x stsex -> 
                if(tsex != stsex)
                then 
                    begin
                    let (pe, ps) = x in
                    let s = Option.get @@ SIDMap.find_opt (Session.txid stsex) pe.smap in
                    let (pe, p) = forward_sdecl_to_session pe res s in 
                    (pe, p :: ps)
                    end
                else x
                ) (pe, []) in 
            let (pe, ps) = match Session.is_broker subsex with
            | false -> (pe, ps)
            | true -> let (pe, p) = forget_sdecl_to_session pe res subsex in (pe, p::ps) in 
            TreeSet.get_broken_links tree0 
            |> List.map (fun (node:Spn_tree.Node.t) -> 
                (List.find_opt (fun peer -> peer.pid = node.node_id) trees.peers))
            |> List.filter (fun opt -> opt != None)
            |> List.map (fun peer -> (Option.get peer).tsex)
            |> List.fold_left (fun x stsex -> 
                if(tsex != stsex)
                then 
                    begin
                    let (pe, ps) = x in
                    let s = Option.get @@ SIDMap.find_opt (Session.txid stsex) pe.smap in
                    let (pe, p) = forget_sdecl_to_session pe res s in 
                    (pe, p :: ps)
                    end
                else x
                ) (pe, ps))
        | _ -> 
            let module TreeSet = (val trees.tree_mod : Spn_tree.Set.S) in
            let tree0 = Option.get (TreeSet.get_tree trees.tree_set 0) in
            let (pe, ps) = (match TreeSet.get_parent tree0 with 
            | None -> TreeSet.get_childs tree0 
            | Some parent -> parent :: TreeSet.get_childs tree0 )
            |> List.map (fun (node:Spn_tree.Node.t) -> 
                (List.find_opt (fun peer -> peer.pid = node.node_id) trees.peers))
            |> List.filter (fun opt -> opt != None)
            |> List.map (fun peer -> (Option.get peer).tsex)
            |> List.fold_left (fun x stsex -> 
                    let (pe, ps) = x in
                    let s = Option.get @@ SIDMap.find_opt (Session.txid stsex) pe.smap in
                    let (pe, p) = forward_sdecl_to_session pe res s in 
                    (pe, p :: ps)
                ) (pe, []) in 
            TreeSet.get_broken_links tree0 
            |> List.map (fun (node:Spn_tree.Node.t) -> 
                (List.find_opt (fun peer -> peer.pid = node.node_id) trees.peers))
            |> List.filter (fun opt -> opt != None)
            |> List.map (fun peer -> (Option.get peer).tsex)
            |> List.fold_left (fun x stsex -> 
                    let (pe, ps) = x in
                    let s = Option.get @@ SIDMap.find_opt (Session.txid stsex) pe.smap in
                    let (pe, p) = forget_sdecl_to_session pe res s in 
                    (pe, p :: ps)
                ) (pe, ps))
        in 
        Lwt.Infix.(
        Lwt.catch(fun () -> Lwt.join ps >>= fun () -> Lwt.return pe)
                (fun ex -> Logs_lwt.debug (fun m -> m "Ex %s" (Printexc.to_string ex)) >>= fun () -> Lwt.return pe))

    let forward_all_sdecl pe = 
        let _ = ResMap.for_all (fun _ res -> Lwt.ignore_result @@ forward_sdecl pe res pe.trees; true) pe.rmap in ()

    let notify_pub pe ?sub:(sub=None) res m = 
        let open Resource in
        let sub = match sub with 
        | None -> ResMap.exists (fun _ res -> 
            List.exists (fun map -> (map.sub != None || map.sto != None) && map.session != m.session) res.mappings) pe.rmap
        | Some sub -> sub in
        match (sub, m.matched_pub) with 
        | (true, true) -> (pe, [])
        | (true, false) -> 
            let m = {m with matched_pub = true} in
            let pres = Resource.with_mapping res m in
            let rmap = ResMap.add pres.name pres pe.rmap in
            let pe = {pe with rmap} in

            let session = SIDMap.find m.session pe.smap in
            let oc = Session.out_channel session in
            let ds = [Message.Declaration.SubscriberDecl (Message.SubscriberDecl.create m.id Message.SubscriptionMode.push_mode [])] in
            let decl = Message.Declare (Message.Declare.create (true, true) (OutChannel.next_rsn oc) ds) in
            Lwt.ignore_result @@ Logs_lwt.debug(fun m ->  m "Sending SubscriberDecl to session %s" (Id.to_string session.sid));
            let open Lwt.Infix in
            let p = Mcodec.ztcp_safe_write_frame_pooled (Session.tx_sex session) (Frame.Frame.create [decl]) pe.buffer_pool 
              >>= fun _ -> Lwt.return_unit in 
            (pe, [p])
        | (false, true) -> 
            let m = {m with matched_pub = false} in
            let pres = Resource.with_mapping res m in
            let rmap = ResMap.add pres.name pres pe.rmap in
            let pe = {pe with rmap} in

            let session = SIDMap.find m.session pe.smap in
            let oc = Session.out_channel session in
            let ds = [Message.Declaration.ForgetSubscriberDecl (Message.ForgetSubscriberDecl.create m.id)] in
            let decl = Message.Declare (Message.Declare.create (true, true) (OutChannel.next_rsn oc) ds) in
            Lwt.ignore_result @@ Logs_lwt.debug(fun m ->  m "Sending ForgetSubscriberDecl to session %s" (Id.to_string session.sid));
            let open Lwt.Infix in
            let p = Mcodec.ztcp_safe_write_frame_pooled (Session.tx_sex session) (Frame.Frame.create [decl]) pe.buffer_pool 
              >>= fun _ -> Lwt.return_unit in 
            (pe, [p])
        | (false, false) -> (pe, [])

    let notify_pub_matching_res pe ?sub:(sub=None) (s:Session.t) res = 
        let open Resource in 
        let%lwt _ = Logs_lwt.debug (fun m -> m "Notifing Publishers matching resource %s" (ResName.to_string res.name)) in 
        let (pe, ps) = List.fold_left (fun (pe, ps) name -> 
            match ResMap.find_opt name pe.rmap with 
            | None -> (pe, ps)
            | Some mres ->  List.fold_left (fun (pe, ps) m -> 
                    match (m.pub && m.session != s.sid) with 
                    | false -> (pe, ps)
                    | true -> let (pe, p) = notify_pub pe ~sub:sub mres m in (pe, List.append p ps)) 
                (pe, ps) mres.mappings
        ) (pe, []) Resource.(res.matches) in
        let%lwt _ = Lwt.join ps in
        Lwt.return pe

    let notify_all_pubs pe = 
        let open Resource in
        let (pe, ps) = ResMap.fold (fun _ res (pe, ps) -> 
            List.fold_left (fun (pe, ps) m -> 
                match m.pub with 
                | true -> let (pe, p) = notify_pub pe res m in (pe, List.append p ps)
                | false -> (pe, ps)) (pe, ps) res.mappings) pe.rmap (pe, []) in 
        let%lwt _ = Lwt.join ps in
        Lwt.return pe

    let register_subscription pe (s:Session.t) sd =
        let rid = Message.SubscriberDecl.rid sd in 
        let pull = match Message.SubscriberDecl.mode sd with 
            | Message.SubscriptionMode.PullMode -> true
            | Message.SubscriptionMode.PushMode -> false 
            | Message.SubscriptionMode.PeriodicPullMode _ -> true
            | Message.SubscriptionMode.PeriodicPushMode _ -> false in
        let resname = res_name s rid in
        let (pe, res) = update_resource_mapping pe resname s rid 
            (fun m -> 
                match m with 
                | Some m -> {m with sub=Some pull;} 
                | None -> {(Resource.create_mapping rid s.sid) with sub = Some pull;}) in
        Lwt.return (pe, Some res)

    let unregister_subscription pe (s:Session.t) fsd =
        let rid = Message.ForgetSubscriberDecl.id fsd in 
        let resname = res_name s rid in
        let (pe, res) = update_resource_mapping_opt pe resname s rid 
            (fun m -> 
                match m with 
                | Some m -> Some {m with sub=None;} 
                | None -> None) in
        Lwt.return (pe, res)

    let sub_state pe (s:Session.t) rid = 
        let resname = res_name s rid in
        let optres = ResMap.find_opt resname pe.rmap in 
        match optres with 
        | None -> None
        | Some res ->
            let open Resource in
            let mapping = List.find_opt (fun m -> m.session = s.sid) res.mappings in 
            match mapping with 
            | None -> None
            | Some mapping -> mapping.sub

    let process_sdecl pe s sd = 
        if sub_state pe s (Message.SubscriberDecl.rid sd) = None 
        then 
        begin
            let%lwt (pe, res) = register_subscription pe s sd in
            match res with 
            | None -> Lwt.return (pe, [])
            | Some res -> 
            let%lwt pe = forward_sdecl pe res pe.trees in
            let%lwt pe = notify_pub_matching_res pe s res ~sub:(Some true) in
            Lwt.return (pe, [])
        end
        else Lwt.return (pe, [])

    let process_fsdecl pe s fsd = 
        if sub_state pe s (Message.ForgetSubscriberDecl.id fsd) != None 
        then 
        begin
            let%lwt (pe, res) = unregister_subscription pe s fsd in
            match res with 
            | None -> Lwt.return (pe, [])
            | Some res -> 
            let%lwt pe = forward_sdecl pe res pe.trees in
            let%lwt pe = notify_pub_matching_res pe s res in
            Lwt.return (pe, [])
        end
        else Lwt.return (pe, [])

    (* ======================== STO DECL =========================== *)

    let sto_state pe (s:Session.t) rid = 
        let resname = res_name s rid in
        let optres = ResMap.find_opt resname pe.rmap in 
        match optres with 
        | None -> None
        | Some res ->
            let open Resource in
            let mapping = List.find_opt (fun m -> m.session = s.sid) res.mappings in 
            match mapping with 
            | None -> None
            | Some mapping -> mapping.sto 

    let forward_stodecl_to_session pe res zsex dist =       
        let module M = Message in
        let open Resource in 
        let oc = Session.out_channel zsex in
        let (pe, ds) = match res.name with 
        | ID id -> (
            let stodecl = M.Declaration.StorageDecl M.(StorageDecl.create id [ZProperty.StorageDist.make (Vle.of_int dist)]) in
            (pe, [stodecl]))
        | Path uri -> 
            let resdecl = M.Declaration.ResourceDecl (M.ResourceDecl.create res.local_id (PathExpr.to_string uri) []) in
            let stodecl = M.Declaration.StorageDecl (M.StorageDecl.create res.local_id [ZProperty.StorageDist.make (Vle.of_int dist)]) in
            let (pe, _) = update_resource_mapping pe res.name zsex res.local_id 
                (fun m -> match m with 
                    | Some mapping -> mapping
                    | None -> Resource.create_mapping res.local_id (Session.id zsex)) in 
            let rmap = VleMap.add res.local_id res.name zsex.rmap in
            let session = {zsex with rmap;} in
            let smap = SIDMap.add session.sid session pe.smap in
            ({pe with smap}, [resdecl; stodecl]) in
        let decl = M.Declare (M.Declare.create (true, true) (OutChannel.next_rsn oc) ds) in
        let open Lwt.Infix in 
        (pe, Mcodec.ztcp_safe_write_frame_pooled (Session.tx_sex zsex) (Frame.Frame.create [decl]) pe.buffer_pool>|= fun _ -> ())

    let forget_stodecl_to_session pe res zsex =
        let module M = Message in
        let open Resource in 
        let oc = Session.out_channel zsex in
        let (pe, ds) = match res.name with 
        | ID id -> (
            let fstodecl = M.Declaration.ForgetStorageDecl M.(ForgetStorageDecl.create id) in
            (pe, [fstodecl]))
        | Path uri -> 
            let resdecl = M.Declaration.ResourceDecl (M.ResourceDecl.create res.local_id (PathExpr.to_string uri) []) in
            let fstodecl = M.Declaration.ForgetStorageDecl M.(ForgetStorageDecl.create res.local_id) in
            let (pe, _) = update_resource_mapping pe res.name zsex res.local_id 
                (fun m -> match m with 
                    | Some mapping -> mapping
                    | None -> Resource.create_mapping res.local_id (Session.id zsex)) in 
            let rmap = VleMap.add res.local_id res.name zsex.rmap in
            let session = {zsex with rmap;} in
            let smap = SIDMap.add session.sid session pe.smap in
            ({pe with smap}, [resdecl; fstodecl]) in
        let decl = M.Declare (M.Declare.create (true, true) (OutChannel.next_rsn oc) ds) in
        let open Lwt.Infix in 
        (pe, Mcodec.ztcp_safe_write_frame_pooled (Session.tx_sex zsex) (Frame.Frame.create [decl]) pe.buffer_pool >|= fun _ -> ())


    let forward_stodecl pe res trees =
        let open Spn_trees_mgr in
        let open Resource in 
        let stos = List.filter (fun map -> map.sto != None) res.mappings in 
        let (pe, ps) = (match stos with 
        | [] -> 
            SIDMap.fold (fun _ session (pe, ps) -> 
                match Session.is_broker session with 
                | true -> let (pe, p) = forget_stodecl_to_session pe res session in (pe, p::ps)
                | false -> (pe, ps)) pe.smap (pe, [])
        | sto :: [] -> 
            (match SIDMap.find_opt sto.session pe.smap with 
            | None -> (pe, [])
            | Some stosex ->
            let tsex = Session.tx_sex stosex in
            let module TreeSet = (val trees.tree_mod : Spn_tree.Set.S) in
            let tree0 = Option.get (TreeSet.get_tree trees.tree_set 0) in
            let (pe, ps) = (match TreeSet.get_parent tree0 with 
            | None -> TreeSet.get_childs tree0 
            | Some parent -> parent :: TreeSet.get_childs tree0 )
            |> List.map (fun (node:Spn_tree.Node.t) -> 
                (List.find_opt (fun peer -> peer.pid = node.node_id) trees.peers))
            |> List.filter (fun opt -> opt != None)
            |> List.map (fun peer -> (Option.get peer).tsex)
            |> List.fold_left (fun x stsex -> 
                if(tsex != stsex)
                then 
                    begin
                    let (pe, ps) = x in
                    let s = Option.get @@ SIDMap.find_opt (Session.txid stsex) pe.smap in
                    let (pe, p) = forward_stodecl_to_session pe res s (Option.get sto.sto) in 
                    (pe, p :: ps)
                    end
                else x
                ) (pe, []) in 
            let (pe, ps) = match Session.is_broker stosex with
            | false -> (pe, ps)
            | true -> let (pe, p) = forget_stodecl_to_session pe res stosex in (pe, p::ps) in 
            TreeSet.get_broken_links tree0 
            |> List.map (fun (node:Spn_tree.Node.t) -> 
                (List.find_opt (fun peer -> peer.pid = node.node_id) trees.peers))
            |> List.filter (fun opt -> opt != None)
            |> List.map (fun peer -> (Option.get peer).tsex)
            |> List.fold_left (fun x stsex -> 
                if(tsex != stsex)
                then 
                    begin
                    let (pe, ps) = x in
                    let s = Option.get @@ SIDMap.find_opt (Session.txid stsex) pe.smap in
                    let (pe, p) = forget_stodecl_to_session pe res s in 
                    (pe, p :: ps)
                    end
                else x
                ) (pe, ps))
        | _ -> 
            let dist = List.fold_left (fun accu map -> min accu (Option.get map.sto)) (Option.get (List.hd stos).sto) stos in
            let module TreeSet = (val trees.tree_mod : Spn_tree.Set.S) in
            let tree0 = Option.get (TreeSet.get_tree trees.tree_set 0) in
            let (pe, ps) = (match TreeSet.get_parent tree0 with 
            | None -> TreeSet.get_childs tree0 
            | Some parent -> parent :: TreeSet.get_childs tree0 )
            |> List.map (fun (node:Spn_tree.Node.t) -> 
                (List.find_opt (fun peer -> peer.pid = node.node_id) trees.peers))
            |> List.filter (fun opt -> opt != None)
            |> List.map (fun peer -> (Option.get peer).tsex)
            |> List.fold_left (fun x stsex -> 
                    let (pe, ps) = x in
                    let s = Option.get @@ SIDMap.find_opt (Session.txid stsex) pe.smap in
                    let (pe, p) = forward_stodecl_to_session pe res s dist in 
                    (pe, p :: ps)
                ) (pe, []) in 
            TreeSet.get_broken_links tree0 
            |> List.map (fun (node:Spn_tree.Node.t) -> 
                (List.find_opt (fun peer -> peer.pid = node.node_id) trees.peers))
            |> List.filter (fun opt -> opt != None)
            |> List.map (fun peer -> (Option.get peer).tsex)
            |> List.fold_left (fun x stsex -> 
                    let (pe, ps) = x in
                    let s = Option.get @@ SIDMap.find_opt (Session.txid stsex) pe.smap in
                    let (pe, p) = forget_stodecl_to_session pe res s in 
                    (pe, p :: ps)
                ) (pe, ps))
        in 
        Lwt.Infix.(
        Lwt.catch(fun () -> Lwt.join ps >>= fun () -> Lwt.return pe)
                (fun ex -> Logs_lwt.debug (fun m -> m "Ex %s" (Printexc.to_string ex)) >>= fun () -> Lwt.return pe))

    let register_storage pe (s:Session.t) sd =
        let rid = Message.StorageDecl.rid sd in 
        let dist = match ZProperty.StorageDist.find_opt (Message.StorageDecl.properties sd) with 
            | Some prop -> (ZProperty.StorageDist.dist prop |> Vle.to_int) + 1
            | None -> 1 in
        let resname = res_name s rid in
        let (pe, res) = update_resource_mapping pe resname s rid 
            (fun m -> 
                match m with 
                | Some m -> {m with sto = Some dist} 
                | None -> {(Resource.create_mapping rid s.sid) with sto = Some dist;}) in
        Lwt.return (pe, Some res)

    let unregister_storage pe (s:Session.t) fsd =
        let rid = Message.ForgetStorageDecl.id fsd in 
        let resname = res_name s rid in
        let (pe, res) = update_resource_mapping_opt pe resname s rid 
            (fun m -> 
                match m with 
                | Some m -> Some {m with sto = None;} 
                | None -> None) in
        Lwt.return (pe, res)

    let forward_all_stodecl pe = 
        let _ = ResMap.for_all (fun _ res -> Lwt.ignore_result @@ forward_stodecl pe res pe.trees; true) pe.rmap in ()

    let process_stodecl pe s sd = 
        let oldstate = sto_state pe s (Message.StorageDecl.rid sd) in
        let%lwt (pe, res) = register_storage pe s sd in
        let newstate = sto_state pe s (Message.StorageDecl.rid sd) in
        if oldstate <> newstate then 
        begin
            match res with 
            | None -> Lwt.return (pe, [])
            | Some res -> 
            let%lwt pe = forward_stodecl pe res pe.trees in
            let%lwt pe = notify_pub_matching_res pe s res ~sub:(Some true) in
            Lwt.return (pe, [])
        end
        else Lwt.return (pe, [])

    let process_fstodecl pe s fsd = 
        let oldstate = sto_state pe s (Message.ForgetStorageDecl.id fsd) in
        let%lwt (pe, res) = unregister_storage pe s fsd in
        let newstate = sto_state pe s (Message.ForgetStorageDecl.id fsd) in
        if oldstate <> newstate then 
        begin
            match res with 
            | None -> Lwt.return (pe, [])
            | Some res -> 
            let%lwt pe = forward_stodecl pe res pe.trees in
            let%lwt pe = notify_pub_matching_res pe s res in
            Lwt.return (pe, [])
        end
        else Lwt.return (pe, [])


    (* ======================== EVAL DECL =========================== *)

    let eval_state pe (s:Session.t) rid = 
        let resname = res_name s rid in
        let optres = ResMap.find_opt resname pe.rmap in 
        match optres with 
        | None -> None
        | Some res ->
            let open Resource in
            let mapping = List.find_opt (fun m -> m.session = s.sid) res.mappings in 
            match mapping with 
            | None -> None
            | Some mapping -> mapping.eval 

    let forward_evaldecl_to_session pe res zsex dist =       
        let module M = Message in
        let open Resource in 
        let oc = Session.out_channel zsex in
        let (pe, ds) = match res.name with 
        | ID id -> (
            let evaldecl = M.Declaration.EvalDecl M.(EvalDecl.create id [ZProperty.StorageDist.make (Vle.of_int dist)]) in
            (pe, [evaldecl]))
        | Path uri -> 
            let resdecl = M.Declaration.ResourceDecl (M.ResourceDecl.create res.local_id (PathExpr.to_string uri) []) in
            let evaldecl = M.Declaration.EvalDecl (M.EvalDecl.create res.local_id [ZProperty.StorageDist.make (Vle.of_int dist)]) in
            let (pe, _) = update_resource_mapping pe res.name zsex res.local_id 
                (fun m -> match m with 
                    | Some mapping -> mapping
                    | None -> Resource.create_mapping res.local_id (Session.id zsex)) in 
            let rmap = VleMap.add res.local_id res.name zsex.rmap in
            let session = {zsex with rmap;} in
            let smap = SIDMap.add session.sid session pe.smap in
            ({pe with smap}, [resdecl; evaldecl]) in
        let decl = M.Declare (M.Declare.create (true, true) (OutChannel.next_rsn oc) ds) in
        let open Lwt.Infix in 
        (pe, Mcodec.ztcp_safe_write_frame_pooled (Session.tx_sex zsex) (Frame.Frame.create [decl]) pe.buffer_pool>|= fun _ -> ())

    let forget_evaldecl_to_session pe res zsex =
        let module M = Message in
        let open Resource in 
        let oc = Session.out_channel zsex in
        let (pe, ds) = match res.name with 
        | ID id -> (
            let fevaldecl = M.Declaration.ForgetEvalDecl M.(ForgetEvalDecl.create id) in
            (pe, [fevaldecl]))
        | Path uri -> 
            let resdecl = M.Declaration.ResourceDecl (M.ResourceDecl.create res.local_id (PathExpr.to_string uri) []) in
            let fevaldecl = M.Declaration.ForgetEvalDecl M.(ForgetEvalDecl.create res.local_id) in
            let (pe, _) = update_resource_mapping pe res.name zsex res.local_id 
                (fun m -> match m with 
                    | Some mapping -> mapping
                    | None -> Resource.create_mapping res.local_id (Session.id zsex)) in 
            let rmap = VleMap.add res.local_id res.name zsex.rmap in
            let session = {zsex with rmap;} in
            let smap = SIDMap.add session.sid session pe.smap in
            ({pe with smap}, [resdecl; fevaldecl]) in
        let decl = M.Declare (M.Declare.create (true, true) (OutChannel.next_rsn oc) ds) in
        let open Lwt.Infix in 
        (pe, Mcodec.ztcp_safe_write_frame_pooled (Session.tx_sex zsex) (Frame.Frame.create [decl]) pe.buffer_pool >|= fun _ -> ())

    let forward_evaldecl pe res trees =
        let open Spn_trees_mgr in
        let open Resource in 
        let evals = List.filter (fun map -> map.eval != None) res.mappings in 
        let (pe, ps) = (match evals with 
        | [] -> 
            SIDMap.fold (fun _ session (pe, ps) -> 
                match Session.is_broker session with 
                | true -> let (pe, p) = forget_evaldecl_to_session pe res session in (pe, p::ps)
                | false -> (pe, ps)) pe.smap (pe, [])
        | eval :: [] -> 
            (match SIDMap.find_opt eval.session pe.smap with 
            | None -> (pe, [])
            | Some evalsex ->
            let tsex = Session.tx_sex evalsex in
            let module TreeSet = (val trees.tree_mod : Spn_tree.Set.S) in
            let tree0 = Option.get (TreeSet.get_tree trees.tree_set 0) in
            let (pe, ps) = (match TreeSet.get_parent tree0 with 
            | None -> TreeSet.get_childs tree0 
            | Some parent -> parent :: TreeSet.get_childs tree0 )
            |> List.map (fun (node:Spn_tree.Node.t) -> 
                (List.find_opt (fun peer -> peer.pid = node.node_id) trees.peers))
            |> List.filter (fun opt -> opt != None)
            |> List.map (fun peer -> (Option.get peer).tsex)
            |> List.fold_left (fun x stsex -> 
                if(tsex != stsex)
                then 
                    begin
                    let (pe, ps) = x in
                    let s = Option.get @@ SIDMap.find_opt (Session.txid stsex) pe.smap in
                    let (pe, p) = forward_evaldecl_to_session pe res s (Option.get eval.eval) in 
                    (pe, p :: ps)
                    end
                else x
                ) (pe, []) in 
            let (pe, ps) = match Session.is_broker evalsex with
            | false -> (pe, ps)
            | true -> let (pe, p) = forget_evaldecl_to_session pe res evalsex in (pe, p::ps) in 
            TreeSet.get_broken_links tree0 
            |> List.map (fun (node:Spn_tree.Node.t) -> 
                (List.find_opt (fun peer -> peer.pid = node.node_id) trees.peers))
            |> List.filter (fun opt -> opt != None)
            |> List.map (fun peer -> (Option.get peer).tsex)
            |> List.fold_left (fun x stsex -> 
                if(tsex != stsex)
                then 
                    begin
                    let (pe, ps) = x in
                    let s = Option.get @@ SIDMap.find_opt (Session.txid stsex) pe.smap in
                    let (pe, p) = forget_evaldecl_to_session pe res s in 
                    (pe, p :: ps)
                    end
                else x
                ) (pe, ps))
        | _ -> 
            let dist = List.fold_left (fun accu map -> min accu (Option.get map.eval)) (Option.get (List.hd evals).eval) evals in
            let module TreeSet = (val trees.tree_mod : Spn_tree.Set.S) in
            let tree0 = Option.get (TreeSet.get_tree trees.tree_set 0) in
            let (pe, ps) = (match TreeSet.get_parent tree0 with 
            | None -> TreeSet.get_childs tree0 
            | Some parent -> parent :: TreeSet.get_childs tree0 )
            |> List.map (fun (node:Spn_tree.Node.t) -> 
                (List.find_opt (fun peer -> peer.pid = node.node_id) trees.peers))
            |> List.filter (fun opt -> opt != None)
            |> List.map (fun peer -> (Option.get peer).tsex)
            |> List.fold_left (fun x stsex -> 
                    let (pe, ps) = x in
                    let s = Option.get @@ SIDMap.find_opt (Session.txid stsex) pe.smap in
                    let (pe, p) = forward_evaldecl_to_session pe res s dist in 
                    (pe, p :: ps)
                ) (pe, []) in 
            TreeSet.get_broken_links tree0 
            |> List.map (fun (node:Spn_tree.Node.t) -> 
                (List.find_opt (fun peer -> peer.pid = node.node_id) trees.peers))
            |> List.filter (fun opt -> opt != None)
            |> List.map (fun peer -> (Option.get peer).tsex)
            |> List.fold_left (fun x stsex -> 
                    let (pe, ps) = x in
                    let s = Option.get @@ SIDMap.find_opt (Session.txid stsex) pe.smap in
                    let (pe, p) = forget_evaldecl_to_session pe res s in 
                    (pe, p :: ps)
                ) (pe, ps))
        in 
        Lwt.Infix.(
        Lwt.catch(fun () -> Lwt.join ps >>= fun () -> Lwt.return pe)
                (fun ex -> Logs_lwt.debug (fun m -> m "Ex %s" (Printexc.to_string ex)) >>= fun () -> Lwt.return pe))

    let register_eval pe (s:Session.t) ed =
        let rid = Message.EvalDecl.rid ed in 
        let dist = match ZProperty.StorageDist.find_opt (Message.EvalDecl.properties ed) with 
            | Some prop -> (ZProperty.StorageDist.dist prop |> Vle.to_int) + 1
            | None -> 1 in
        let resname = res_name s rid in
        let (pe, res) = update_resource_mapping pe resname s rid 
            (fun m -> 
                match m with 
                | Some m -> {m with eval = Some dist} 
                | None -> {(Resource.create_mapping rid s.sid) with eval = Some dist;}) in
        Lwt.return (pe, Some res)

    let unregister_eval pe (s:Session.t) fed =
        let rid = Message.ForgetEvalDecl.id fed in 
        let resname = res_name s rid in
        let (pe, res) = update_resource_mapping_opt pe resname s rid 
            (fun m -> 
                match m with 
                | Some m -> Some {m with eval = None;} 
                | None -> None) in
        Lwt.return (pe, res)

    let forward_all_evaldecl pe = 
        let _ = ResMap.for_all (fun _ res -> Lwt.ignore_result @@ forward_evaldecl pe res pe.trees; true) pe.rmap in ()

    let process_evaldecl pe s ed = 
        let oldstate = eval_state pe s (Message.EvalDecl.rid ed) in
        let%lwt (pe, res) = register_eval pe s ed in
        let newstate = eval_state pe s (Message.EvalDecl.rid ed) in
        if oldstate <> newstate then 
        begin
            match res with 
            | None -> Lwt.return (pe, [])
            | Some res -> 
            let%lwt pe = forward_evaldecl pe res pe.trees in
            Lwt.return (pe, [])
        end
        else Lwt.return (pe, [])

    let process_fevaldecl pe s fed = 
        let oldstate = eval_state pe s (Message.ForgetEvalDecl.id fed) in
        let%lwt (pe, res) = unregister_eval pe s fed in
        let newstate = eval_state pe s (Message.ForgetEvalDecl.id fed) in
        if oldstate <> newstate then 
        begin
            match res with 
            | None -> Lwt.return (pe, [])
            | Some res -> 
            let%lwt pe = forward_evaldecl pe res pe.trees in
            Lwt.return (pe, [])
        end
        else Lwt.return (pe, [])

    (* ======================== ======== =========================== *)

    let forward_all_decls pe = 
        forward_all_sdecl pe;
        forward_all_stodecl pe;
        forward_all_evaldecl pe

    let make_result pe _ cd =
        Lwt.return (pe, [Message.Declaration.ResultDecl (Message.ResultDecl.create (Message.CommitDecl.commit_id cd) (char_of_int 0) None)])

    let process_declaration pe sid d =
        let open Message.Declaration in
        match SIDMap.find_opt sid pe.smap with 
        | Some s ->
            (match d with
            | ResourceDecl rd ->
                let%lwt _ = Logs_lwt.debug (fun m -> m "RDecl for resource: %Ld %s"  (Message.ResourceDecl.rid rd) (Message.ResourceDecl.resource rd) ) in
                let%lwt pe = declare_resource pe s rd in
                Lwt.return (pe, [])
            | PublisherDecl pd ->
                let%lwt _ = Logs_lwt.debug (fun m -> m "PDecl for resource: %Ld" (Message.PublisherDecl.rid pd)) in
                process_pdecl pe s pd
            | SubscriberDecl sd ->
                let%lwt _ = Logs_lwt.debug (fun m -> m "SDecl for resource: %Ld"  (Message.SubscriberDecl.rid sd)) in
                process_sdecl pe s sd
            | StorageDecl sd ->
                let%lwt _ = Logs_lwt.debug (fun m -> m "StoDecl for resource: %Ld"  (Message.StorageDecl.rid sd)) in
                process_stodecl pe s sd
            | EvalDecl ed ->
                let%lwt _ = Logs_lwt.debug (fun m -> m "EvalDecl for resource: %Ld"  (Message.EvalDecl.rid ed)) in
                process_evaldecl pe s ed
            | ForgetSubscriberDecl fsd ->
                let%lwt _ = Logs_lwt.debug (fun m -> m "FSDecl for resource: %Ld"  (Message.ForgetSubscriberDecl.id fsd)) in
                process_fsdecl pe s fsd
            | ForgetStorageDecl fsd ->
                let%lwt _ = Logs_lwt.debug (fun m -> m "FStoDecl for resource: %Ld"  (Message.ForgetStorageDecl.id fsd)) in
                process_fstodecl pe s fsd
            | ForgetEvalDecl fed ->
                let%lwt _ = Logs_lwt.debug (fun m -> m "FEvalDecl for resource: %Ld"  (Message.ForgetEvalDecl.id fed)) in
                process_fevaldecl pe s fed
            | CommitDecl cd -> 
                let%lwt _ = Logs_lwt.debug (fun m -> m "Commit SDecl ") in
                make_result pe s cd
            | _ ->
                let%lwt _ = Logs_lwt.debug (fun m -> m "Unknown / Unhandled Declaration...."  ) in       
                Lwt.return (pe, []))
        | None -> let%lwt _ = Logs_lwt.debug (fun m -> m "Received declaration on unknown session %s. Ignore it! \n" (Id.to_string sid)) in Lwt.return (pe, [])


    let process_declarations engine sid ds =  
        let open Message.Declaration in
        (* Must process ResourceDecls first *)
        Guard.guarded engine 
        @@ fun pe -> 
            let%lwt (pe, ms) = List.sort (fun x y -> match (x, y) with 
            | (ResourceDecl _, ResourceDecl _) -> 0
            | (ResourceDecl _, _) -> -1
            | (_, ResourceDecl _) -> 1
            | (_, _) -> 0) ds
                            |> List.fold_left (fun x d -> 
                                let%lwt (pe, ds) = x in
                                let%lwt (pe, decl) = process_declaration pe sid d in 
                                Lwt.return (pe, decl @ ds)) (Lwt.return (pe, [])) 
        in Guard.return ms pe

    let process_declare engine tsex msg =         
        let pe = Guard.get engine in
        let%lwt _ = Logs_lwt.debug (fun m -> m "Handling Declare Message") in    
        let sid = Session.txid tsex in 
        match SIDMap.find_opt sid pe.smap with 
        | Some s ->
            let ic = Session.in_channel s in
            let oc = Session.out_channel s in
            let sn = (Message.Declare.sn msg) in
            let csn = InChannel.rsn ic in
            if sn >= csn then
            begin
                InChannel.update_rsn ic sn  ;
                let%lwt ds = process_declarations engine sid (Message.Declare.declarations msg) in
                match ds with 
                | [] ->
                Lwt.return [Message.AckNack (Message.AckNack.create (Vle.add sn 1L) None)]
                | _ as ds ->
                Lwt.return [Message.Declare (Message.Declare.create (true, true) (OutChannel.next_rsn oc) ds);
                            Message.AckNack (Message.AckNack.create (Vle.add sn 1L) None)]
            end
            else
            begin
                let%lwt _ = Logs_lwt.debug (fun m -> m "Received out of oder message") in
                Lwt.return  []
            end

        | None -> let%lwt _ = Logs_lwt.debug (fun m -> m "Received message on unknown session %s. Ignore it! \n" (Id.to_string sid)) in Lwt.return [] 
