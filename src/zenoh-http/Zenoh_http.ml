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
open Lwt.Infix
open Httpaf
open Httpaf_lwt_unix

module HLC = Apero_time.HLC.Make (Apero_time.Clock_unix)

let zwrite_kind_put = 0L
let zwrite_kind_update = 1L
let zwrite_kind_remove = 2L
let empty_buf = Abuf.create 0

let encoding_raw = 0x00L
let encoding_string = 0x02L
let encoding_json = 0x04L

let timestamp0 = HLC.Timestamp.create 
  (Option.get @@ Uuid.of_string "00000000-0000-0000-0000-000000000000")
  (Option.get @@ HLC.Timestamp.Time.of_string "0")

let respond ?(body="") ?(headers=Headers.empty) ?(status=`OK) reqd =
  let headers = Headers.add headers "content-length" (String.length body |> string_of_int) in
  let headers = Headers.add headers "Access-Control-Allow-Origin" "*" in
  Reqd.respond_with_string reqd (Response.create ~headers status) body

let respond_file path reqd = 
  try respond ~body:(OCamlRes.Res.find (OCamlRes.Path.of_string path) Resources.root) reqd; true
  with Not_found -> 
  try respond ~body:(OCamlRes.Res.find (OCamlRes.Path.of_string (path^"/index.html")) Resources.root) reqd; true
  with Not_found -> false

let respond_internal_error reqd error = respond reqd ~status:`Internal_server_error ~body:
  ("INTERNAL ERROR: "^error)

let respond_unsupported reqd meth path = respond reqd ~status:`Bad_request ~body:
  ("Operation "^(Method.to_string meth)^" not supported on path: "^path)


let json_of_results (results : (string * Abuf.t * Ztypes.data_info) list) =
  let open Ztypes in
  (* We assume the value can be decoded as a string *)
  let read_all_bytes buf = Abuf.read_bytes (Abuf.readable_bytes buf) buf in
  let string_of_buf = Apero.compose Bytes.to_string read_all_bytes in
  let json_of_value value encoding =
    match encoding with
    | Some e when e=encoding_json -> value
    | _ -> Printf.sprintf "\"%s\"" value
  in
  results
  |> List.map (fun (resname, buf, info) ->
      let time = match info.ts with
        | None -> "None"
        | Some ts when ts=timestamp0 -> "None"
        | Some ts -> Timestamp.Time.to_rfc3339 @@ Timestamp.get_time ts
      in
      Printf.sprintf "{ \"key\": \"%s\",\n  \"value\": %s,\n  \"time\": \"%s\" }"
        resname  (json_of_value (string_of_buf buf) info.encoding) time
      )
  |> String.concat ",\n"
  |> Printf.sprintf "[\n%s\n]"


let on_body_read_complete body (action:Abuf.t -> unit) =
  let rec on_read buffer chunk ~off ~len =
    let chunk = Bigstringaf.substring chunk ~off ~len in
    Abuf.write_bytes (Bytes.of_string chunk) buffer;
    Body.schedule_read body ~on_eof:(on_eof buffer) ~on_read:(on_read buffer)
  and on_eof buffer () = action buffer
  in
  let buffer = Abuf.create ~grow:1024 1024 in
  Body.schedule_read body ~on_eof:(on_eof buffer) ~on_read:(on_read buffer)

let media_type_regex =
  (* RFC6838 Media type format:   type "/" [tree "."] subtype ["+" suffix] *[";" parameter]   *)
  Str.regexp @@ Printf.sprintf "^\\(%s\\)/\\(\\(%s\\)\\.\\)?\\(%s\\)\\(\\+\\(%s\\)\\)?\\(;\\(%s\\)\\)?$"
    "[A-Za-z0-9][-_!#$&^A-Za-z0-9]*" "[A-Za-z0-9][-_!#$&^A-Za-z0-9]*" "[A-Za-z0-9][-_!#$&^A-Za-z0-9]*" "[A-Za-z0-9][-_!#$&^A-Za-z0-9]*" ".+"

let decode_media_type s =
  let matched_group_option n =
    try Str.matched_group n s with Not_found -> ""
  in
  if Str.string_match media_type_regex s 0 then
    (matched_group_option 1) :: (matched_group_option 3) :: (matched_group_option 4) :: (matched_group_option 6) :: (matched_group_option 8) :: []
  else ( Logs.warn (fun m -> m "[Zhttp] Invalid media type: %s (consider value as RAW encoding)" s); [] )

let encoding_of_content_type = function
  | None -> encoding_raw
  | Some s ->
    let l = decode_media_type s in Logs.info (fun m -> m "[Zhttp] media type: %s" (String.concat " / " l));
    match l with
    | [ "text" ; _ ; _ ; _ ; _ ]
    | [ "application" ; _ ; "x-www-form-urlencoded" ; _ ; _ ]
    | [ "application" ; _ ; "xml" ; _ ; _ ]
    | [ "application" ; _ ; "xhtml+xml" ; _ ; _ ]
      -> encoding_string
    | [ "application" ; _ ; "json" ; _ ; _ ]
      -> encoding_json
    | [] -> Logs.warn (fun m -> m "[Zhttp] Invalid media type: %s (consider value as RAW encoding)" s);
      encoding_raw
    | _ -> Logs.debug (fun m -> m "[Zhttp] Unknown media type: %s (default as RAW encoding)" s);
      encoding_raw


let request_handler zenoh zpid (_ : Unix.sockaddr) reqd =
  let req = Reqd.request reqd in
  Logs.debug (fun m -> m "[Zhttp] HTTP req: %a on %s with headers: %a"
                                  Method.pp_hum req.meth req.target
                                  Headers.pp_hum req.headers);
  let resname, predicate = Astring.span ~sat:(fun c -> c <> '?') req.target in
  let resname =
    if Astring.is_prefix ~affix:"/@/router/local" resname
    then "/@/router/"^zpid^(Astring.with_index_range ~first:15 resname)
    else resname
  in
  let predicate = Astring.with_range ~first:1 predicate in
  try begin
      match req.meth with
      | `GET -> begin
        Lwt.async (fun _ ->
          try begin
            (* TODO: manage "accept" header *)
            Logs.debug (fun m -> m "[Zhttp] Zenoh.lquery on %s with predicate: %s" resname predicate);
            Zenoh_net.lquery zenoh ~consolidation:KeepAll resname predicate >|= function
            | [] -> if not (respond_file resname reqd) then respond reqd ~body:"{}"
            | results ->
              Logs.debug (fun m -> m "[Zhttp] Zenoh.lquery received %d key/values" (List.length results));
              respond reqd ~body:(json_of_results results)
          end with
          | exn ->
            respond_internal_error reqd (Printexc.to_string exn); Lwt.return_unit
        )
        end
      | `PUT -> begin
          try begin
            on_body_read_complete (Reqd.request_body reqd) (
              fun buf ->
                Lwt.async (fun _ ->
                  let encoding = encoding_of_content_type @@ Headers.get req.headers "content-type" in
                  Logs.debug (fun m -> m "[Zhttp] Zenoh.write put on %s %d bytes with encoding %Ld" resname (Abuf.readable_bytes buf) encoding);
                  Zenoh_net.write zenoh resname buf ~kind:zwrite_kind_put ~encoding >|= fun _ ->
                  respond reqd ~status:`No_content)
            )
          end with
          | exn ->
            respond_internal_error reqd (Printexc.to_string exn)
        end
      | `Other m when m = "PATCH" -> begin
          try begin
            on_body_read_complete (Reqd.request_body reqd) (
              fun buf ->
                Lwt.async (fun _ ->
                  let encoding = encoding_of_content_type @@ Headers.get req.headers "content-type" in
                  Logs.debug (fun m -> m "[Zhttp] Zenoh.write update on %s %d bytes with encoding %Ld" resname (Abuf.readable_bytes buf) encoding);
                  Zenoh_net.write zenoh resname buf ~kind:zwrite_kind_update ~encoding >|= fun _ ->
                  respond reqd ~status:`No_content)
            )
          end with
          | exn ->
            respond_internal_error reqd (Printexc.to_string exn)
        end
      | `DELETE -> begin
        Lwt.async (fun _ ->
          try begin
            Logs.debug (fun m -> m "[Zhttp] Zenoh_net.write remove on %s" resname);
            Zenoh_net.write zenoh resname empty_buf ~kind:zwrite_kind_remove >|= fun _ ->
            respond reqd ~status:`No_content
          end with
          | exn ->
            respond_internal_error reqd (Printexc.to_string exn); Lwt.return_unit
        )
        end
      | _ -> respond_unsupported reqd req.meth resname
  end with
  | exn ->
    Logs.err (fun m -> m "Exception %s raised:\n%s" (Printexc.to_string exn) (Printexc.get_backtrace ()));
    raise exn


let error_handler _ (_ : Unix.sockaddr) ?request:_ error start_response =
  let response_body = start_response Headers.empty in
  begin match error with
  | `Exn exn ->
    Logs.debug (fun m -> m "[Zhttp] error_handler: %s\n%s" (Printexc.to_string exn) (Printexc.get_backtrace ()));
    Body.write_string response_body "INTERNAL SERVER ERROR:\n";
    Body.write_string response_body (Printexc.to_string exn);
    Body.write_string response_body "\n";
  | #Status.standard as error ->
    Logs.debug (fun m -> m "[Zhttp] error_handler: #Status.standard \n%s" (Printexc.get_backtrace ()));
    Body.write_string response_body "INTERNAL SERVER ERROR:\n";
    Body.write_string response_body (Status.default_reason_phrase error)
  end;
  Body.close_writer response_body

let run port =
  let listen_address = Unix.(ADDR_INET (inet_addr_any, port)) in
  let%lwt zns = Zenoh_net.zopen "" in
  let zprops = Zenoh_net.info zns in
  let zpid = match Properties.get "peer_pid" zprops with
    | Some pid -> pid
    | None -> Uuid.make () |> Uuid.to_string
  in
  Lwt_io.establish_server_with_client_socket listen_address 
    (Server.create_connection_handler ~request_handler:(request_handler zns zpid) ~error_handler:(error_handler zns))
  >|= fun _ ->
  Zenoh_net.evaluate zns ("/@/router/" ^ zpid ^ "/plugin/http")  (fun _ _ -> 
    let data = Abuf.create ~grow:65536 1024 in 
    let locators = Aunix.inet_addrs_up_nolo () 
      |> List.map (fun addr -> `String (Printf.sprintf "http://%s:%d" (Unix.string_of_inet_addr addr) port)) in
    let json = `Assoc [ ("locators",  `List locators); ] in
    Abuf.write_bytes (Bytes.unsafe_of_string (Yojson.Safe.to_string json)) data;
    let info = Ztypes.({srcid=None; srcsn=None; bkrid=None; bkrsn=None; ts=Some(timestamp0); encoding=Some 4L (* JSON *); kind=None}) in
    Lwt.return [("/@/router/" ^ zpid ^ "/plugin/http", data, info)]
  )
  >|= fun _ ->
  Logs.info (fun m -> m "[Zhttp] listening on port tcp/0.0.0.0:%d" port)

let port = Cmdliner.Arg.(value & opt int 8000 & info ["h"; "httpport"] ~docv:"HTTPPORT" ~doc:"Listening http port")

let _ = 
  Logs.debug (fun m -> m "[Zhttp] starting with args: %s" (Array.to_list Sys.argv |> String.concat " "));
  match Cmdliner.Term.(eval ~argv:Sys.argv (const run $ port, Cmdliner.Term.info "zenoh-http")) with
  | `Ok _ -> ()
  | `Help -> exit 0
  | `Error `Parse ->
    Logs.err (fun m -> m "Error parsing zenoh-http options: %s" (Array.to_list Sys.argv |> String.concat " "));
    exit 1
  | `Error `Term -> exit 2 (* by default term eval error is written to err by Term.eval *)
  | `Error `Exn -> exit 3 (* by default exception is caught and written to err by Term.eval *)
  | _ -> exit 4
