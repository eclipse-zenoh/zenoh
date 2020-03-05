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
open Zenoh_types

module Storage = struct

  module Id = Apero.Uuid
  module NetUtils = Zenoh_storages_netutils

  type t = 
    { id : Id.t
    ; selector : Selector.t
    ; props : properties
    ; dispose : unit -> unit Lwt.t
    ; get : Selector.t -> (Path.t * TimedValue.t) list Lwt.t
    ; put : Path.t -> TimedValue.t -> unit Lwt.t 
    ; put_delta : Path.t -> TimedValue.t -> unit Lwt.t 
    ; remove : Path.t -> Timestamp.t -> unit Lwt.t
    ; as_string : string
    }

  let make selector props dispose get put put_delta remove =
    let alias = Properties.get "alias" props in
    let uuid = match alias with | Some(a) -> Id.make_from_alias a | None -> Id.make () in
    { id = uuid; selector; props; dispose; get; put; put_delta; remove;
      as_string = "Sto#"^(Id.to_string uuid)^"("^(Selector.to_string selector)^")"
    }

  let dispose s = s.dispose ()
  let id s = s.id
  let alias s = Apero.Uuid.alias s.id
  let selector s = s.selector
  let properties s = s.props

  let to_string s = s.as_string

  let covers_fully s sel = Selector.includes ~subsel:sel s.selector
  let covers_partially s sel = Selector.intersects sel s.selector

  let get s = s.get
  let put s = s.put
  let update s = s.put_delta
  let remove s = s.remove


  let on_zenoh_write s path changes =
    Lwt_list.iter_s (function
      | Put(tv)      -> put s path tv
      | Update(tv)   -> update s path tv
      | Remove(time) -> remove s path time
    ) changes

  let align s zenoh selector =
    Logs.debug (fun m -> m "[Sto] %s: align with remote storages..." (Id.to_string s.id));
    (* create a temporary Zenoh listener (to not miss ongoing updates) *)
    let listener path (changes:change list) =
      Lwt_list.iter_s (fun change -> 
        if Astring.is_prefix ~affix:"/@" (Path.to_string path) then Lwt.return_unit
        else match change with
        | Put tv -> put s path tv
        | Update tv -> update s path tv
        | Remove time -> remove s path time) changes
    in
    let%lwt tmp_sub = NetUtils.subscribe zenoh ~listener selector in

    (* query the remote storages (to get historical data) *)
    let%lwt kvs = NetUtils.query_timedvalues zenoh ~dest_evals:No selector in
    let%lwt () = List.map (fun (path, (tv:TimedValue.t)) ->
      if Astring.is_prefix ~affix:"/@" (Path.to_string path) then Lwt.return_unit
      else put s path tv)
      kvs
      |> Lwt.join
    in
    Logs.debug (fun m -> m "[Sto] %s: alignment done" (Id.to_string s.id));
    let open Apero.LwtM.InfixM in
    (* program the removal of the temporary Zenoh listener after a while *)
    Lwt.async (fun() -> Lwt_unix.sleep 10.0 >> lazy (NetUtils.unsubscribe zenoh tmp_sub));
    Lwt.return_unit

end  [@@deriving show]

