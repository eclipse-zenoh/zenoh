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
open Zenoh_common_errors

type database_type = | POSTGRESQL | MARIADB | SQLITE3

type connection = {
  pool: (Caqti_lwt.connection, Caqti_error.t) Caqti_lwt.Pool.t;
  db_type: database_type
}


let fail_on_error res = match%lwt res with
  | Ok r -> Lwt.return r
  | Error e -> 
    Logs.err (fun m -> m "[SQL]: %s" (Caqti_error.show e));
    Lwt.fail @@ YException (`StoreError (`Msg (Caqti_error.show e)))

let exec_query conx query =
  let exec_query' (module C : Caqti_lwt.CONNECTION) =
    Logs.debug (fun m -> m "[SQL]: %s" query);
    C.exec (Caqti_request.exec Caqti_type.unit query) ()
  in
  Caqti_lwt.Pool.use exec_query' conx.pool |> fail_on_error

let collect_query conx query row_type=
  let collect_query' (module C : Caqti_lwt.CONNECTION) =
    Logs.debug (fun m -> m "[SQL]: %s" query);
    C.collect_list (Caqti_request.collect Caqti_type.unit row_type query) ()
  in
  Caqti_lwt.Pool.use collect_query' conx.pool |> fail_on_error




let database_type_of_url uri =
  match Uri.scheme uri with
  | None -> let err_msg = "no schema in URL "^(Uri.to_string uri) in print_endline ("ERROR: "^err_msg); failwith err_msg
  | Some scheme -> 
    if scheme = "postgresql" then POSTGRESQL
    else if scheme = "mariadb" then MARIADB
    else if scheme = "sqlite3" then SQLITE3
    else let err_msg = "Unkown schema in URL "^(Uri.to_string uri) in print_endline ("ERROR: "^err_msg); failwith err_msg


let connect url =
  let uri = Uri.of_string url in
  let db_type = database_type_of_url uri in
  match Caqti_lwt.connect_pool ~max_size:10 uri with
  | Ok pool -> { pool; db_type }
  | Error err -> let err_msg = Caqti_error.show err in print_endline ("ERROR: "^err_msg); failwith err_msg


module [@warning "-32"] Dyntype = struct
  type t = Pack : 'a Caqti_type.t * ('a -> string list) -> t

  let as_list x = x::[]
  let (<.>) f g = fun x -> f @@ g @@ x
  let empty = Pack (Caqti_type.unit, fun () -> [])
  let bool = Pack (Caqti_type.bool, as_list <.> string_of_bool)
  let int = Pack (Caqti_type.int, as_list <.> string_of_int)
  let int32 = Pack (Caqti_type.int32, as_list <.> Int32.to_string)
  let int64 = Pack (Caqti_type.int64, as_list <.> Int64.to_string)
  let float = Pack (Caqti_type.float, as_list <.> string_of_float)
  let string = Pack (Caqti_type.string, fun s -> s::[])
  let octets = Pack (Caqti_type.octets, fun s -> s::[])
  let pdate = Pack (Caqti_type.pdate, fun t -> (Ptime.to_date t |> function (y,m,d) -> Printf.sprintf "%d-%02d-%02d" y m d)::[])
  let ptime = Pack (Caqti_type.ptime, as_list <.> Ptime.to_rfc3339)
  let ptime_span = Pack (Caqti_type.ptime_span, as_list <.> string_of_float <.> Ptime.Span.to_float_s )

  let add (Pack (t,f)) (Pack (t',f')) = Pack ((Caqti_type.tup2 t t'), fun (v, v') ->  (f v)@(f' v'))

  let to_string (Pack (t,_)) = Caqti_type.show t

  let equal t t' = (to_string t) = (to_string t')
end

module TypeMap = Map.Make(String)

let types_map_postgresql =
  let open TypeMap in
  TypeMap.empty |>
  add "bigint" Dyntype.int64 |>
  add "bigserial" Dyntype.int64 |>
  add "bit" Dyntype.octets |>
  add "bit varying" Dyntype.octets |>
  add "boolean" Dyntype.bool |>
  add "bytea" Dyntype.octets |>
  add "character" Dyntype.string |>
  add "character varying" Dyntype.string |>
  add "date" Dyntype.pdate |>
  add "double precision" Dyntype.float |>
  add "integer" Dyntype.int32 |>
  add "json" Dyntype.string |>
  add "real" Dyntype.float |>
  add "smallint" Dyntype.int32 |>
  add "smallserial" Dyntype.int32 |>
  add "serial" Dyntype.int32 |>
  add "text" Dyntype.string |>
  add "xml" Dyntype.string


let types_map_mariadb =
  let open TypeMap in
  TypeMap.empty |>
  add "tinyint"  Dyntype.int32 |>
  add "boolean"  Dyntype.bool |>
  add "smallint"  Dyntype.int32 |>
  add "mediumint"  Dyntype.int32 |>
  add "int"  Dyntype.int32 |>
  add "integer"  Dyntype.int32 |>
  add "bigint"  Dyntype.int64 |>
  add "decimal"  Dyntype.float |>
  add "dec"  Dyntype.float |>
  add "numeric"  Dyntype.float |>
  add "fixed"  Dyntype.float |>
  add "float"  Dyntype.float |>
  add "double"  Dyntype.float |>
  add "char"  Dyntype.string |>
  add "varchar"  Dyntype.string |>
  add "binary"  Dyntype.octets |>
  add "char byte"  Dyntype.octets |>
  add "varbinary"  Dyntype.octets |>
  add "tinyblob"  Dyntype.octets |>
  add "blob"  Dyntype.octets |>
  add "mediumblob"  Dyntype.octets |>
  add "longblob"  Dyntype.octets |>
  add "tinytext"  Dyntype.string |>
  add "mediumtext"  Dyntype.string |>
  add "longtext"  Dyntype.string |>
  add "json"  Dyntype.string |>
  add "date"  Dyntype.pdate


let types_map_sqlite3 =
  let open TypeMap in
  TypeMap.empty |>
  (*  SQLite3 *)
  add "int"  Dyntype.int64 |>
  add "integer"  Dyntype.int64 |>
  add "tinyint"  Dyntype.int64 |>
  add "smallint"  Dyntype.int64 |>
  add "mediumint"  Dyntype.int64 |>
  add "bigint"  Dyntype.int64 |>
  add "unsigned big int"  Dyntype.int64 |>
  add "int2"  Dyntype.int64 |>
  add "int8"  Dyntype.int64 |>
  add "character"  Dyntype.string |>
  add "varchar"  Dyntype.string |>
  add "varying character"  Dyntype.string |>
  add "nchar"  Dyntype.string |>
  add "native character"  Dyntype.string |>
  add "nvarchar"  Dyntype.string |>
  add "text"  Dyntype.string |>
  add "clob"  Dyntype.string |>
  add "blob"  Dyntype.octets |>
  add "real"  Dyntype.float |>
  add "double"  Dyntype.float |>
  add "double precision"  Dyntype.float |>
  add "float"  Dyntype.float |>
  add "numeric"  Dyntype.float |>
  add "decimal"  Dyntype.float |>
  add "boolean"  Dyntype.bool |>
  add "date"  Dyntype.pdate


let normalized_type_name type_name =
  (match String.index_opt type_name '(' with
   | Some i -> String.sub type_name 0 i
   | None -> type_name)
  |> String.lowercase_ascii

let type_from_name db_type type_name =
  let types_map = match db_type with
    | POSTGRESQL -> types_map_postgresql
    | MARIADB -> types_map_mariadb
    | SQLITE3 -> types_map_sqlite3
  in
  match TypeMap.find_opt (normalized_type_name type_name) types_map with
  | Some t -> t
  | None -> failwith ("Unkown SQL type: "^type_name)

let schema_from_type_list db_type type_list =
  let rec make_schema = function
    | t::[] -> type_from_name db_type t
    | t::l -> Dyntype.add (type_from_name db_type t) (make_schema l)
    | [] -> failwith "Cannot make schema for an empty type list"
  in
  make_schema type_list


let get_schema conx table_name =
  let query = match conx.db_type with
    | POSTGRESQL | MARIADB ->
      "SELECT column_name, data_type FROM information_schema.columns WHERE table_name = '"^table_name^"'"
    | SQLITE3 ->
      "SELECT name, type FROM pragma_table_info('"^table_name^"')"
  in
  match%lwt collect_query conx query Caqti_type.(tup2 string string) with
  | [] -> Lwt.return_none
  | schema -> List.split schema
              |> (fun (names, type_list) -> names, schema_from_type_list conx.db_type type_list)
              |> Lwt.return_some


let default_key_size = 3072   (* Note: this is max key size in MariaDB *)

let default_val_size = 1024*1024


let kv_table_schema = "k"::"v"::"e"::"t"::[], Dyntype.(add string (add string (add string string)))

let create_kv_table conx table_name props =
  let open Apero.Option.Infix in
  let key_size = Properties.get Be_sql_properties.Key.key_size props >>= int_of_string_opt >?= default_key_size in
  let query = "CREATE TABLE "^table_name^" (k VARCHAR("^(string_of_int key_size)^") NOT NULL PRIMARY KEY, v TEXT, e VARCHAR(10), t VARCHAR(60))" in
  let open Apero.LwtM.InfixM in
  exec_query conx query >> lazy (Lwt.return kv_table_schema)

let get_timestamp_kv_table conx table_name key =
  let query = "SELECT t FROM "^table_name^" WHERE k = "^key in
  match%lwt collect_query conx query Caqti_type.string with
  | [] -> Lwt.return_none
  | t::_ -> Lwt.return_some t

let trunc_table conx table_name =
  let query = "TRUNCATE TABLE "^table_name in
  fun () -> exec_query conx query

let drop_table conx table_name =
  let query = "DROP TABLE "^table_name in
  fun () -> exec_query conx query

let get conx table_name ?(columns="*") ?condition (Dyntype.Pack (typ, typ_to_s)) =
  let open Lwt.Infix in
  let query = match condition with
    | Some cond -> Printf.sprintf "SELECT %s FROM %s WHERE %s" columns table_name cond
    | None -> Printf.sprintf "SELECT %s FROM %s" columns table_name
  in
  fun () -> collect_query conx query typ >|= (fun rows -> List.map (fun row -> typ_to_s row) rows)


let put conx table_name (col_names, _) values =
  let query = match conx.db_type with
    | MARIADB | SQLITE3 -> 
      Printf.sprintf "REPLACE INTO %s VALUES (%s)" table_name (String.concat "," values)
    | POSTGRESQL -> 
      let open List in
      let kv_list = combine col_names values |> map (fun (c,v) -> c^"="^v) |> String.concat "," in
      Printf.sprintf "INSERT INTO %s VALUES (%s) ON CONFLICT ON CONSTRAINT %s_pkey DO UPDATE SET %s"
      table_name (String.concat "," values) table_name kv_list
  in
  fun () -> exec_query conx query

let remove conx table_name condition =
  let query = "DELETE FROM "^table_name^" WHERE "^condition in
  fun () -> exec_query conx query


