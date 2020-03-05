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
open Caqti_driver
open Be_sql_types

module SQLBE = struct 

  module type Config = sig    
    val id : BeId.t 
    val properties : properties
    val conx : connection
  end

  module Make (C : Config) = struct 

    let id = C.id
    let properties = C.properties

    let to_string = "SQLBE#"^(BeId.to_string C.id)^"{"^(Properties.to_string properties)^"}"


    (*************************)
    (*   Common operations   *)
    (*************************)
    let dispose storage_info =
      match storage_info.on_dispose with
      | Drop -> 
        Logs.debug (fun m -> m "[SQL]: dispose storage of table %s dropping it" (storage_info.table_name));
        drop_table C.conx storage_info.table_name
      | Truncate ->
        Logs.debug (fun m -> m "[SQL]: dispose storage of table %s truncating it" (storage_info.table_name));
        trunc_table C.conx storage_info.table_name
      | DoNothing ->
        Logs.debug (fun m -> m "[SQL]: dispose storage of table %s keeping it" (storage_info.table_name));
        fun () -> Lwt.return_unit 

    let make_kv_table_name () =
      "Zenoh_kv_table_"^(Uuid.make () |> Uuid.to_string |> String.map (function | '-' -> '_' | c -> c))

    let compare_schema (cols, types) (cols', types') =
      (List.for_all2 (=) cols cols') && (Dyntype.equal types types')

    let create_storage selector props =
      let open LwtM.InfixM in
      let connection = C.conx in
      let props = Properties.union (fun _ _ v2 -> Some v2) properties props in
      let table_name = match Properties.get Be_sql_properties.Key.table props with
        | Some name -> name
        | None -> make_kv_table_name ()
      in
      let on_dispose = on_dispose_from_properties props in
      Logs.debug (fun m -> m "[SQL]: create storage for table %s" table_name);
      let%lwt (schema, is_kv_table) = match%lwt Caqti_driver.get_schema connection table_name with
        | Some s ->
          let is_kv_table = compare_schema s Caqti_driver.kv_table_schema in
          Logs.debug (fun m -> m "[SQL]: table %s found (is_kv_table = %b)" table_name is_kv_table);
          if not is_kv_table && not @@ Selector.is_path_unique selector then
            Lwt.fail @@ YException (`StoreError (`Msg (Printf.sprintf
              "Invalid selector '%s' on legacy SQL table '%s': a storage on a legacy SQL table must not have wildcards in its selector" (Selector.to_string selector) table_name))) 
          else
            Lwt.return (s, is_kv_table)
        | None -> 
          Logs.debug (fun m -> m "[SQL]: table %s not found - create it as key/value table" table_name);
          Caqti_driver.create_kv_table connection table_name props >>= fun r -> Lwt.return (r, true)
      in
      let keys_prefix = Selector.get_prefix selector in
      let keys_prefix_length = String.length (Path.to_string keys_prefix) in
      let removals = Guard.create @@ RemovalMap.empty in
      let storage_info = { selector; keys_prefix; keys_prefix_length; props; connection; table_name; schema; on_dispose; removals } in
      if is_kv_table then
        Lwt.return @@
        Storage.make selector props
          (dispose storage_info)
          (Kv_table.get storage_info)
          (Kv_table.put storage_info)
          (Kv_table.update storage_info)
          (Kv_table.remove storage_info)
      else
        Lwt.fail @@ YException (`StoreError (`Msg "Creation of Storage on a legacy SQL table is temporarly not supported."))
        (* Lwt.return @@
        Storage.make selector props
          (dispose storage_info)
          (Legacy_table.get storage_info)
          (Legacy_table.put storage_info)
          (Legacy_table.update storage_info)
          (Legacy_table.remove storage_info) *)
  end
end 


module SQLBEF = struct 

  let make id properties = 
    let url = match Properties.get Be_sql_properties.Key.url properties with
      | Some url -> url
      | None -> raise @@ YException (`InvalidBackendProperty (`Msg ("Property "^Be_sql_properties.Key.url^" is not specified")))
    in
    let connection = Caqti_driver.connect url in
    let module M = SQLBE.Make (
      struct 
        let id = id
        let properties = Properties.add "kind" "sql" properties
        let conx = connection
      end) 
    in Lwt.return (module M : Backend)

end

let () =
  Zenoh_storages_be.register_backend_factory (module SQLBEF:BackendFactory);
