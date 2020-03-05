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
module PluginsArgs = Map.Make(String)

let add_plugin_arg plugin arg pluginargs =
  match PluginsArgs.find_opt plugin pluginargs with
  | None -> PluginsArgs.add plugin [arg] pluginargs
  | Some args -> PluginsArgs.add plugin (arg::args) pluginargs

let sep = Filename.dir_sep
let sepchar = String.get sep 0
let exe_dir = Filename.dirname Sys.executable_name

let plugin_filenames plugin = 
  [
    plugin;
    plugin ^ ".cmxs";
    "zenoh-plugin-" ^ plugin ^ ".cmxs";
  ] 

let plugin_locations = 
  [
    "";
    exe_dir ^ sep ^ ".." ^ sep ^ "lib" ^ sep;
    "~/.zenoh/lib/";
    "/usr/local/lib/";
    "/usr/lib/";
  ]

let lookup_plugin plugin = 
  List.map (fun plugin_location -> 
    List.map (fun plugin_filename -> plugin_location ^ plugin_filename)
             (plugin_filenames plugin))
    plugin_locations
  |> List.flatten 
  |> List.find_opt Sys.file_exists

let plugin_default_dirs = 
  [
    exe_dir ^ sep ^ ".." ^ sep ^ "lib";
    "~/.zenoh/lib/";
    "/usr/local/lib/";
    "/usr/lib/";
  ]

let lookup_default_plugins () = 
  let rec lookup dirs = 
    match dirs with 
    | [] -> None
    | dir::dirs -> 
      match Sys.file_exists dir with 
      | true -> (Sys.readdir dir |> Array.to_list
                |> List.filter (fun file -> String.length file > 18 
                                         && String.equal (Str.last_chars file 5) ".cmxs"
                                         && String.equal (Str.first_chars file 13) "zenoh-plugin-")
                |> List.map (fun file -> dir ^ sep ^file)
                |> function 
                | [] -> lookup dirs 
                | files -> Some(files))
      | false -> lookup dirs
  in
  match lookup plugin_default_dirs with
  | None -> []
  | Some ls -> ls
  

let plugin_name_of_file f =
  String.split_on_char sepchar f |> List.rev |> List.hd 
  |> (function str -> Apero.Astring.is_prefix ~affix:"zenoh-plugin-" str |> function 
  | true ->  String.sub str 13 (String.length str - 13)
  | false -> str )
  |> (function str -> Apero.Astring.is_suffix ~affix:".cmxs" str |> function 
  | true ->  String.sub str 0 (String.length str - 5)
  | false -> str)

let load_plugins plugins plugins_args = 
    match plugins with 
    | ["None"] -> ()
    | ["none"] -> ()
    | plugins -> 
        let plugins = match plugins with 
        | [] -> lookup_default_plugins ()
        | plugins -> plugins in
        Lwt_list.iter_p (fun plugin ->
        let args = String.split_on_char ' ' plugin |> Array.of_list in
        (try
            match lookup_plugin args.(0) with
            | Some plugin ->
              let plugin_name = plugin_name_of_file plugin in
              let args = match PluginsArgs.find_opt plugin_name plugins_args with
                | Some pargs -> Array.append args (Array.of_list pargs)
                | None -> args
              in
              Logs.info (fun m -> m "Loading plugin '%s' from '%s' with args: '%s'..." plugin_name plugin (String.concat " " @@ Array.to_list args));
              Dynload.loadfile plugin args
            | None -> Logs.warn (fun m -> m "Unable to find plugin %s !" plugin)
        with e -> Logs.warn (fun m -> m "Unable to load plugin %s ! Error: %s" plugin (Printexc.to_string e)));
        Lwt.return_unit
        ) plugins |> Lwt.ignore_result