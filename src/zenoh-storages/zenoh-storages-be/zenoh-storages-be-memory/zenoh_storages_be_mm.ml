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
open Zenoh_common_errors
open Zenoh_storages_core_types
open Zenoh_storages_be
open Zenoh_storages_storage

module MainMemoryBE = struct 

  module type Config = sig    
    val id: BeId.t 
    val properties : properties
  end

  module Make (C : Config) = struct     
    module SMap = Map.Make(Path)

    type stored_value = | Present of TimedValue.t | Removed of Timestamp.t * unit Lwt.t

    let state = Guard.create SMap.empty

    let id = C.id
    let properties = C.properties

    let to_string = "MainMemoryBE#"^(BeId.to_string C.id)^"{"^(Properties.to_string properties)^"}"

    let cleanup_timeout = 5.

    let cleanup_entry path =
      let open Lwt.Infix in
      Lwt_unix.sleep cleanup_timeout >>= fun () ->
      Guard.guarded state
      @@ fun self ->
        Guard.return () (SMap.remove path self)

    let get selector =       
      let self = Guard.get state in        
      Lwt.return @@ match Selector.as_unique_path selector with 
      | Some path ->
        (match SMap.find_opt path self with 
        | Some Present tv -> [(path, tv)]
        | _ -> [])
      | None -> 
        SMap.fold (fun path sv l -> 
          match sv with
          | Present tv -> if Selector.is_matching_path path selector then (path, tv)::l else l
          | Removed _ -> l
        ) self []

    let put path (value:TimedValue.t) =
      Guard.guarded state
      @@ fun self ->
        match SMap.find_opt path self with
        | Some Present {time=t; value=_} ->
          let open Timestamp.Infix in
          if (t < value.time)
          then Guard.return () (SMap.add path (Present value) self)
          else (
            Logs.debug (fun m -> m "[MEM] put on %s dropped: out-of-date" (Path.to_string path));
            Guard.return () self)
        | Some Removed (t, cleanup) ->
          let open Timestamp.Infix in
          if (t < value.time)
          then (
            Lwt.cancel cleanup; 
            Guard.return () (SMap.add path (Present value) self))
          else (
            Logs.debug (fun m -> m "[MEM] put on %s dropped: out-of-date" (Path.to_string path));
            Guard.return () self)
        | None ->
          Guard.return  () (SMap.add path (Present value) self)

    let update path (delta:TimedValue.t) =       
      Guard.guarded state
      @@ fun self -> 
        match SMap.find_opt path self with 
        | Some Present tv ->
          if TimedValue.preceeds ~first:tv ~second:delta
          then 
            (match TimedValue.update tv ~delta with
            | Ok tv' -> Guard.return () (SMap.add path (Present tv') self)
            | Error e -> Logs.warn (fun m -> m "[MEM] update on %s failed: %s" (Path.to_string path) (show_yerror e)); Guard.return () self
            )
          else (
            Logs.debug (fun m -> m "[MEM] update on %s dropped: out-of-date" (Path.to_string path));
            Guard.return () self)
        | Some Removed (t, cleanup) ->
          let open Timestamp.Infix in
          if (t < delta.time)
          then (
            Lwt.cancel cleanup; 
            Guard.return () (SMap.add path (Present delta) self))
          else Guard.return () self
        | None -> Guard.return () (SMap.add path (Present delta) self)


    let remove path time = 
      Guard.guarded state 
      @@ fun self ->
        match SMap.find_opt path self with
        | Some Removed _ ->
          (* do nothing *)
          Guard.return () self
        | Some Present {time=t; value=_} ->
          let open Timestamp.Infix in
          if (t < time)
          then Guard.return () (SMap.add path (Removed (time, cleanup_entry path)) self)
          else (
            Logs.debug (fun m -> m "[MEM] remove on %s dropped: out-of-date" (Path.to_string path));
            Guard.return () self)
        | None ->
          (* NOTE: even if path is not known yet, we need to store the removal time:
            if ever a put with a lower timestamp arrive (e.g. msg inversion between put and remove) 
            we must drop the put.
          *)
          Guard.return  () (SMap.add path (Removed (time, cleanup_entry path)) self)


    let dispose () = 
      Guard.guarded state
        (fun _ -> Guard.return () (SMap.empty))


    let create_storage selector props =
      Lwt.return @@ Storage.make selector props dispose get put update remove
  end
end 


module MainMemoryBEF = struct 
  let make id properties =
    let module M = MainMemoryBE.Make (
    struct 
      let id = id
      let properties = Properties.add "kind" "memory" properties
    end) in Lwt.return (module M : Backend)

end

let () =
  Zenoh_storages_be.register_backend_factory (module MainMemoryBEF:BackendFactory);
